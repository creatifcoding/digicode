/**
 * Jcode codemode sidecar (protocol v2).
 *
 * Reads one JSON request from stdin:
 *   { protocol_version, id, source, inputs, history?, store_root, limits }
 *
 * Boots an AgentOS VM with:
 *   - a durable chunked-local mount at /data backed by store_root on the host
 *     (write-through: guest writes land in host storage immediately),
 *   - the bundled codemode engine installed into the guest filesystem,
 *   - deny-by-default network/child-process/binding permissions.
 *
 * Inside the guest, a bootstrap assembles the metatool engine over guest
 * node:sqlite at /data/store.db, exposes the live `mt` Proxy (original
 * eval-child.ts contract), evaluates the user program via new Function, and
 * sanitizes the result with the ported clone-safe sanitizer.
 *
 * Writes one JSON response to stdout:
 *   { protocol_version, id, duration_ms, result | error }
 */
import { AgentOs } from "@rivet-dev/agentos-core";
import { chunkedLocalMountPlugin } from "@rivet-dev/agentos-runtime-core/descriptors";
import { readFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const PROTOCOL_VERSION = 2;
const here = dirname(fileURLToPath(import.meta.url));

const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const request = JSON.parse(Buffer.concat(chunks).toString("utf8"));
const started = Date.now();

const respond = (payload) => {
  process.stdout.write(
    JSON.stringify({
      protocol_version: PROTOCOL_VERSION,
      id: request.id,
      duration_ms: Date.now() - started,
      ...payload,
    }),
  );
};

let runtime;
try {
  const storeRoot = request.store_root;
  if (typeof storeRoot !== "string" || storeRoot.length === 0) {
    throw new Error("store_root is required");
  }
  mkdirSync(join(storeRoot, "blocks"), { recursive: true });

  const engineSource = readFileSync(join(here, "guest-engine.mjs"), "utf8");

  runtime = await AgentOs.create({
    // Virtual guest identity. The durable mount root is guest-owned by 0:0;
    // this is virtual-Linux identity inside the sandbox, not host authority.
    user: { uid: 0, gid: 0 },
    mounts: [
      {
        path: "/data",
        plugin: chunkedLocalMountPlugin({
          metadataPath: join(storeRoot, "metadata.db"),
          blockRoot: join(storeRoot, "blocks"),
        }),
      },
    ],
    permissions: {
      fs: "allow",
      network: "deny",
      childProcess: {
        default: "deny",
        rules: [{ mode: "allow", operations: ["spawn"], patterns: ["node"] }],
      },
      process: "allow",
      env: "allow",
      binding: "deny",
    },
    limits: {
      resources: {
        maxProcesses: 8,
        maxOpenFds: 64,
        maxSockets: 0,
        maxFilesystemBytes: 256 * 1024 * 1024,
        maxInodeCount: 16384,
      },
      jsRuntime: {
        v8HeapLimitMb: request.limits.heap_mb,
        cpuTimeLimitMs: request.limits.cpu_time_ms,
        wallClockLimitMs: request.limits.wall_time_ms,
      },
    },
  });

  await runtime.filesystem.writeFiles([
    { path: "/opt/jcode-mt/engine.mjs", content: engineSource },
    {
      path: "/opt/jcode-mt/run.mjs",
      content: `
	import { createMetatool, sqliteNodeLayer, NodeFileSystemLayer, sanitizeForToolPayload, stringifyForToolContent, metatoolPlugin } from "/opt/jcode-mt/engine.mjs";

	const HISTORY_COLLECTION = "jcode.session";
	const HISTORY_KEY = "mt-history";
	const HISTORY_LIMIT = 50;
	const RESULT_TRUNCATE_LENGTH = 500;

	function truncateResult(value) {
	  const text = typeof value === "string" ? value : stringifyForToolContent(value);
	  return text.length > RESULT_TRUNCATE_LENGTH ? text.slice(0, RESULT_TRUNCATE_LENGTH - 3) + "..." : text;
	}

	function disabledProvider(name) {
	  return async () => {
	    throw new Error("mt." + name + " is disabled in native codemode until provider authority is brokered");
	  };
	}

	function createArtifactsCapability(config, effects) {
	  if (!config || typeof config !== "object") return undefined;
	  const mode = config.mode === "apply" ? "apply" : "off";
	  const catalogSnapshot = config.catalog && typeof config.catalog === "object"
	    ? config.catalog
	    : { artifacts: [], candidates: [] };
	  let admitted = false;
	  const limited = (items, limit) => Array.isArray(items)
	    ? items.slice(0, Math.max(0, Math.min(Number(limit ?? 100), 200)))
	    : [];
	  const clone = (value) => JSON.parse(JSON.stringify(value));
	  const assertText = (name, value) => {
	    if (typeof value !== "string" || value.length === 0) {
	      throw new Error("artifact bundle " + name + " must be a non-empty string");
	    }
	    return value;
	  };
	  const normalizeBundle = (bundle) => {
	    if (!bundle || typeof bundle !== "object") throw new Error("artifact bundle must be an object");
	    const normalized = {
	      key: assertText("key", bundle.key),
	      title: assertText("title", bundle.title),
	      source: assertText("source", bundle.source),
	      rendered: assertText("rendered", bundle.rendered),
	    };
	    if (bundle.annotation != null) normalized.annotation = assertText("annotation", bundle.annotation);
	    if (bundle.templateKey != null) normalized.templateKey = assertText("templateKey", bundle.templateKey);
	    return normalized;
	  };
	  return Object.freeze({
	    mode,
	    status: () => ({
	      mode,
	      counts: {
	        artifacts: Array.isArray(catalogSnapshot.artifacts) ? catalogSnapshot.artifacts.length : 0,
	        candidates: Array.isArray(catalogSnapshot.candidates) ? catalogSnapshot.candidates.length : 0,
	      },
	      limit: catalogSnapshot.limit ?? 200,
	    }),
	    catalog: (limit = 100) => ({
	      ...clone(catalogSnapshot),
	      artifacts: limited(catalogSnapshot.artifacts, limit),
	      candidates: limited(catalogSnapshot.candidates, limit),
	    }),
	    candidates: (limit = 100) => limited(catalogSnapshot.candidates, limit).map(clone),
	    admitBundle: async (bundle) => {
	      if (mode !== "apply") throw new Error("mt.artifacts.admitBundle requires artifact_mode apply");
	      if (admitted) throw new Error("mt.artifacts permits one bundle admission per evaluation");
	      const effect = {
	        capability: "artifacts",
	        operation: "admit_bundle",
	        input: normalizeBundle(bundle),
	      };
	      effects.push(effect);
	      admitted = true;
	      return { queued: true, mode, effectIndex: effects.length - 1 };
	    },
	  });
	}

	function createTaskerCapability(config, effects) {
	  if (!config || typeof config !== "object") return undefined;
	  const mode = config.mode === "apply" ? "apply" : "plan";
	  const snapshot = config.snapshot && typeof config.snapshot === "object"
	    ? config.snapshot
	    : { tasks: [], features: [], ready: [] };
	  const expectedSnapshotHash = typeof config.snapshot_hash === "string"
	    ? config.snapshot_hash
	    : null;
	  const projectId = typeof config.project_id === "string" ? config.project_id : null;
	  let reconciled = false;

	  const bounded = (value, fallback = 100) => Math.max(
	    0,
	    Math.min(Number.isFinite(Number(value)) ? Number(value) : fallback, 500),
	  );
	  const limited = (items, limit = 100) => Array.isArray(items)
	    ? items.slice(0, bounded(limit))
	    : [];
	  const clone = (value) => JSON.parse(JSON.stringify(value));
	  const tasks = Array.isArray(snapshot.tasks) ? snapshot.tasks : [];
	  const features = Array.isArray(snapshot.features) ? snapshot.features : [];
	  const dependencies = Array.isArray(snapshot.dependencies) ? snapshot.dependencies : [];
	  const featureDependencies = Array.isArray(snapshot.featureDependencies)
	    ? snapshot.featureDependencies
	    : [];
	  const taskNotes = Array.isArray(snapshot.taskNotes) ? snapshot.taskNotes : [];
	  const featureNotes = Array.isArray(snapshot.featureNotes) ? snapshot.featureNotes : [];
	  const ready = Array.isArray(snapshot.ready) ? snapshot.ready : [];
	  const projections = snapshot.projections && typeof snapshot.projections === "object"
	    ? snapshot.projections
	    : {};
	  const concurrencySnapshot = snapshot.concurrency && typeof snapshot.concurrency === "object"
	    ? snapshot.concurrency
	    : {};
	  const concurrencyProjection = concurrencySnapshot.projection && typeof concurrencySnapshot.projection === "object"
	    ? concurrencySnapshot.projection
	    : {};

	  const resolveTask = (reference) => {
	    if (reference == null) return undefined;
	    const value = String(reference);
	    return tasks.find((task) => task?.id === value
	      || String(task?.displayId ?? "") === value
	      || ("#" + String(task?.displayId ?? "")) === value);
	  };
	  const resolveFeature = (reference) => {
	    if (reference == null) return undefined;
	    const value = String(reference);
	    return features.find((feature) => feature?.id === value
	      || String(feature?.displayId ?? "") === value
	      || ("F" + String(feature?.displayId ?? "")) === value
	      || ("#F" + String(feature?.displayId ?? "")) === value);
	  };
	  const taskProjection = (name, limit) => {
	    const value = projections[name];
	    if (!value || typeof value !== "object") return { limit: bounded(limit), truncated: false };
	    const result = clone(value);
	    result.limit = bounded(limit);
	    for (const key of ["nodes", "edges", "tasks", "features", "dependencies", "featureDependencies", "roots", "branches"]) {
	      if (Array.isArray(result[key])) result[key] = limited(result[key], limit);
	    }
	    return result;
	  };

	  const show = (reference) => {
	    const task = resolveTask(reference);
	    if (!task) return null;
	    const taskId = task.id;
	    return {
	      task: clone(task),
	      readiness: { ready: ready.some((candidate) => candidate?.id === taskId) },
	      dependencies: limited(dependencies.filter((dependency) => dependency?.taskId === taskId), 500),
	      notes: limited(taskNotes.filter((note) => note?.taskId === taskId), 500),
	    };
	  };

	  const search = (query, options = {}) => {
	    const needle = String(query ?? "").toLowerCase();
	    const matches = tasks.filter((task) => {
	      const text = (String(task?.title ?? "") + "\\n" + String(task?.description ?? "")).toLowerCase();
	      return text.includes(needle) && (!options.state || task?.state === options.state);
	    });
	    return limited(matches, options.limit);
	  };

	  const featureStatus = (reference) => {
	    const feature = resolveFeature(reference);
	    if (!feature) return null;
	    const children = features.filter((candidate) => candidate?.parentFeatureId === feature.id);
	    const ownedTasks = tasks.filter((task) => task?.featureId === feature.id);
	    const subtree = new Set([feature.id]);
	    let changed = true;
	    while (changed) {
	      changed = false;
	      for (const candidate of features) {
	        if (candidate?.parentFeatureId && subtree.has(candidate.parentFeatureId) && !subtree.has(candidate.id)) {
	          subtree.add(candidate.id);
	          changed = true;
	        }
	      }
	    }
	    const subtreeTasks = tasks.filter((task) => subtree.has(task?.featureId));
	    const done = ownedTasks.filter((task) => task?.state === "done").length;
	    const subtreeDone = subtreeTasks.filter((task) => task?.state === "done").length;
	    const gates = Array.isArray(feature.gates) ? feature.gates : [];
	    return {
	      feature: clone(feature),
	      tasks: limited(ownedTasks, 500),
	      childFeatures: limited(children, 500),
	      subtree: { featureIds: Array.from(subtree).slice(0, 500), tasks: limited(subtreeTasks, 500) },
	      progress: { doneTasks: done, totalTasks: ownedTasks.length, ratio: ownedTasks.length ? done / ownedTasks.length : 0 },
	      rollupProgress: { doneTasks: subtreeDone, totalTasks: subtreeTasks.length, ratio: subtreeTasks.length ? subtreeDone / subtreeTasks.length : 0 },
	      gateProgress: { passed: gates.filter((gate) => gate?.status === "passed").length, total: gates.length },
	      dependencies: limited(featureDependencies.filter((dependency) => dependency?.featureId === feature.id), 500),
	      notes: limited(featureNotes.filter((note) => note?.featureId === feature.id), 500),
	    };
	  };

	  const topologyPaths = (options = {}) => {
	    const domain = options.domain === "feature" ? "feature" : "task";
	    const from = domain === "feature" ? resolveFeature(options.from) : resolveTask(options.from);
	    const to = domain === "feature" ? resolveFeature(options.to) : resolveTask(options.to);
	    if (!from || !to) throw new Error("topologyPaths requires resolvable from and to references");
	    const edges = domain === "feature" ? featureDependencies : dependencies;
	    const adjacency = new Map();
	    for (const edge of edges) {
	      const source = domain === "feature" ? edge?.featureId : edge?.taskId;
	      const target = domain === "feature" ? edge?.dependsOnId : edge?.dependsOnId;
	      if (!adjacency.has(source)) adjacency.set(source, []);
	      adjacency.get(source).push(target);
	    }
	    const maxDepth = Math.max(1, Math.min(Number(options.maxDepth ?? 8), 64));
	    const maxPaths = Math.max(1, Math.min(Number(options.maxPaths ?? 5), 100));
	    const paths = [];
	    const mode = options.mode === "all_up_to_depth" ? "all_up_to_depth" : "shortest";
	    const walk = (current, path, seen) => {
	      if (paths.length >= maxPaths || path.length > maxDepth + 1) return;
	      if (current === to.id) {
	        paths.push(path.slice());
	        return;
	      }
	      for (const next of adjacency.get(current) ?? []) {
	        if (seen.has(next)) continue;
	        seen.add(next);
	        path.push(next);
	        walk(next, path, seen);
	        path.pop();
	        seen.delete(next);
	        if (mode === "shortest" && paths.length > 0) return;
	      }
	    };
	    walk(from.id, [from.id], new Set([from.id]));
	    return { domain, mode, from: { id: from.id }, to: { id: to.id }, maxDepth, maxPaths, foundPaths: paths.length, truncated: mode === "all_up_to_depth" && paths.length >= maxPaths, paths };
	  };

	  const featureChildren = (options = {}) => {
	    const root = resolveFeature(options.featureId ?? options.feature);
	    if (!root) throw new Error("featureChildren requires a resolvable feature reference");
	    const depth = Math.max(0, Math.min(Number(options.depth ?? 1), 64));
	    const limit = bounded(options.limit);
	    const entries = [];
	    const queue = [[root.id, 0]];
	    while (queue.length > 0 && entries.length < limit) {
	      const [id, currentDepth] = queue.shift();
	      const children = features.filter((feature) => feature?.parentFeatureId === id);
	      if (currentDepth < depth) for (const child of children) queue.push([child.id, currentDepth + 1]);
	      entries.push({ parentFeatureId: id, depth: currentDepth, featureChildren: limited(children, limit), taskChildren: options.includeTasks ? limited(tasks.filter((task) => task?.featureId === id), limit) : [], expanded: currentDepth < depth });
	    }
	    return { root: { id: root.id }, depthLimit: depth, includeTasks: options.includeTasks === true, entries, returnedEntries: entries.length, limit, truncated: entries.length >= limit };
	  };

	  const taskNeighbors = (options = {}) => {
	    const origin = resolveTask(options.taskId ?? options.task);
	    if (!origin) throw new Error("taskNeighbors requires a resolvable task reference");
	    const direction = ["upstream", "downstream", "both"].includes(options.direction) ? options.direction : "both";
	    const depth = Math.max(0, Math.min(Number(options.depth ?? 1), 64));
	    const limit = bounded(options.limit);
	    const nodes = new Set([origin.id]);
	    const edges = [];
	    const queue = [[origin.id, 0]];
	    while (queue.length > 0 && nodes.size < limit) {
	      const [id, currentDepth] = queue.shift();
	      if (currentDepth >= depth) continue;
	      for (const dependency of dependencies) {
	        let candidate;
	        let kind;
	        if ((direction === "upstream" || direction === "both") && dependency?.taskId === id) {
	          candidate = dependency.dependsOnId; kind = "upstream";
	        } else if ((direction === "downstream" || direction === "both") && dependency?.dependsOnId === id) {
	          candidate = dependency.taskId; kind = "downstream";
	        }
	        const task = resolveTask(candidate);
	        if (!task || (options.includeDone !== true && task.state === "done")) continue;
	        edges.push({ kind, from: { id }, to: { id: task.id } });
	        if (!nodes.has(task.id)) { nodes.add(task.id); queue.push([task.id, currentDepth + 1]); }
	        if (nodes.size >= limit) break;
	      }
	    }
	    return { origin: { id: origin.id }, direction, depthLimit: depth, includeDone: options.includeDone === true, nodes: limited(tasks.filter((task) => nodes.has(task.id)), limit), edges: limited(edges, limit), nodeCount: Math.min(nodes.size, limit), limit, truncated: nodes.size > limit };
	  };

	  const reconciliationKinds = [
	    "batch", "plan", "feature_plan", "create", "update", "set_state", "add_dependency", "add_note",
	    "task_index", "create_feature", "feature_update", "resolve_feature_gate", "set_dependencies",
	    "set_feature_dependencies", "link", "unlink", "create_candidate_set", "register_candidate",
	    "submit_candidate", "set_candidate_set_state", "record_round", "record_ballot", "prepare_promotion",
	    "mark_promotion_ref_updated", "finalize_promotion", "abort_promotion", "rollback_promotion",
	    "recover_promotion", "resume_promotion",
	  ];
	  const reconcile = async (kind, payload) => {
	    if (reconciled) throw new Error("mt.tasker permits one atomic reconciliation per evaluation");
	    if (!reconciliationKinds.includes(kind)) {
	      throw new Error("unsupported Tasker reconciliation kind: " + kind);
	    }
	    if (!payload || typeof payload !== "object") {
	      throw new Error("Tasker reconciliation payload must be an object");
	    }
	    const effect = {
	      capability: "tasker",
	      operation: "reconcile",
	      input: {
	        kind,
	        payload: clone(payload),
	        mode,
	        expected_snapshot_hash: expectedSnapshotHash,
	        project_id: projectId,
	      },
	    };
	    effects.push(effect);
	    reconciled = true;
	    return {
	      queued: true,
	      mode,
	      kind,
	      expectedSnapshotHash,
	      effectIndex: effects.length - 1,
	    };
	  };

	  return Object.freeze({
	    mode,
	    snapshotHash: expectedSnapshotHash,
	    projectId,
	    snapshot: () => clone(snapshot),
	    status: () => ({
	      mode,
	      snapshotHash: expectedSnapshotHash,
	      counts: snapshot.counts ?? {},
	      truncated: snapshot.truncated === true,
	      projectId,
	      lifecycle: clone(snapshot.lifecycle ?? {}),
	    }),
	    list: (options = {}) => {
	      const state = options.state;
	      const filtered = state ? tasks.filter((task) => task?.state === state) : tasks;
	      return limited(filtered, options.limit);
	    },
	    show,
	    search,
	    ready: (limit = 100) => limited(ready, limit),
	    features: (options = {}) => {
	      const state = options.state;
	      const filtered = state ? features.filter((feature) => feature?.state === state) : features;
	      return limited(filtered, options.limit);
	    },
	    feature: featureStatus,
	    dependencies: (options = {}) => limited(dependencies.filter((dependency) => !options.taskId || dependency?.taskId === resolveTask(options.taskId)?.id), options.limit),
	    featureDependencies: (options = {}) => limited(featureDependencies.filter((dependency) => !options.featureId || dependency?.featureId === resolveFeature(options.featureId)?.id), options.limit),
	    notes: (options = {}) => limited([...taskNotes, ...featureNotes].filter((note) => (!options.taskId || note?.taskId === resolveTask(options.taskId)?.id) && (!options.featureId || note?.featureId === resolveFeature(options.featureId)?.id)), options.limit),
	    taskGraph: (limit = 100) => taskProjection("taskGraph", limit),
	    taskStructure: (limit = 100) => taskProjection("taskStructure", limit),
	    topologySummary: (limit = 100) => taskProjection("topologySummary", limit),
	    topologyAnomalies: (limit = 100) => taskProjection("topologyAnomalies", limit),
	    topologyPaths,
	    topologyFrontier: (limit = 100) => taskProjection("topologyFrontier", limit),
	    featureChildren,
	    taskNeighbors,
	    featureTree: (options = {}) => taskProjection("featureTree", options.limit),
	    resolveTask: (reference) => clone(resolveTask(reference) ?? null),
	    resolveFeature: (reference) => clone(resolveFeature(reference) ?? null),
	    featureGates: (reference) => clone(resolveFeature(reference)?.gates ?? []),
	    featureGate: (reference, index) => clone((resolveFeature(reference)?.gates ?? [])[Number(index)] ?? null),
	    lifecycle: () => clone(snapshot.lifecycle ?? {}),
	    policy: () => clone(concurrencySnapshot.policy ?? null),
	    concurrency: (limit = 100) => ({
	      projectId: concurrencySnapshot.projectId ?? projectId,
	      schemaVersion: concurrencySnapshot.schemaVersion ?? null,
	      revision: concurrencySnapshot.revision ?? 0,
	      candidateSets: limited(concurrencyProjection.candidateSets, limit),
	      candidates: limited(concurrencyProjection.candidates, limit),
	      rounds: limited(concurrencyProjection.adjudicationRounds, limit),
	      ballots: limited(concurrencyProjection.adjudicationBallots, limit),
	      promotions: limited(concurrencyProjection.promotionIntents, limit),
	      counts: clone(concurrencyProjection.counts ?? {}),
	      limit: bounded(limit),
	      truncated: concurrencyProjection.truncated === true,
	    }),
	    candidateSets: (limit = 100) => limited(concurrencyProjection.candidateSets, limit),
	    candidates: (limit = 100) => limited(concurrencyProjection.candidates, limit),
	    rounds: (limit = 100) => limited(concurrencyProjection.adjudicationRounds, limit),
	    promotions: (limit = 100) => limited(concurrencyProjection.promotionIntents, limit),
	    candidateSet: (id) => clone((concurrencyProjection.candidateSets ?? []).find((value) => value?.id === id) ?? null),
	    candidate: (id) => clone((concurrencyProjection.candidates ?? []).find((value) => value?.id === id) ?? null),
	    round: (id) => clone((concurrencyProjection.adjudicationRounds ?? []).find((value) => value?.id === id) ?? null),
	    promotion: (id) => clone((concurrencyProjection.promotionIntents ?? []).find((value) => value?.id === id) ?? null),
	    reconcile: async (program) => reconcile(program?.kind, program?.payload),
	    batch: async (operations) => reconcile("batch", { operations }),
	    plan: async (tasks) => reconcile("plan", { tasks }),
	    featurePlan: async (feature) => reconcile("feature_plan", { feature }),
	    create: async (task) => reconcile("create", task),
	    update: async (task) => reconcile("update", task),
	    setState: async (taskId, state, options = {}) => reconcile("set_state", { ...options, taskId, state }),
	    addDependency: async (taskId, dependsOnTaskId) => reconcile("add_dependency", { taskId, dependsOnTaskId }),
	    addNote: async (note) => reconcile("add_note", note),
	    taskIndex: async (input) => reconcile("task_index", input),
	    createFeature: async (feature) => reconcile("create_feature", feature),
	    updateFeature: async (feature) => reconcile("feature_update", feature),
	    resolveFeatureGate: async (input) => reconcile("resolve_feature_gate", input),
	    setDependencies: async (input) => reconcile("set_dependencies", input),
	    setFeatureDependencies: async (input) => reconcile("set_feature_dependencies", input),
	    link: async (taskId, featureId) => reconcile("link", { taskId, featureId }),
	    unlink: async (taskId) => reconcile("unlink", { taskId }),
	    reconcileConcurrency: async (kind, payload) => reconcile(kind, payload),
	    createCandidateSet: async (candidateSet, expectedRevision) => reconcile("create_candidate_set", { candidateSet, expectedRevision }),
	    registerCandidate: async (candidate, expectedRevision) => reconcile("register_candidate", { candidate, expectedRevision }),
	    submitCandidate: async (candidate, evidence, expectedRevision) => reconcile("submit_candidate", { candidate, evidence, expectedRevision }),
	    setCandidateSetState: async (candidateSetId, state, expectedRevision) => reconcile("set_candidate_set_state", { candidateSetId, state, expectedRevision }),
	    recordRound: async (round, expectedRevision) => reconcile("record_round", { round, expectedRevision }),
	    recordBallot: async (ballot, expectedRevision) => reconcile("record_ballot", { ballot, expectedRevision }),
	    preparePromotion: async (intent, expectedRevision) => reconcile("prepare_promotion", { intent, expectedRevision }),
	    markPromotionRefUpdated: async (intentId, observedCommit, expectedRevision) => reconcile("mark_promotion_ref_updated", { intentId, observedCommit, expectedRevision }),
	    finalizePromotion: async (intentId, expectedRevision) => reconcile("finalize_promotion", { intentId, expectedRevision }),
	    abortPromotion: async (intentId, reason, expectedRevision) => reconcile("abort_promotion", { intentId, reason, expectedRevision }),
	    rollbackPromotion: async (intentId, reason, expectedRevision) => reconcile("rollback_promotion", { intentId, reason, expectedRevision }),
	    recoverPromotion: async (intentId, observedCommit, expectedRevision) => reconcile("recover_promotion", { intentId, observedCommit, expectedRevision }),
	    resumePromotion: async (intentId, observedCommit, expectedRevision) => reconcile("resume_promotion", { intentId, observedCommit, expectedRevision }),
	  });
	}

export async function run(request) {
  const consoleLines = [];
  const capture = (...args) => {
    consoleLines.push(args.map((arg) => {
      if (typeof arg === "string") return arg;
      try { return JSON.stringify(arg) ?? String(arg); } catch { return String(arg); }
    }).join(" "));
  };
  const original = { log: console.log, info: console.info, warn: console.warn, error: console.error };
  console.log = capture; console.info = capture; console.warn = capture; console.error = capture;

	  let metatool;
	  const effects = [];
  try {
	    metatool = await createMetatool({
      sqlLayer: sqliteNodeLayer({ filename: "/data/store.db" }),
      fsLayer: NodeFileSystemLayer,
      overlays: [],
	      cwd: "/data",
	    });
	    const api = metatool.getApi();

	    let metatoolOverlayLoaded = false;
	    const bounded = (n, fallback = 10) => Math.max(0, Math.min(Number.isFinite(Number(n)) ? Number(n) : fallback, HISTORY_LIMIT));
	    async function readHistory(n) {
	      const record = await api.get(HISTORY_COLLECTION, HISTORY_KEY).catch(() => undefined);
	      const entries = Array.isArray(record?.entries) ? record.entries : [];
	      return entries.slice(-bounded(n));
	    }
	    async function writeHistory(entry) {
	      const record = await api.get(HISTORY_COLLECTION, HISTORY_KEY).catch(() => undefined);
	      const entries = Array.isArray(record?.entries) ? record.entries : [];
	      entries.push(entry);
	      await api.put(HISTORY_COLLECTION, HISTORY_KEY, {
	        _meta: { summary: "Bounded native MetaTool codemode evaluation history" },
	        entries: entries.slice(-HISTORY_LIMIT),
	      });
	    }
	    async function contextSnapshot() {
	      const collections = await api.collections().catch(() => []);
	      const overlayIds = metatool.overlays?.() ?? [];
	      return {
	        cwd: "/data",
	        project: "agentos-codemode",
	        inputs: request.inputs ?? {},
	        profiles: {
	          current: request.profile ?? "pure",
	          workspaceRead: "blocked until a native capability broker grants authority",
	          workspaceMutate: "blocked until a native capability broker grants authority",
	        },
	        skills: { count: 0, names: [] },
	        overlays: { count: overlayIds.length, ids: overlayIds },
	        collections,
	        history: { limit: HISTORY_LIMIT, count: (await readHistory(HISTORY_LIMIT)).length },
	      };
	    }
	    const tasker = createTaskerCapability(request.capabilities?.tasker, effects);
	    const artifacts = createArtifactsCapability(request.capabilities?.artifacts, effects);
	    const extensionMethods = {
	      inputs: request.inputs,
	      ...(tasker ? { tasker } : {}),
	      ...(artifacts ? { artifacts } : {}),
	      get context() { return contextSnapshot(); },
	      history: readHistory,
	      recordHistory: writeHistory,
	      loadMetatoolPlugin: async () => {
	        if (!metatoolOverlayLoaded) {
	          await metatool.loadOverlay(metatoolPlugin("/data", NodeFileSystemLayer));
	          metatoolOverlayLoaded = true;
	        }
	      },
	      llm: disabledProvider("llm"),
	      llm_batch: disabledProvider("llm_batch"),
	      ask: disabledProvider("ask"),
	    };
    const mt = new Proxy(extensionMethods, {
      get(target, property, receiver) {
        if (Reflect.has(target, property)) return Reflect.get(target, property, receiver);
        return metatool?.getApi()[String(property)];
      },
      has(target, property) {
        return Reflect.has(target, property) || property in (metatool?.getApi() ?? {});
      },
      ownKeys(target) {
        return Array.from(new Set([...Reflect.ownKeys(metatool?.getApi() ?? {}), ...Reflect.ownKeys(target)]));
      },
      getOwnPropertyDescriptor(target, property) {
        return Reflect.getOwnPropertyDescriptor(target, property)
          ?? (property in (metatool?.getApi() ?? {}) ? { enumerable: true, configurable: true } : undefined);
      },
    });

    const fn = new Function("mt", "inputs", '"use strict"; return (async () => { ' + request.source + ' })()');
	    const rawResult = await fn(mt, request.inputs);
    const { value, warnings } = await sanitizeForToolPayload(rawResult, {
      promiseTimeoutMs: request.promise_timeout_ms ?? 10_000,
      maxArrayItems: 200,
      maxObjectKeys: 200,
      maxDepth: 8,
    });
	    await writeHistory({
	      code: request.source,
	      result: truncateResult(value),
	      timestamp: new Date().toISOString(),
	    }).catch((error) => {
	      warnings.push("Failed to record mt.history entry: " + (error?.message ?? String(error)));
	    });
	    const allWarnings = consoleLines.length > 0
      ? [...warnings, "Captured " + consoleLines.length + " console message(s): " + consoleLines.slice(0, 3).join(" | ")]
      : warnings;
    return {
      ok: true,
      result: value,
      resultIsUndefined: value === undefined,
      output: stringifyForToolContent(value),
	      sanitizerWarnings: allWarnings,
	      effects,
	    };
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined,
    };
  } finally {
    console.log = original.log; console.info = original.info; console.warn = original.warn; console.error = original.error;
    await metatool?.dispose?.().catch(() => {});
  }
}
`,
    },
  ]);

  const guestProgram = `
(async () => {
  const { run } = await import("/opt/jcode-mt/run.mjs");
  return await run(${JSON.stringify({
	    source: request.source,
	    inputs: request.inputs,
	    profile: request.profile ?? "pure",
		    promise_timeout_ms: request.promise_timeout_ms,
		    capabilities: request.capabilities,
		  })});
})()`;

  const evaluation = await runtime.javascript.evaluate(guestProgram, {
    timeoutMs: request.limits.wall_time_ms,
  });

  respond({ result: evaluation });
} catch (error) {
  respond({
    error: {
      name: error?.name ?? "Error",
      message: error?.message ?? String(error),
      code: error?.code ?? null,
    },
  });
} finally {
  await runtime?.dispose?.().catch(() => {});
}
