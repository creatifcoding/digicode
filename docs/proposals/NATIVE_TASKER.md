# Proposal: Jcode-Native Tasker

> **Status:** Execution-ready architecture proposal
> **Date:** 2026-07-28
> **Initiative:** `native-tasker`
> **Related:** [`ENTITY_MENTIONS_SCOPE.md`](./ENTITY_MENTIONS_SCOPE.md), [`NATIVE_METATOOL.md`](./NATIVE_METATOOL.md)

## 1. Decision

Jcode should build a native durable project work orchestration system inspired by Pi Tasker.

Native Tasker is distinct from:

- The existing `todo` tool, which communicates a session-local execution plan and progress.
- Initiatives, which represent durable goals, milestones, blockers, and strategic progress.
- Swarm task graphs, which coordinate a particular multi-agent execution topology.
- Scheduled tasks, which arrange future execution.

Native Tasker owns the canonical project work graph: features, tasks, dependencies, notes, claims, locks, work units, completion gates, and retained evidence. It links to the other systems rather than collapsing them into one overloaded record type.

The central invariant is:

> Commit canonical state transactionally, derive and publish a bounded snapshot, then let prompts and UIs project that state. Runtime policy is authoritative; model obedience is not.

## 2. Desired outcome

A user or agent can:

1. Model a durable project backlog with nested features and dependency-aware tasks.
2. Ask for the next deterministic ready task.
3. Claim a bounded working set for a particular session or swarm member.
4. Prevent conflicting work through feature/task/file locks and ownership checks.
5. Run required completion gates and retain full evidence separately from bounded summaries.
6. Resume work from another session without losing project state or provenance.
7. Link tasks to initiatives, sessions, commits, files, peers, scheduled work, and MetaTool procedures.
8. Import the user's existing Pi Tasker database without modifying it.

Example interaction:

```text
Task #184 · Persist entity-reference sidecars
State: in_progress
Feature: #F12 Entity mentions
Claim: session jcode_... / swarm peer blowfish
Dependencies: ready
Files: crates/jcode-message-types/**, crates/jcode-protocol/**
Gates: cargo test -p jcode-message-types; cargo fmt --check
```

## 3. Domain boundaries

### 3.1 Existing `todo`

Keep `todo` lightweight and session-oriented. It is useful for communicating the current plan, confidence, feedback loops, and completion status to the user.

A session todo may reference a native Tasker task, but it does not become canonical project state and does not own dependencies, claims, gates, or cross-session coordination.

### 3.2 Initiative

An initiative answers why the work matters and how a durable outcome progresses. A Tasker feature may link to one initiative and optionally one milestone. Initiative progress is a projection derived from linked Tasker state or explicitly curated updates.

Tasker must not silently rewrite initiative intent, success criteria, or strategy.

### 3.3 Swarm task graph

A swarm node is an execution assignment. It may bind to one Tasker task or work unit. Completing a swarm node produces evidence and may advance a work unit, but only Tasker policy decides whether the canonical task is complete.

### 3.4 Schedule

A scheduled task may point to a Tasker task or work unit. Triggering a schedule does not claim or complete the canonical task automatically.

### 3.5 Native MetaTool

MetaTool receives narrow semantic capabilities over the Tasker service. Guest code never receives database access or the full internal service object.

## 4. Ubiquitous language

### Project

A stable logical workspace identity. It is not merely the current filesystem path. The project record stores canonical and observed roots and may survive a worktree move.

### Feature

A durable container for related work. Features may be nested and may depend on other features. States:

- `open`
- `active`
- `closed`
- `archived`

### Task

The smallest canonical unit of project work. States:

- `todo`
- `in_progress`
- `blocked`
- `done`
- `cancelled`

A task belongs to one feature and can depend on other tasks. A done or cancelled task is terminal unless explicitly reopened with audit provenance.

### Dependency

A directed prerequisite edge. A dependency graph must remain acyclic. Readiness requires all required predecessor tasks to satisfy their completion policy.

### Ready

A non-terminal task is ready when:

