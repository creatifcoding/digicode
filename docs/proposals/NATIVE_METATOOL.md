# Proposal: Jcode-Native MetaTool

> **Status:** Semantic core implemented; native capability migration in progress
> **Date:** 2026-07-28
> **Initiative:** `pi-extension-migration`
> **Related:** [`ENTITY_MENTIONS_SCOPE.md`](./ENTITY_MENTIONS_SCOPE.md)

## 1. Decision

Jcode should build a native successor to Pi MetaTool rather than treating MetaTool only as a legacy compatibility shim.

The native runtime should use the AgentOS JavaScript runtime, powered by Rivet Secure Exec. Guest JavaScript runs in native V8 isolates with Node.js semantics, deny-by-default permissions, virtualized host resources, and explicit CPU and memory limits. QuickJS is not the primary runtime for this initiative.

The durable product is not an evaluator. It is a versioned capability system, programmable composition interface, typed state layer, procedure registry, and auditable execution history. The JavaScript executor remains behind a replaceable runtime boundary.

## 2. Desired outcome

Jcode exposes one programmable async tool:

```javascript
const files = await mt.entity.search({ kind: "file", query: "auth" });
const tasks = await mt.task.search({ state: "ready" });
const result = await mt.procedure.call("review", { files, tasks });
return result;
```

Guest code can compose many bounded operations without repeated model/tool round trips. It receives no ambient access to Jcode internals or the host. Every privileged action crosses an explicit, versioned capability boundary.

## 3. Why a rewrite

The existing Pi MetaTool has valuable semantics:

- A programmable async `mt({ code })` facade.
- Workspace-local named-object storage and full-text search.
- Durable procedures and dynamic overlays.
- Branch-derived execution history.
- Bounded one-turn guidance and steering.
- Short-lived execution workers and bounded serialization.

Its implementation is coupled to Pi lifecycle hooks, prompt injection, custom session records, Bun/Node child processes, and `.pi` storage conventions. Its worker boundary is useful for reliability but is not a security sandbox because guest APIs can expose shell, writes, and arbitrary overlay imports.

A native rewrite lets Jcode preserve the interaction model while making capability authority, isolation, persistence, provenance, and portability first-class.

## 4. Runtime architecture

```text
mt({ code, profile? })
        |
        v
Program validation and budget
        |
        v
AgentOS / Secure Exec V8 isolate
        |
        v
Versioned mt.* capability proxy
        |
        v
Jcode capability broker
   | entity | store | procedure | task | session | fs | process | network |
        |
        v
Jcode services and audited external adapters
```

### 4.1 AgentOS execution

Use AgentOS's JavaScript runtime as the primary executor:

- Native V8 JIT rather than JavaScript interpreted through WASM.
- One isolated guest runtime with Node.js-compatible semantics.
- Kernel-backed or in-isolate implementations of `node:` modules, never raw host modules.
- Deny-by-default permissions for network and host callbacks.
- Virtualized filesystem, process, environment, and child-process behavior.
- Explicit CPU, wall-time, memory, output, and serialization limits.
- Read-only Node module mounts only for reviewed runtime profiles.
- Explicit teardown and cancellation.

Do not describe the runtime as safe merely because JavaScript runs in an isolate. Security depends on the permissions, mounts, host callbacks, system bridge, and capability broker configured for each execution.

**Observed integration boundary (2026-07-29):** the supported AgentOS integration is the pinned `@rivet-dev/agentos-core` Node package. The published Rust crates are internal, lock-step implementation components rather than a supported embedding API. Jcode therefore owns an executor trait and initially implements it with a managed JSON-lines Node sidecar. AgentOS-specific request and error shapes terminate at that adapter boundary.

The sidecar is not an ambient user dependency. Jcode installs a pinned runtime bundle into a versioned Jcode runtime directory, verifies its integrity before launch, starts it with a clean environment, and reports an actionable unavailable-runtime error. Releases may later bundle the same verified assets without changing the executor contract.

AgentOS requires limited **guest** filesystem, process, environment, and child-process facilities to bootstrap its virtual machine. Those facilities are ephemeral kernel resources and are not host authority. The security invariant is therefore precise: no host filesystem mounts, inherited host environment, unrestricted host process execution, network access, or undeclared bindings. A policy that disables the guest bootstrap itself proves only that nothing ran, which is a rather expensive form of silence.

