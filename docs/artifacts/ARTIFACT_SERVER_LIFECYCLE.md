# ADR: Standalone artifact server lifecycle and canonical store root

- **Status:** Accepted for T40 implementation
- **Date:** 2026-08-10
- **Owner:** Operational artifact library and server workstream, Tasker T39
- **Scope:** `jcode-artifact-server`, `jcode-artifact-store`, the host-side MetaTool artifact broker, and the `JCODE_HOME` artifact root
- **Follow-up:** Tasker T40 implements this decision. Tasker T41 adds health, recovery, and maintenance evidence.

## Decision

`jcode-artifact-server` remains a **standalone, foreground presentation process**. Its
lifecycle is owned by the explicit launcher or service manager that starts that
process. The server process owns its listener and request tasks until the host
supplies shutdown. The ordinary `jcode serve` daemon, `MetaTool`, and
`ArtifactStore` do not start, stop, restart, or supervise it.

The server is therefore not an app-core startup dependency. T40 must not add an
automatic artifact-server spawn to ordinary Jcode startup, detach the artifact
server behind a client, or make MetaTool admission depend on the HTTP process.
A future product decision may embed the presentation task in a host daemon, but
that would be a new lifecycle decision, not an implicit T40 change.

There is exactly one canonical artifact store root per Jcode home:

```text
<JCODE_HOME>/artifacts/
├── artifacts.sqlite3
└── assets/
    └── artifacts/<artifact-id>/r<revision>-<kind>-<sha256>.blob
```

`JCODE_HOME` is resolved by `jcode-storage::jcode_dir()` (`$JCODE_HOME` when
set, otherwise `~/.jcode`). The process boundary must turn that value into one
absolute store-root path before constructing either the server or the host
artifact store. A relative `.jcode/artifacts` path is not a valid production
default. An explicit custom root remains useful for tests and operator-managed
instances, but it must also be absolute and the same value must be passed to
every participant in that process.

## Why this is the decision

The existing code already separates the important authorities, but it does not
yet make the process and root contract explicit:

- `jcode-artifact-server` exposes loopback catalog, revision, raw-asset, and SSE
  presentation routes. `ArtifactServer::serve_until` accepts a host shutdown
  future, and `main` currently uses Ctrl-C. The binary is independently runnable
  and is not spawned by ordinary app-core startup.
- `jcode-artifact-store` owns the SQLite schema and asset files. It creates the
  database and asset directories, writes assets atomically, commits admission in
  a transaction, and marks revisions immutable. It does not own a listener or a
  process.
- The MetaTool host broker owns the `mt.artifacts` API boundary. Guest code sees
  a bounded catalog snapshot and can emit one `artifacts.admit_bundle` effect in
  apply mode. The host validates and reconciles that effect through
  `ArtifactStore::admit_bundle`; the guest does not receive the artifact
  database or host filesystem.
- MetaTool currently derives its artifact root from
  `crate::storage::jcode_dir()?.join("artifacts")`, while the standalone
  server CLI defaults to the relative path `.jcode/artifacts`. Those are not a
  single-root contract when the server is launched from a different working
  directory or when `JCODE_HOME` is set.
- The current capability manifest records the server as `standalone`, with
  recovery state `standalone`, and explicitly excludes ordinary app-core
  startup and artifact admission policy. This ADR preserves that declared
  disposition while making its boundaries implementable.

The canonical references are:

- `crates/jcode-artifact-server/src/lib.rs`
  (`ArtifactServer`, `serve_until`, `ensure_loopback`, request routing)
- `crates/jcode-artifact-server/src/main.rs` (standalone CLI and Ctrl-C owner)
- `crates/jcode-artifact-store/src/lib.rs`
  (`ArtifactStore::open_migrate`, `admit_bundle`, revision schema)
- `crates/jcode-app-core/src/tool/metatool.rs`
  (`artifact_root`, `artifact_store`, `artifact_catalog`, and effect reconciliation)
- `crates/jcode-metatool-runtime/assets/codemode-sidecar.mjs`
  (`mt.artifacts` guest capability)
- `crates/jcode-metatool-runtime/assets/guide.json` (public capability signatures)
- `crates/jcode-storage/src/lib.rs` (`jcode_dir()`)
- `crates/jcode-app-core/src/tool/metatool.rs` via its `crate::storage::jcode_dir()` adapter
- `docs/FORK_CAPABILITY_MANIFEST.json` (`artifact-server` standalone contract)

## Ownership matrix