- Its state permits execution.
- All required task dependencies are satisfied.
- Its feature and ancestor features permit work.
- It is not blocked by an unresolved gate, policy hold, or foreign lock.
- Its concurrency and ownership policy allows a new work unit.

Readiness is deterministic for a specific database revision and policy version.

### Claim

A renewable assertion that a working identity intends to perform work. Claim kinds:

- `claim`: ordinary ownership.
- `hold`: temporary reservation without active execution.
- `lock`: exclusive ownership enforced by policy.

Claims have scope, owner, lease, heartbeat, and release reason. A model may request a claim, but trusted provenance comes from Jcode's runtime context.

### Work unit

One bounded execution attempt against a task. States:

- `queued`
- `active`
- `done`
- `cancelled`
- `failed`

A work unit binds a task to a session, agent runtime, optional swarm member, process instance, claim, and evidence set.

### Gate

A completion requirement evaluated against a task or feature. Gate kinds:

- `manual`
- `command`
- `tool`
- `test_suite`
- `agent_review`
- `script`
- `evidence`

Gate states:

- `pending`
- `running`
- `passed`
- `failed`
- `waived`
- `stale`

### Evidence

Immutable or append-only proof associated with work or a gate, such as command output, test results, commits, diffs, tool results, reviews, screenshots, or external artifact references.

### Snapshot

A bounded immutable projection of the canonical state at one monotonically increasing project revision. Prompts, UIs, and subscribers consume snapshots rather than holding mutable database entities.

## 5. Identity and project partitioning

Use opaque, time-sortable UUIDv7 identifiers internally with stable type prefixes at serialized boundaries:

```text
proj_<uuidv7>
feat_<uuidv7>
task_<uuidv7>
claim_<uuidv7>
wu_<uuidv7>
gate_<uuidv7>
evidence_<uuidv7>
note_<uuidv7>
```

Use the `uuid` crate with the `v7` and `serde` features. Jcode already uses the `uuid` ecosystem, so this avoids another ID dependency.

For human interaction, allocate monotonic project-local aliases:

- `#184` for tasks.
- `#F12` for features.

Aliases are never reused, even after deletion or import.

Project identity resolution should prefer, in order:

1. Explicit configured project ID.
2. Repository-native stable identity when available.
3. Existing root-to-project mapping.
4. A newly created project record for the canonicalized root.

The current path is a locator, not identity. Multiple worktrees may map to the same logical project only when explicitly configured. Default behavior isolates worktrees to avoid accidental cross-branch locks.

## 6. Persistence architecture

### 6.1 Location

Canonical database:

```text
~/.jcode/tasker/tasks.db
```

Use one global database partitioned by project ID. This preserves cross-session discovery while keeping all writes transactional and migration management centralized.

### 6.2 Crate choices

Use established Rust crates narrowly:

| Concern | Choice | Reason |
|---|---|---|
| SQLite access | `rusqlite` | Mature, direct SQLite API, predictable transactions, FTS5 support, no query-runtime abstraction tax |
| Tokio integration | `tokio-rusqlite` | Runs each SQLite connection on its dedicated blocking thread and exposes a cheap async handle |
| SQLite distribution | `rusqlite` bundled SQLite feature | Reproducible SQLite version and required FTS5/JSON behavior across supported platforms |
| Migrations | `rusqlite_migration` | Small rusqlite-native migration layer using SQLite `user_version` |
| Graph validation | `petgraph` | Mature cycle detection, traversal, and topology algorithms for in-memory validation |
| Serialization | `serde`, `serde_json` | Existing Jcode standard |
| IDs | `uuid` v7 | Existing ecosystem and time-sortable opaque identifiers |
| Time | `chrono` | Existing Jcode standard and serde support |
| Content hashes | `blake3` | Fast hashes for evidence, snapshots, policies, and import idempotency |
| Async/eventing | `tokio::sync::watch` and `broadcast` | Existing Jcode runtime primitives |
| Cancellation | `tokio-util::sync::CancellationToken` | Existing dependency and structured cancellation support |
| Errors | Typed domain errors plus `thiserror` at crate boundary | Stable machine-readable failures; `anyhow` only at application composition boundaries |