**Measured runtime probe (2026-07-29):** the exact sidecar committed with the native adapter executed pure JavaScript successfully while a host environment sentinel was absent, a host SSH path resolved as absent, outbound `fetch` failed, and an attempted `sh` child process failed. An infinite loop was terminated within the bounded execution window. AgentOS `0.2.15` reported that CPU-limit termination as generic `execution_failed`, however, rather than a typed timeout. Jcode therefore preserves the failure evidence but does not claim deterministic timeout classification yet. A 1-second wall budget also produced false startup timeouts on this machine; the experimental default is 5 seconds pending platform-matrix measurement.

### 4.2 Execution profiles

Start with three named profiles:

| Profile | Intended use | Default authority |
|---|---|---|
| `pure` | Transform supplied values and call deterministic procedures | Ephemeral guest bootstrap resources only; no host mounts, inherited host environment, network, arbitrary host process, or tool callbacks |
| `workspace-read` | Search and inspect authorized entities and native store state | Read-only entity/store capabilities scoped to the active workspace |
| `workspace-mutate` | Explicitly approved state or file mutation | Narrow declared mutation capabilities; no network or process unless separately granted |

Network, shell/process, package installation, host environment, and unrestricted tool callbacks are never implicit. They require a profile or per-run grant that is visible to the user and recorded in provenance.

## 5. Capability contract

Each host operation has a stable semantic identity independent of the current tool name:

```text
CapabilityDescriptor {
  id: "jcode.entity.search",
  version: 1,
  effect: read_only | mutating | process | network | stateful,
  input_schema,
  output_schema,
  required_scope,
  availability,
  provenance_policy,
}
```

The broker must:

1. Validate every argument and result against bounded schemas.
2. Enforce workspace, session, ownership, and policy scope.
3. Apply per-capability time, output, and call-count budgets.
4. Record semantic capability ID, version, effect, duration, outcome, and redacted provenance.
5. Reject undeclared host callbacks.
6. Keep historical capability evidence distinct from executable runtime capabilities.

This manifest is shared with the Legacy Session Bridge. Compatibility matching uses semantic identity, schema version, effect class, state binding, and policy status. It never relies on fuzzy tool-name similarity.

## 6. Native domains

### 6.1 Entities

Entity mentions and MetaTool share one resolver contract. `mt.entity.*` operates on structured references, not reparsed prompt strings.

Initial methods:

- `entity.search`
- `entity.resolve`
- `entity.preview`
- `entity.materialize`

The FFF-backed file/folder provider supplies discovery. Jcode remains authoritative for path authorization, secret policy, final reads, budgets, and provenance.

### 6.2 Store

Create a Jcode-owned, workspace-scoped SQLite store with:

- Namespaced typed objects.
- JSON payload and tags.
- Full-text search.
- Schema and migration versions.
- Transactional writes.
- Ownership and workspace identity.
- Object revision and content hash.
- Auditable mutation history.

Minimum API:

- `store.put`, `store.get`, `store.delete`
- `store.query`, `store.search`, `store.keys`
- `store.describe`, `store.collections`
- Atomic batch/transaction support

#### Store ownership boundary

Jcode's native store is the target canonical backend. The current AgentOS
codemode runtime still mounts a workspace-scoped `/data` directory and runs the
forked Pi-compatible TypeScript engine inside the guest. That path is an
**observed compatibility runtime**, not the final ownership boundary. It exists
so the programmable `mt.*` surface and existing object semantics can be tested
without granting the guest host authority.

`jcode-metatool-store` owns the future canonical object model, migrations,
revision rules, search semantics, and mutation history. The AgentOS guest owns
program execution only. Guest store operations must eventually cross a
versioned Jcode capability broker rather than opening the canonical database
directly. This preserves one writer policy and prevents a runtime adapter from
defining product persistence semantics.

The compatibility runtime may become the default only for execution, never for
canonical state ownership. Migration from guest-local Pi-compatible storage to
`jcode-metatool-store` begins when all of the following are measured:

1. Native CRUD, query, search, collections, and transactional batch behavior
   cover the active Pi store corpus without lossy conversion.
2. A capability-broker transport exposes those operations to guest `mt.*`
   without host filesystem or database mounts.
3. Import verification compares object counts, keys, JSON payloads, tags,
   search results, and content hashes before cutover.
