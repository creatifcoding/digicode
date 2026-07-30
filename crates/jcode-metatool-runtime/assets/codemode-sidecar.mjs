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
	  let reconciled = false;

	  const limited = (items, limit) => Array.isArray(items)
	    ? items.slice(0, Math.max(0, Math.min(Number(limit ?? 100), 500)))
	    : [];
	  const clone = (value) => JSON.parse(JSON.stringify(value));
	  const reconcile = async (kind, payload) => {
	    if (reconciled) throw new Error("mt.tasker permits one atomic reconciliation per evaluation");
	    if (!["batch", "plan", "feature_plan"].includes(kind)) {
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
	    status: () => ({
	      mode,
	      snapshotHash: expectedSnapshotHash,
	      counts: snapshot.counts ?? {},
	      truncated: snapshot.truncated === true,
	    }),
	    list: (options = {}) => {
	      const state = options.state;
	      const tasks = state
	        ? limited(snapshot.tasks, 500).filter((task) => task?.state === state)
	        : limited(snapshot.tasks, 500);
	      return limited(tasks, options.limit);
	    },
	    ready: (limit = 100) => limited(snapshot.ready, limit),
	    features: (options = {}) => {
	      const state = options.state;
	      const features = state
	        ? limited(snapshot.features, 500).filter((feature) => feature?.state === state)
	        : limited(snapshot.features, 500);
	      return limited(features, options.limit);
	    },
	    reconcile: async (program) => reconcile(program?.kind, program?.payload),
	    batch: async (operations) => reconcile("batch", { operations }),
	    plan: async (tasks) => reconcile("plan", { tasks }),
	    featurePlan: async (feature) => reconcile("feature_plan", { feature }),
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