Do not add SQLx, Diesel, SeaORM, RocksDB, Tantivy, or an external graph database for the initial system. SQLite already provides transactions, indexes, JSON functions, and FTS5. The workload has one serialized write path and many bounded reads, so a large connection pool is unnecessary and may worsen SQLite contention.

If Jcode does not already depend on `thiserror` or `blake3` at implementation time, validate their compile and maintenance cost before adding them. A small handwritten error type or existing SHA-256 helper is acceptable if it materially reduces dependency surface.

### 6.3 Connection model

Use:

- One dedicated read/write `tokio-rusqlite::Connection` for canonical mutations.
- WAL mode.
- Foreign keys enabled.
- A bounded busy timeout.
- `synchronous=NORMAL` by default, configurable for durability-sensitive environments.
- Explicit transaction modes. Mutations use immediate transactions where conflict detection matters.
- A small number of optional read-only connections only if profiling proves the single handle is a bottleneck.

Every public mutation executes through one `TaskerService` command path. Tools never open ad hoc database connections.

### 6.4 Core schema

Initial tables:

```text
projects
project_roots
project_revisions
features
feature_dependencies
feature_notes
tasks
task_dependencies
task_notes
claims
claim_heartbeats
work_units
work_unit_events
gates
gate_runs
evidence
entity_links
imports
outbox_events
```

FTS5 virtual tables:

```text
features_fts
 tasks_fts
notes_fts
```

FTS content is maintained transactionally through explicit repository writes or carefully tested triggers. Prefer explicit writes initially because they are easier to reason about during migrations and imports.

### 6.5 Revision invariant

Every successful canonical mutation:

1. Begins a transaction.
2. Reads and validates the expected project revision when supplied.
3. Applies all writes.
4. Increments the project revision exactly once.
5. Appends an `outbox_events` record with the new revision and bounded change summary.
6. Commits.
7. Reloads the bounded project snapshot at that revision.
8. Publishes it through `watch` and publishes the event through the Jcode bus.

The durable outbox prevents a process crash after commit from permanently losing the state-change notification. On startup, the service replays undispatched outbox records idempotently.

## 7. Repository and service layers

Create leaf crates rather than placing SQL and domain policy inside tool handlers:

```text
crates/jcode-tasker-types/
  src/domain.rs
  src/commands.rs
  src/events.rs
  src/snapshot.rs
  src/policy.rs
  src/errors.rs

crates/jcode-tasker-store/
  src/lib.rs
  src/migrations.rs
  src/repository.rs
  src/import_pi.rs
  migrations/*.sql

crates/jcode-tasker-core/
  src/service.rs
  src/readiness.rs
  src/claims.rs
  src/gates.rs
  src/snapshot.rs
  src/policy.rs
  src/integrations.rs
```

Application adapters live in `jcode-app-core`:

```text
crates/jcode-app-core/src/tool/task.rs
crates/jcode-app-core/src/tool/work_unit.rs
crates/jcode-app-core/src/tool/gate.rs
crates/jcode-app-core/src/tasker_hooks.rs
crates/jcode-app-core/src/agent/task_context.rs
```

Protocol/UI types belong in the narrowest existing protocol or view-model crate. Do not make the TUI depend directly on the SQLite store.

## 8. Command and query model

### 8.1 Commands

Commands are typed and always include expected scope. Runtime provenance is added by the service, not trusted from model input.

Examples:

```text
CreateTask
UpdateTask
CreateFeature
AddDependency
AppendNote
CreateClaim
RenewClaim
ReleaseClaim
StartWorkUnit
CompleteWorkUnit
DefineGate
RunGate
CompleteTask
ReopenTask
Batch
```

Each mutation accepts an optional expected revision. Interactive clients should send it after previewing state. A stale revision returns a structured conflict containing the current revision and bounded changed entities.

### 8.2 Atomic batch

Batch operations are first-class. A plan involving many tasks and dependencies should validate and commit as one transaction.

Batch validation includes:

- Referential integrity.
- State transitions.
- Duplicate aliases and IDs.
- Dependency cycle detection.
- Feature ancestry rules.
- Claim and lock conflicts.
- Gate policy validity.
- Maximum operation and payload limits.