4. Durable codemode, procedure, overlay, and history tests pass against the
   brokered store on the supported platform matrix.
5. Rollback to the pre-migration database is proven before the first live
   workspace is promoted.

Until those gates pass, describe results precisely: the AgentOS guest store is
live and durable for the compatibility runtime; the Rust store is implemented
replacement infrastructure; brokered canonical ownership is proposed, not yet
wired.

### 6.3 Procedures

Procedures are durable, versioned programs:

```text
Procedure {
  id,
  revision,
  source,
  runtime_profile,
  required_capabilities,
  input_schema,
  output_schema,
  resource_limits,
  content_hash,
  created_at,
  provenance,
}
```

Procedure calls pin a revision. Updating a procedure creates a new revision rather than silently rewriting historical executions.

### 6.4 Overlays

Overlays extend the capability namespace through a reviewed manifest:

- Versioned manifest format.
- Stable overlay and export IDs.
- Root containment and realpath verification.
- Content hashes.
- Declared capability requirements and effects.
- Explicit runtime profile.
- No ambient host imports.
- Approval when installation or update expands authority.

Dynamic npm and read-only `node_modules` mounts are later-stage features. They must not bypass overlay review or capability declarations.

### 6.5 Execution journal

Every run records:

- Program or procedure identity and content hash.
- Runtime and profile versions.
- Granted capabilities.
- Bounded inputs and outputs or references to retained evidence.
- Host capability calls and outcomes.
- CPU, wall time, memory, output size, and termination reason.
- Workspace, session, model, and user provenance.
- Redactions and omitted evidence.

The journal is not model context by default. Jcode may derive a short bounded guidance result for the next turn.

## 7. Pi MetaTool migration

Treat existing Pi data as an import source, not the canonical backend for new state.

Sources include:

- `<workspace>/.pi/rlm/store.db`
- Pi `mt-history` custom session records
- Existing procedure and overlay metadata

Migration requirements:

1. Open source databases read-only.
2. Preserve collection/key identity, data, tags, timestamps, and source locators.
3. Record importer and source schema versions.
4. Preserve unsupported records as archived evidence with warnings.
5. Never modify or silently upgrade the Pi database.
6. Provide repeatable, idempotent import with source digests.
7. Maintain a compatibility view where lossless mapping is not yet possible.

Crowmacs annotation RPC and the existing Pi TUI overlay are optional adapters, not MVP requirements.

## 8. Interaction with legacy sessions

Imported Pi/OpenCode sessions may contain historical `mt` calls. Those calls are evidence and must never be executed during import or mention expansion.

A compatibility preflight compares their observed MetaTool capabilities with the current Jcode manifest and reports:

- `exact`
- `schema_drift`
- `adaptable`
- `state_missing`
- `policy_blocked`
- `missing`
- `unknown`
- `unsupported`

A Jcode-native procedure may be offered as an explicit reviewed adapter. Jcode never silently substitutes it for a historical operation.

## 9. MVP vertical slice

The smallest useful end-to-end implementation is:

1. Register `mt` with `{ code: string, profile?: "pure" | "workspace-read" }`.
2. Execute code inside an AgentOS runtime with hard resource limits.
3. Expose only `mt.entity.search/resolve/preview` and read-only `mt.store.get/query/search`.
4. Return a bounded clone-safe value plus captured output and termination metadata.
5. Persist an execution-journal entry.
6. Add one-turn bounded guidance describing available methods and failed capability requests.
7. Demonstrate that host filesystem mounts, inherited host environment, network, arbitrary host processes, arbitrary host tools, and undeclared modules are unavailable. Guest virtual resources required by AgentOS bootstrap remain bounded and ephemeral.

This slice should follow the FFF-backed `@file` entity foundation so it can consume the same typed entity references and authorization logic.

## 10. Staged milestones

### Milestone 0: Contract

- Lock capability descriptors, runtime profiles, limits, journal schema, and storage ownership.
- Threat-model mounts, host callbacks, tool bridging, process execution, and network escalation.
- Define the relationship between `Request`/`StoredMessage` entity sidecars and `mt.entity` values.

### Milestone 1: Runtime MVP

- Integrate AgentOS/Secure Exec.
- Implement `pure` and `workspace-read` profiles.
- Expose the read-only entity and store capabilities.
- Add cancellation, resource limits, bounded serialization, and audit records.