| Concern | Exact owner | Contract | Explicit non-owner |
| --- | --- | --- | --- |
| Start, stop, and restart policy | The explicit launcher or service manager that invoked `jcode-artifact-server` | Keep one foreground process per listener; wait for clean exit before restart | `jcode serve`, MetaTool, ArtifactStore |
| Listener and request-task lifetime | The `ArtifactServer` process through `serve_until` | Bind loopback, stop accepting on shutdown, finish or cancel owned requests, then return | The launcher does not reach into store internals |
| Store-root resolution | The host process boundary using `jcode-storage::jcode_dir()` | Resolve one absolute `<JCODE_HOME>/artifacts` path and pass it consistently | Current working directory, guest code |
| Durable artifact records and assets | `ArtifactStore` | SQLite schema, atomic asset writes, admission transaction, immutable revisions | HTTP server and guest runtime |
| Admission API and policy | Host-side MetaTool broker | Validate one effect in apply mode and call `ArtifactStore::admit_bundle` | HTTP routes, guest JavaScript, compatibility scripts |
| Catalog and rendered presentation | `jcode-artifact-server` | Read from the canonical root and serve GET-only presentation | Admission and candidate status mutation |
| Health and maintenance evidence | T41 and the eventual operator tooling | Report liveness, root identity, recovery, and maintenance outcomes | T40 must not invent a second supervisor |

## Store-root rule

The following rule is normative:

> For a given Jcode home, every artifact writer and every artifact presenter uses
> the same absolute directory `<JCODE_HOME>/artifacts`. The database is
> `<root>/artifacts.sqlite3`, and assets are `<root>/assets`. No component may
> derive an artifact root from its current working directory, workspace name,
> session id, or a second environment variable.

Details:

1. If `JCODE_HOME` is unset, resolve it as the user's `~/.jcode` and then derive
   `~/.jcode/artifacts`.
2. If `JCODE_HOME` is set, resolve that configured directory to an absolute path
   and derive `<resolved JCODE_HOME>/artifacts`.
3. A CLI override is allowed only when it is an explicit absolute path. Tests
   may use an absolute temporary directory. The override is not a second
   default and must be passed to both the server and any host admission code in
   that test or operator process.
4. Create the root and its `assets` directory before advertising readiness. Do
   not use a relative path and hope that the server and MetaTool happen to share
   a working directory.
5. A restart reopens the same database and asset tree. It does not copy,
   migrate, or merge a second store. A missing root is initialized by the host
   store contract; a non-directory, unreadable, or ambiguous root fails startup.
6. The ordinary MetaTool object store remains separate under its existing
   workspace-scoped `JCODE_HOME/metatool/stores/...` path. It is not an artifact
   store and must not be used as one.

The root identity should be visible in startup or readiness diagnostics without
printing credentials or arbitrary host paths to guest code. MetaTool may expose
the logical name `JCODE_HOME/artifacts` in its bounded catalog; host logs and
operator diagnostics may use the resolved absolute path.

## Process lifetime and restart semantics

### Start

- The current supported surface is an explicit invocation of the standalone
  binary, for example `cargo run -p jcode-artifact-server -- ...`, or an
  equivalent installed binary under a user-approved service manager.
- The process stays attached to its launcher unless the service manager owns the
  detachment. The server itself does not daemonize, fork, or register with the
  ordinary Jcode server registry.
- It resolves and validates the canonical root before binding the listener.
  Binding a non-loopback address or opening the root fails before readiness.
- Readiness for T40 is a successfully bound loopback listener plus a store root
  that can be opened. An explicit HTTP health endpoint is deferred to T41.

### Shutdown

- Shutdown is host-controlled. `serve_until` receives a cancellation future;
  the standalone binary maps the process shutdown signals it supports to that
  future.
- Shutdown stops accepting new connections, lets bounded in-flight work finish
  or cancels it, closes the listener, and returns a result that the launcher can
  observe. A graceful shutdown must leave no owned listener task or stale socket
  behind.
- The server has no admission queue to drain. MetaTool admission is a separate
  host operation and must not be routed through the HTTP listener.
- A forced kill is allowed as an operational last resort. SQLite transactions
  and atomic asset writes remain the persistence recovery boundary, but forced
  kill is not reported as graceful shutdown.

### Restart

- Restart is a supervisor operation: stop the old process, wait for its exit and
  socket release, then start a new process with the same absolute root.
- Restart must preserve artifact ids, revision numbers, digests, annotations, and
  candidate rows because they live in the canonical store. The new process must
  be able to list and render records admitted before the restart.
- Two processes may not claim the same listener address. There is no hot
  handoff, socket takeover, or implicit second server for the same root in T40.
- A server restart must not restart MetaTool, invalidate its host broker, or
  change the artifact root. Conversely, MetaTool must remain usable when the
  presentation process is stopped.

## Security boundary

The server is a **local presentation adapter**, not an authentication service or
an admission authority.

- Bind only to loopback. `0.0.0.0`, a public interface, or an arbitrary remote
  address is rejected. Loopback is still reachable by other local processes, so
  it is not a per-user authentication boundary.
- Keep the root under the user-controlled Jcode home or an explicitly approved
  absolute override. Do not accept path traversal, symlink surprises that move
  outside the selected root, or a working-directory-relative fallback.
- Serve only GET presentation routes. POST, PUT, PATCH, DELETE, and unknown
  mutation-like operations remain rejected. The server never calls
  `ArtifactStore::admit_bundle`, changes candidate status, or accepts a guest
  effect.
- Preserve segment validation for artifact, revision, and raw-asset paths. Raw
  assets and rendered HTML are content from the artifact store, not a channel for
  credentials, host configuration, or arbitrary filesystem reads.