No partial batch success.

### 8.3 Queries

Queries return bounded projections:

```text
GetTask
GetFeature
Search
ListReady
ListBlocked
ListMine
GetWorkingSet
GetGateStatus
GetDependencyGraph
GetChangesSinceRevision
```

Search uses SQLite FTS5 plus structured filters. Do not add a separate search engine until measured requirements exceed FTS5.

## 9. Dependency graph and readiness

Use SQL as canonical edge storage and build a bounded in-memory `petgraph::GraphMap` or equivalent for validation and traversal.

### 9.1 Cycle prevention

Before adding dependency edges:

1. Load the affected feature/project subgraph.
2. Apply proposed edges in memory.
3. Run cycle detection/topological validation.
4. Reject the entire transaction with a concrete cycle path.

For deterministic ready ordering, do not rely on `petgraph::toposort` tie ordering. Rank ready tasks explicitly by:

1. Priority.
2. Blocking-descendant count or critical-path signal when available.
3. User-defined rank.
4. Creation sequence.
5. Canonical task ID.

### 9.2 Readiness query

Readiness is computed from canonical state and a versioned policy. Cache it only inside a snapshot. Do not persist a mutable `is_ready` flag that can drift from dependencies.

A `ReadinessExplanation` identifies:

- Satisfied dependencies.
- Unsatisfied dependencies.
- Feature or ancestor restrictions.
- Claim/lock conflicts.
- Pending or failed gates.
- Policy holds.
- Snapshot revision and policy version.

## 10. Claims, locks, and work units

### 10.1 Trusted ownership

Owner provenance derives from `ToolContext` and runtime state:

```text
OwnerIdentity {
  session_id,
  agent_instance_id,
  process_instance_id,
  swarm_member_id?,
  device_id?,
  user_id?,
}
```

The model supplies only the desired task and claim scope. It cannot claim to be another session or peer.

### 10.2 Leases

Claims use leases with:

- `acquired_at`
- `expires_at`
- `last_heartbeat_at`
- `owner_identity`
- `release_reason`

Jcode renews active local leases from runtime liveness, not from model-generated calls alone. A crashed session eventually releases its ordinary claim. Explicit locks may require manual or policy-authorized recovery.

### 10.3 Scope

Claim scope may be:

- Task.
- Feature subtree.
- Explicit entity set.
- File/path patterns.

Global project claims are prohibited by default. The service enforces bounded scope and configurable maximum lease duration.

### 10.4 Working set

A session sees its own active work units plus bounded conflicts relevant to them. It does not receive every agent's complete task state in the prompt.

## 11. Gates and evidence

### 11.1 Gate definitions

A gate definition includes:

```text
GateDefinition {
  id,
  scope,
  kind,
  config,
  required,
  fail_fast,
  freshness_policy,
  effect_policy,
  created_by,
  revision,
}
```

Command and script gates require explicit executable, arguments, working directory policy, environment allowlist, timeout, output cap, and cancellation behavior. Do not store an unparsed shell string as the canonical gate definition.

### 11.2 Gate execution

Gate runs:

- Use `tokio::process::Command` or an approved AgentOS/Secure Exec profile when stronger isolation is required.
- Capture bounded stdout/stderr summaries.
- Store full output as content-addressed evidence when retention policy permits.
- Record exit status, duration, cancellation, timeout, environment fingerprint, code revision, and affected files.
- Mark prior passes stale when their freshness inputs change.

### 11.3 Completion transaction

Completing a task:

1. Verifies ownership policy.
2. Evaluates required gate freshness.
3. Rejects completion if required gates are pending, failed, stale, or running.
4. Attaches completion evidence.
5. Transitions the task and work unit atomically.
6. Releases ordinary claims according to policy.
7. Publishes the new snapshot.

A tool result or optimistic model statement cannot bypass gates.

## 12. Runtime tool surface

Avoid porting Pi Tasker's entire tool count. Start with three cohesive tools.

### 12.1 `task`

Actions:

```text
list, show, search, ready, blocked
create, update, batch
add_dependency, remove_dependency
add_note
complete, reopen, cancel
```