### Milestone 2: State and procedures

- Ship native typed object storage and FTS.
- Add procedure definitions, revisions, schemas, calls, and execution history.
- Add transactional writes behind explicit mutation authority.

### Milestone 3: Overlays and escalation

- Add contained overlay manifests and reviewed module mounts.
- Add explicit process/network/package profiles.
- Add approval and provenance for privilege expansion.

### Milestone 4: Pi migration

- Import `.pi/rlm/store.db`, procedures, overlays, and `mt-history` records.
- Produce loss and compatibility reports.
- Validate imports against representative user data without modifying it.

### Milestone 5: Legacy capability bridge

- Generate historical and runtime manifests.
- Add compatibility preflight for imported sessions.
- Support explicit reviewed adapters for compatible MetaTool operations.

## 11. Non-goals

- Recreating the Pi extension lifecycle or UI exactly.
- Treating a child process, V8 isolate, or AgentOS VM as sufficient security without a deny-by-default capability policy.
- Giving guest code ambient access to all Jcode tools.
- Enabling network, shell, package installation, or host environment access in the MVP.
- Using the existing Pi database as Jcode's permanent canonical store.
- Silently executing historical MetaTool calls.
- Shipping every existing MetaTool method before the programmable core is proven.
- Coupling the capability contract to AgentOS-specific types.

## 12. Acceptance criteria

1. Untrusted guest code cannot access host files, environment, network, processes, or Jcode tools without an explicit grant.
2. Runtime limits terminate infinite loops, excessive allocation, oversized output, and excessive capability calls deterministically, and classify each termination precisely. Infinite-loop termination is measured; precise AgentOS CPU-limit classification remains blocked by upstream `execution_failed` reporting.
3. All host calls are schema-validated, scoped, budgeted, and journaled.
4. `workspace-read` cannot mutate files, store state, tasks, sessions, or initiatives.
5. Procedures are revisioned and historical calls remain reproducible against their pinned definition and capability manifest.
6. Overlay installation rejects traversal, ID mismatch, undeclared exports, changed hashes, and privilege escalation without approval.
7. Entity operations use the same canonical references and authorization logic as first-class mentions.
8. Pi imports are read-only, idempotent, source-versioned, and report unsupported records.
9. Historical `mt` calls remain inert evidence until a user explicitly chooses a compatible continuation action.
10. The executor can be replaced without changing procedure, capability, storage, or journal identities.

## 13. Open decisions

1. Default CPU, wall-time, memory, output, and capability-call budgets after platform-matrix measurement.
2. Whether explicit global collections are warranted beyond the canonical workspace-scoped SQLite store.
3. Whether procedure source is JavaScript-only initially or supports declarative pipelines.
4. Which additional Node builtins are available beyond those required for AgentOS bootstrap.
5. How user approvals compose with existing Jcode tool policy for mutating, process, and network capabilities.
6. Whether execution journals retain bounded values directly or content-addressed encrypted evidence.
7. How read-only npm module mounts are reviewed, pinned, and updated.

## 14. Dependency security gate

The pinned `@rivet-dev/agentos-core@0.2.15` dependency graph currently reports seven advisories: three high and four moderate. The high advisory chain includes Pi packages that AgentOS depends on but the MetaTool executor does not intentionally invoke; the moderate chain includes Google API and UUID dependencies. This is measured dependency exposure, not yet measured exploitability.

Before production enablement Jcode must:

1. Produce the runtime-reachable dependency subset for the JavaScript evaluator path.
2. Verify that no Pi extension installation, auth-file writing, HTML export, Google API, or affected UUID buffer API is reachable from the sidecar protocol.
3. Run the sidecar from an immutable, owner-only runtime directory with no package installation at execution time.
4. Pin the complete dependency tree and integrity digest rather than accepting semver drift.
5. Fail the release security gate if a reachable high-severity advisory remains.

The semantic core may be developed behind an explicit experimental availability state while this gate remains open. “Transitive” is not a synonym for “harmless”; it is merely a direction in a graph.

## 15. Definition of done

The initiative is ready for implementation when Milestone 0 fixes the runtime adapter boundary, capability descriptor schema, default profiles and limits, native store ownership, journal retention, and threat model. The first production milestone is complete when a bounded `mt({code})` execution can safely compose read-only entity and store operations inside AgentOS while proving that undeclared host authority is unavailable.