- MetaTool is the only guest-facing artifact API. Its capability payload is
  inert data; `admitBundle` queues one effect and the host performs validation
  and persistence. No guest code receives the SQLite path, asset root, or a
  filesystem handle.
- Remote access, authentication, authorization, or multi-user sharing requires
  a separate authenticated boundary such as an approved reverse proxy. T40
  must not broaden the bind address as a convenience.

## Health and limits

### Existing limits that remain authoritative

| Boundary | Current contract |
| --- | --- |
| MetaTool source | 64 KiB maximum JavaScript source |
| MetaTool inputs | 1 MiB serialized input maximum |
| MetaTool execution | 30 s CPU, 60 s wall, 256 MiB heap, 256 KiB output by default |
| Artifact admission | Source and rendered text are each capped at 1 MiB by the host broker |
| Catalog projection | MetaTool catalog and guest sidecar are bounded to 200 entries |
| Admission effects | At most one artifact admission effect per evaluation |
| HTTP request shape | GET-only, one request per connection, no request body; current read buffer is 8 KiB |
| HTTP bind | Loopback only |

MetaTool limits are admission and guest-execution limits. They do not grant the
standalone HTTP server permission to write larger bundles, and the server must
not become a bypass for admission. `ArtifactStore` is a host crate; callers
outside MetaTool remain trusted host code and are responsible for using the
admission contract.

### Known current gaps and ownership

- The server has no explicit `/healthz` or `/readyz` route. T40 must expose
  startup and shutdown outcomes to its launcher; T41 owns an observable health,
  recovery, and maintenance evidence contract.
- The server currently opens the store through `open_migrate` while serving
  requests. This can create directories or apply schema setup even though the
  routes are logically read-only. T40 must ensure root validation is done before
  readiness and must not add domain mutation routes; a true read-only store
  handle is a separate hardening change.
- The current connection loop has an 8 KiB read buffer but no documented idle
  timeout or concurrency budget. T40 must make request/connection lifetime
  bounded and test the chosen bounds, without changing the admission contract.
  T41 should add measurements and recovery evidence for those bounds.
- The current catalog route reads the durable store on demand. A transient
  SQLite or filesystem error is a server error, not evidence that a second root
  should be created. Restart and recovery must continue to use the same root.

## T40 implementation acceptance

T40 is complete only when all of the following are true. These are acceptance
criteria, not an invitation to implement T41 or alter unrelated app-core code.

1. **One root:** the standalone server default resolves to the same absolute
   `<JCODE_HOME>/artifacts` root used by MetaTool. A relative `.jcode/artifacts`
   default is removed. Explicit test/operator overrides are absolute and are
   rejected when ambiguous or invalid.
2. **One owner:** the server remains standalone and foreground/host-supervised.
   T40 does not add ordinary `jcode serve` auto-start, client-owned detachment,
   MetaTool supervision, or HTTP admission routes.
3. **Host shutdown:** `serve_until` has a deterministic shutdown path for the
   supported process signals. It stops accepting, releases the listener, and
   returns cleanly without leaked server tasks. The launcher can distinguish a
   graceful stop from a bind, root, or request error.
4. **Socket lifecycle:** focused tests prove bind rejection for non-loopback,
   clean shutdown, socket release, and successful restart on a new process with
   the same absolute root.
5. **Same-root persistence:** a test admits or seeds an artifact through the
   existing host/store path, starts the server, observes the catalog and
   revision, stops it, restarts it with the identical root, and observes the
   same ids, revision number, and content again. The test must not copy files
   between roots.
6. **Boundary preservation:** GET-only routing, path-segment validation,
   loopback-only binding, and the existing MetaTool one-effect admission bridge
   remain covered. The server never invokes `admit_bundle`.
7. **Bounded transport:** request parsing and connection handling have explicit,
   tested size, timeout, and concurrency behavior consistent with the limits
   above. The server does not grow an unbounded task set while the store or
   launcher is unhealthy.
8. **Diagnostics:** startup/readiness diagnostics identify the resolved store
   root and listener without exposing guest authority or credentials. No health
   claim is made beyond what the tests observe; richer health and maintenance
   evidence remains T41.
9. **Scope discipline:** T40 changes only the artifact server/root/lifecycle
   surfaces and focused tests or docs. It does not rewrite MetaTool admission,
   change the ordinary app daemon lifecycle, or touch unrelated app-core files.

## Explicit non-goals

- No remote artifact hosting or authentication.
- No automatic artifact-server startup from `jcode`, `jcode run`, `jcode serve`,
  or MetaTool.
- No artifact admission, candidate mutation, or revision deletion over HTTP.
- No second per-workspace artifact database.
- No hot restart, shared socket handoff, or cross-process supervisor protocol.
- No implementation of T40 or T41 in this decision task.

## Validation evidence for T39

The decision was grounded against the canonical Tasker task and the current
source contracts listed above. Existing focused tests cover server route
presentation, path traversal rejection, mutation-method rejection, loopback
binding rejection, host-controlled shutdown, MetaTool artifact reconciliation,
and artifact-store persistence primitives. T39 adds no production code or test
fixture; T40 owns the implementation and lifecycle regression coverage.