### 12.2 `work_unit`

Actions:

```text
mine, show
claim, hold, lock
start, renew, release
complete, fail, cancel
```

### 12.3 `gate`

Actions:

```text
list, show, define
check, check_all
waive, invalidate
history, evidence
```

Tool schemas carry detailed operating semantics and examples. Read-only and mutation actions should be distinguishable by the registry's effect metadata and approval policy.

## 13. Tool effect metadata and policy hooks

### 13.1 General effect descriptors

Extend the tool abstraction or registry metadata with a machine-readable effect descriptor:

```text
ToolEffectDescriptor {
  class: read_only | filesystem_mutation | process | network | communication | state_mutation,
  resources: declared or dynamically resolved resource selectors,
  supports_preflight: bool,
}
```

Do not infer security-critical behavior solely from tool names or prose descriptions.

### 13.2 Pre-tool policy pipeline

Add a general pre-tool policy stage before execution:

```text
Tool request
  -> session tool policy
  -> effect/resource resolution
  -> Tasker work policy
  -> existing approval/sandbox policy
  -> execution
  -> post-tool hooks
```

The Tasker policy can:

- Allow.
- Allow with warning.
- Require explicit user approval.
- Reject with a structured corrective action.

Examples:

- Reject editing a path locked by another working set.
- Reject mutation when the active task is blocked.
- Require a work unit for a feature configured as governed.
- Warn when a mutation falls outside the claimed entity/path set.
- Reject completion while required gates are unsatisfied.

### 13.3 Shell limitations

Arbitrary shell commands can mutate files that are not statically knowable. Initial enforcement must be honest:

- Parse known safe/read-only commands where reliable.
- Treat unknown shell commands as broad process/filesystem effects.
- Require stricter approval or an AgentOS profile for governed work.
- Reconcile actual changed files after execution using git/worktree observation.
- Never claim perfect path enforcement for unrestricted host shell execution.

Longer term, AgentOS or another mediated execution environment can provide stronger filesystem capability boundaries.

### 13.4 Post-tool lifecycle

Use the existing registry post-tool hook path for:

- Advancing a claimed task from `todo` to `in_progress` after the first successful mutation.
- Attaching test/build/tool evidence.
- Updating gate runs.
- Reconciling changed files with the claimed scope.
- Recording commits and artifact references.
- Emitting bounded next-action guidance.

Post-tool hooks do not complete tasks implicitly unless an explicit, versioned policy says the required evidence is sufficient.

## 14. Prompt architecture

Jcode currently composes prompts from:

1. Static base system prompt.
2. Capability-gated prompt modules.
3. `AGENTS.md` and global instructions.
4. Project/global prompt overlays.
5. Preferred-tools guidance.
6. Skills.
7. Memory.
8. Current-turn system reminders.
9. Provider-visible tool definitions and schemas.

Native Tasker should use these layers deliberately.

### 14.1 Tool schemas

Most instructions belong in tool definitions:

- State transitions.
- Readiness semantics.
- Dependency behavior.
- Claim and lock rules.
- Atomic batch behavior.
- Gate requirements.
- Expected-revision conflicts.

This guidance appears only while the relevant tools are enabled and stays close to the operation contract.

### 14.2 Capability-gated static module

Add a short `PromptCapabilities` module only when native Tasker is enabled:

```text
Durable project work may be governed by Tasker. Inspect the active working
set before mutation. Respect dependencies, claims, locks, and required gates.
Do not claim global work or mark work complete without required evidence.
Runtime policy and canonical Tasker state are authoritative.
```

The static module must remain small. Do not inject the complete Tasker manual into every turn.

### 14.3 Dynamic task-context capsule

At turn start, append a bounded current-turn system reminder when the session has relevant governed work:

```text
[Tasker working context · revision 1942]
Task #184: Persist entity-reference sidecars
Work unit: wu_... active; lease valid for 12m
Dependencies: ready
Claimed scope: crates/jcode-message-types/**, crates/jcode-protocol/**
Conflicts: none
Required gates: unit-tests pending; rustfmt pending
```

The capsule includes only:

- Current task and work unit.
- Lease/claim status.
- Blocking dependency or conflict.
- Claimed files/entities.
- Required completion gates.
- Snapshot revision and policy version.

The capsule is generated from canonical state. The model cannot author or persist it directly. It is recalculated when revision, claim, work unit, or gate state changes.

### 14.4 Dynamic corrective guidance

When policy rejects an action, return a structured tool result with:

- The violated invariant.
- Canonical IDs.
- Current revision.
- A bounded corrective action, such as claiming the task, waiting for a lock, or resolving a dependency.

Avoid permanent system-prompt mutation and avoid Pi-style hidden steering records as the primary control mechanism.

## 15. Snapshot and event model

### 15.1 Snapshot contents

A session snapshot contains bounded projections:

- Project revision.
- Current working set.
- Ready tasks capped and ranked.
- Relevant blocked tasks.
- Claim conflicts relevant to the session.
- Gate summaries.
- Linked initiative progress summary.

It excludes full notes, full evidence, complete project topology, and other agents' unrelated work.

### 15.2 Eventing

Use:

- `watch::Sender<Arc<ProjectSnapshot>>` for the latest project snapshot.
- Jcode's existing bus/broadcast infrastructure for discrete events.
- Durable outbox replay for crash consistency.

Slow subscribers may skip intermediate snapshots but can always observe the latest revision. Consumers needing every event read from the outbox/change query.

## 16. Initiative integration

A feature may link to one initiative and milestone. Tasker computes a progress projection:

- Total and completed tasks.
- Weighted progress if explicitly configured.
- Active blockers.
- Current ready work.
- Gate health.

Updating the initiative is explicit or policy-controlled. Tasker does not overwrite qualitative initiative fields such as strategy, success criteria, or why.

## 17. Swarm integration

A swarm node may bind to a Tasker task or work unit:

```text
TaskBinding {
  task_id,
  work_unit_id,
  swarm_node_id,
  assigned_session_id,
  lease_revision,
}
```

Rules:

- Assignment attempts acquire or validate the claim transactionally.
- Agent completion reports become evidence.
- A completed swarm node does not imply a completed Tasker task if gates remain.
- Reassignment releases or transfers ownership explicitly.
- Deep swarm descendants cannot escape the parent task's claimed scope without approval.

## 18. Native MetaTool integration

Expose semantic capabilities:

```text
jcode.task.read.v1
jcode.task.mutate.v1
jcode.work.read.v1
jcode.work.claim.v1
jcode.work.release.v1
jcode.gate.read.v1
jcode.gate.check.v1
```

Example:

```javascript
const ready = await mt.task.ready({ feature: "feat_...", limit: 5 });
const unit = await mt.work.claim({ task: ready[0].id });
const gates = await mt.gate.checkAll({ task: unit.taskId });
return { unit, gates };
```

AgentOS profiles determine which capabilities are granted. Guest code cannot send trusted owner/session provenance and cannot access raw SQL.

## 19. Pi Tasker import

Source:

```text
~/.pi/tasker/tasks.db
```

Migration principles:

1. Open the Pi database read-only and fail if read-only mode cannot be guaranteed.
2. Fingerprint source schema and contents.
3. Preserve source IDs, aliases, timestamps, notes, dependencies, features, claims, work units, gates, and visual-artifact references when representable.
4. Map records into native IDs while retaining a durable source-ID mapping.
5. Preserve unsupported fields as archived JSON evidence with warnings.
6. Import idempotently using source digest plus source record identity.
7. Never mutate, vacuum, migrate, or lock the source database for writing.
8. Produce a structured import report with counts, losses, conflicts, and unresolved project-root mappings.

Do not preserve stale live claims as active native claims. Import them as historical claim evidence unless the user explicitly reactivates work.

Cockpit layouts, visual artifact rails, GraphViz/Mermaid projections, topology microtools, and Pi-specific styling are later UI work.

## 20. Security and reliability

- All SQL uses bound parameters.
- Foreign keys and constraints enforce core invariants in addition to service validation.
- The database and migration path use secure file permissions.
- Secrets and raw environment values are not stored as task evidence by default.
- Evidence retention is configurable and bounded.
- Tool outputs and external review text are untrusted data.
- Imported database content cannot create executable gates without explicit review.
- Command gates use structured executables/arguments, not interpolated shell strings.
- Lock recovery is audited.
- Lease expiry uses wall time plus process/session liveness where available.
- State transitions are idempotent under request IDs.
- Mutations support expected-revision optimistic concurrency.
- Backups use SQLite's online backup API or safe checkpointed copies, never arbitrary copying during writes.

## 21. Observability

Metrics may include:

- Command/query latency.
- Transaction retries and busy time.
- Snapshot build latency and size.
- Outbox lag.
- Ready-task count.
- Claim conflicts and lease expirations.
- Gate duration and pass/fail rates.
- Policy allow/warn/reject counts.
- Import counts and loss categories.

Telemetry must not include task titles, notes, paths, command output, prompts, evidence content, or project roots by default.

Diagnostics should expose:

- Database health and schema version.
- WAL/checkpoint status.
- Current project revision.
- Subscriber count and event lag.
- Active local claims and work units.
- Stale or orphaned claims.
- Failing gates and retained evidence size.

## 22. Testing strategy

### Domain tests

- Every valid and invalid state transition.
- Dependency cycles with concrete cycle paths.
- Deterministic ready ordering.
- Feature ancestry and recursive blocking.
- Claim scope intersections.
- Gate freshness.

Use property-based testing only where it pays for itself, especially random DAG generation and state-machine transitions. Prefer the established `proptest` crate if introduced.

### Store tests

- Fresh migrations and migration upgrades.
- Foreign keys and uniqueness.
- Atomic batches and rollback.
- WAL/busy conflict behavior.
- Revision monotonicity.
- Durable outbox replay.
- FTS synchronization.
- Backup and restore.

### Concurrency tests

- Competing claims.
- Stale expected revisions.
- Lease expiry and renewal races.
- Simultaneous gate completion and task completion.
- Process crash after commit before publish.

### Policy tests

- Edit/write/patch blocked by foreign lock.
- Blocked task rejects mutation.
- Out-of-scope mutation warnings and rejection modes.
- Unknown shell commands receive broad-effect treatment.
- Post-tool evidence cannot bypass required gates.

### Prompt tests

- Tasker static guidance appears only when capability is enabled.
- Dynamic capsule is bounded, authoritative, and refreshed on revision change.
- Unrelated backlog does not enter the prompt.
- Tool schema token cost remains measured and bounded.

### Import tests

- Representative snapshots of the user's Pi schema.
- Read-only guarantee.
- Idempotent re-import.
- Source-ID mapping.
- Unsupported-field preservation.
- Stale claim handling.

## 23. Performance targets

Initial targets on a representative local project:

- Read-only task lookup p95 below 20 ms after warm open.
- Ready-list p95 below 50 ms for 100,000 tasks and 500,000 dependency edges after appropriate indexing and bounded project scope.
- Single-task mutation p95 below 50 ms excluding snapshot subscriber work.
- Snapshot build p95 below 100 ms and bounded to configured row/byte caps.
- Claim conflict decision below 25 ms.
- No database operation blocks a Tokio executor thread.
- Prompt capsule below 1,500 characters by default.

These are engineering targets, not promises. Benchmark before adding caches or extra read connections.

## 24. Staged implementation

### Milestone 0: Contract lock

- Approve domain terminology and boundaries.
- Lock IDs, project identity, state transitions, claim semantics, gate semantics, and revision rules.
- Lock crate selections after a minimal compile spike.
- Define `ToolEffectDescriptor` and pre-tool policy interfaces.
- Define static prompt module and dynamic capsule contracts.
- Threat-model shell enforcement, imported gates, and lock recovery.

### Milestone 1: Canonical core

- Add the three Tasker leaf crates.
- Add SQLite configuration and migrations.
- Implement project, feature, task, dependency, note, search, revision, outbox, and snapshot behavior.
- Implement read-only `task` actions and atomic planning batches.
- Benchmark representative graph sizes.

### Milestone 2: Ownership

- Add claims, leases, heartbeats, locks, and work units.
- Derive trusted ownership from session/runtime context.
- Add dynamic working-context capsule.
- Integrate snapshot/bus updates.

### Milestone 3: Policy enforcement

- Add machine-readable tool effects.
- Add pre-tool policy pipeline.
- Enforce governed edits and known mutations.
- Add honest broad-effect behavior for arbitrary shell commands.
- Add post-tool transition and evidence hooks.

### Milestone 4: Gates

- Add structured gate definitions and runs.
- Add evidence storage and retention.
- Enforce completion transactions.
- Integrate test/build/commit evidence.

### Milestone 5: Coordination

- Link features to initiatives and project progress.
- Bind swarm nodes to work units.
- Support schedule references.
- Add richer TUI and desktop projections.

### Milestone 6: Portability

- Import Pi Tasker read-only and idempotently.
- Add MetaTool capabilities.
- Add legacy-session compatibility reporting for historical Tasker calls.

## 25. Explicit non-goals

- Replacing session-local `todo` in the first release.
- Encoding project policy only in a system prompt.
- Porting all Pi Tasker tools or its cockpit before the core is proven.
- Building a distributed database or remote synchronization layer initially.
- Perfectly sandboxing arbitrary host shell commands through prompt rules.
- Automatically activating imported claims, locks, or executable gates.
- Letting a model forge owner identity or gate evidence.
- Storing the complete project graph in every model turn.
- Using a separate search engine before SQLite FTS5 is measured and found insufficient.
- Making initiatives, swarm nodes, schedules, todos, and Tasker tasks the same entity.

## 26. Acceptance criteria

1. Project tasks persist across sessions and process restarts.
2. Dependency cycles are rejected atomically with a useful cycle explanation.
3. Readiness is deterministic for a revision and explains every blocker.
4. Competing claims and locks resolve transactionally using trusted runtime ownership.
5. A crashed session's ordinary claim expires or is recoverable without database surgery.
6. Governed mutations are checked by runtime policy rather than prompt compliance alone.
7. Required gates cannot be bypassed by direct task updates or optimistic tool results.
8. Full gate evidence is retained separately from bounded prompt/UI summaries.
9. Static Tasker prompt guidance appears only while enabled and remains short.
10. Dynamic prompt context contains only the current working set and material constraints.
11. Existing `todo`, initiatives, swarms, and schedules continue to function independently.
12. The database publishes revisioned snapshots after committed transactions and recovers undispatched events after restart.
13. Tasker tools expose machine-readable errors and corrective actions.
14. Pi import is read-only, idempotent, source-versioned, and reports losses.
15. Native MetaTool accesses Tasker only through narrow versioned capabilities.
16. No new database, graph, or search dependency is added without a benchmarked need.

## 27. Open decisions before implementation

1. Exact logical project identity rules across branches and worktrees.
2. Default lease durations, renewal cadence, and manual-lock recovery authority.
3. Whether task priority is categorical, numeric, or both.
4. Whether feature dependencies affect every descendant automatically.
5. The default policy for unclaimed mutations in governed projects: warn, approve, or reject.
6. Which tools can provide precise dynamic resource selectors during preflight.
7. Evidence retention limits and whether large evidence is compressed, content-addressed, encrypted, or externalized.
8. Gate freshness inputs and invalidation rules.
9. Whether project snapshots are daemon-owned for identical remote/local behavior.
10. Exact mapping between Tasker feature progress and initiative progress.
11. Whether the first schema includes visual artifacts or imports them only as generic evidence.
12. Whether SQLite is compiled with bundled SQLite everywhere or follows a platform-specific feature policy.

## 28. Definition of done

This architecture is ready for implementation when Milestone 0 records the open decisions, a dependency compile spike validates `tokio-rusqlite`, bundled `rusqlite`, `rusqlite_migration`, `petgraph`, UUIDv7, and hashing choices, and the tool-policy owners approve the pre-tool effect contract.

The first production vertical slice is complete when Jcode can transactionally create a feature and dependency graph, return deterministic ready work, persist it across restarts, publish revisioned bounded snapshots, and expose the read-only `task` tool without changing the behavior of existing session todos.
