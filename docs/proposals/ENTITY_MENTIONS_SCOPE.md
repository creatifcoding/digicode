# Scope: First-Class `@entity` Mentions

> **Status:** Execution-ready proposal; ontology ratified in [`../ENTITY_MENTIONS_ONTOLOGY_ADR.md`](../ENTITY_MENTIONS_ONTOLOGY_ADR.md)
> **Date:** 2026-07-28
> **Decision requested:** Approve the staged entity-mention implementation plan after adopting the ratified ontology.

## 1. Outcome

Jcode should let a user refer to local and Jcode-owned objects directly in a prompt instead of pasting paths, IDs, or large excerpts. A mention is a structured reference that Jcode can discover, resolve, preview, bound, authorize, and explain to the user and model.

The first release supports only local **files** and **folders**. Later releases add sessions, tasks, initiatives, peers, and URLs without changing the core mention contract.

The key product promise is:

> If Jcode accepts an `@entity` mention, the user can inspect exactly what it resolved, what was sent to the model, and where it came from. If it cannot resolve safely, it says so and does not guess.

This scope is intentionally about reference semantics and user workflow. It does not prescribe a particular parser, UI toolkit, or storage schema. It does prescribe FFF's native Rust engine as the file/folder discovery provider so Jcode does not build and maintain a competing workspace index.

## 2. Repository-grounded constraints

- A session already has a durable ID, short name, working directory, status, provider/model metadata, persisted messages, and transcript export paths. Session search indexes metadata and bounded context from local and imported sources.
- Initiatives are durable, project- or globally-scoped goals with sanitized IDs, status, milestones, success criteria, blockers, progress, updates, and optional side-panel pages.
- Swarm members expose session identity plus friendly name, lifecycle status, runtime status, task/progress information, and completion reports. Parent/child ownership and subtree boundaries are meaningful permissions.
- Transcript rendering distinguishes stored messages from synthetic UI messages, supports tool results and inline media, and already has context-window compaction and output-size caps.
- The safety model treats local read-only inspection as auto-allowed, while external communication, mutation, and actions outside the local sandbox require stronger approval. Entity resolution must preserve this boundary.
- File and folder discovery must use the native `fff-search` Rust crate. The unrelated crate named `fff` is a finite-field library and must not be added. Jcode should reuse FFF's long-lived workspace index, mixed file/directory fuzzy search, frecency, git awareness, ignore handling, metadata, binary classification, and filesystem watcher rather than implementing a parallel crawler or ranking system.

These existing concepts should be reused rather than inventing a parallel identity system.

## 3. User workflows

### 3.1 Mention a file

1. User types `Review @file:src/app.rs for error handling`.
2. After `@`, the composer offers entity kinds and recent/relevant candidates.
3. After selecting a file, the composer displays a compact chip or syntax-preserving token.
4. Before submit, the user can expand a preview showing path, size, modified time, and the bounded content excerpt that will be sent.
5. On submit, Jcode resolves the file relative to the current session working directory, applies configured size limits, and adds a structured context attachment to the turn.
6. The transcript preserves the literal mention and records the resolution summary separately.

### 3.2 Mention a folder

1. User types `@folder:crates/jcode-app-core`.
2. Discovery shows the folder and a count/size summary, not every descendant as individual autocomplete rows.
3. Preview shows the root, selected file count, ignored/excluded count, total candidate bytes, and the inclusion policy.
4. Submit resolves the folder to a deterministic bounded manifest and selected file excerpts. It does not recursively dump an unbounded tree.

### 3.3 Correct an ambiguous or invalid mention

- Ambiguous names show candidate paths and the reason each candidate matched.
- Missing, unreadable, or out-of-scope entities remain visible as unresolved warnings with an edit/remove action.
- The user can submit with unresolved mentions only if the product explicitly supports a “send literal only” choice. The default is to block submission when a selected mention cannot be resolved, preventing silent context loss.

### 3.4 Use mentions in later entity stages

The same interaction should work for `@session`, `@task`, `@initiative`, `@peer`, and `@url`: discover, preview, confirm, resolve, attach bounded context, and show provenance. Each entity kind defines its own identity and safe preview, but the composer and transcript contract stays shared.

## 4. Syntax contract

### 4.1 Canonical syntax

Use a namespaced form that is unambiguous and extensible:

- `@file:<path>`
- `@folder:<path>`
- Later: `@session:<source>/<source-id>`, `@task:<id>`, `@initiative:<id>`, `@peer:<id>`, `@url:<normalized-url>`

Examples:

- `@file:src/main.rs`
- `@file:"docs/Design Notes.md"`
- `@folder:crates/jcode-app-core`
- `@session:jcode/jcode_01J...`

The parser must also accept a UI-created mention token whose display label is shorter than its canonical value. The canonical value, not the display label, is what is persisted and resolved.

### 4.2 Lexing rules

- A mention begins at `@` when it appears at the start of input or after whitespace/punctuation, unless escaped as `\\@`.
- Kind names are case-insensitive for parsing and normalized to lowercase.
- Paths may contain spaces when quoted. Backslash and quote escaping must be deterministic.
- Bare `@foo` is not an entity in MVP. It remains ordinary text unless the user chooses a kind from autocomplete.
- Mentions inside fenced code blocks, inline code, or quoted user text are not auto-resolved unless explicitly selected. This avoids rewriting examples and source snippets.
- Exact original user text remains available for transcript export and replay.

### 4.3 Canonical identity versus display label

Every resolved mention has:

- `kind`
- canonical identifier
- display label
- source/scope
- resolution status
- provenance
- bounded preview metadata

For files and folders, the canonical identifier is a normalized path relative to the session working directory when safely possible, plus the resolved root identity needed to prevent working-directory drift. Absolute paths are displayed only when necessary and are subject to redaction settings.

For sessions, canonical identity is a stable `{source, source_id}` pair. A mutable locator such as a snapshot path, JSONL path, or provider-specific resume path is never the identity and is stored only in provenance. This prevents imported-session IDs from changing when stores are moved or reindexed.

## 5. Discovery and autocomplete

### 5.1 Invocation

Autocomplete opens after `@`, after `@<kind>:` and while editing the selector. It must be keyboard-first and usable without a mouse. The first menu groups by kind, then shows recent/relevant candidates.

### 5.2 MVP discovery sources

- `file` and `folder` candidates from an FFF index rooted at the current working directory.
- Current input prefix and path segments.
- Recently mentioned entities in the current session.
- FFF frecency and query-history ranking within the authorized root.
- Explicitly typed paths, even when they are not indexed.

Do not scan the entire home directory by default. Discovery is scoped to the active project/session root and configured include/exclude rules.

### 5.2.1 FFF lifecycle

- Add `fff-search` as the native dependency and construct one long-lived picker/index per active workspace root, shared by composer discovery requests for that root.
- Initialize and scan asynchronously. Cold-index progress must not block typing; a selected result is always revalidated by Jcode's authoritative resolver at submit time.
- Reuse FFF watcher updates instead of running periodic full rescans. Workspace/root changes create or select the matching scoped index.
- Use FFF's mixed search for the combined file/folder menu and its typed metadata for result rendering. Jcode remains responsible for authorization, secret policy, preview construction, byte/token budgeting, and final content reads.
- Feed successful mention selections back into FFF frecency/query tracking when the API supports it. Do not persist raw query text or paths in Jcode telemetry.
- Bound resident memory and idle workspace indexes. The implementation must define eviction or teardown for inactive roots rather than retaining every workspace index indefinitely.

### 5.3 Ranking

Rank exact path/prefix matches first, then basename matches, then fuzzy matches. Prefer files/folders recently opened or mentioned in the current session, but never allow recency to override an exact path match.

Every candidate row shows enough disambiguating information to avoid selecting the wrong item: kind, relative path, status marker, size/count, and a concise match reason.

### 5.4 Performance target

- First suggestions visible within 100 ms for a warm index.
- Typing remains responsive while a cold scan runs in the background.
- A stale index may provide suggestions, but selected candidates must be revalidated at submit time.
- Autocomplete must cap candidate rows and never materialize all descendants of a large folder.

## 6. Resolution model

Resolution is a separate step from parsing and discovery.

### 6.1 States

Every mention resolves to exactly one state:

- **resolved:** identity verified and preview available.
- **stale:** candidate existed in the index but metadata/path changed; revalidation may recover it.
- **missing:** no entity exists at the canonical location/ID.
- **ambiguous:** more than one candidate matches and no explicit selection was made.
- **forbidden:** entity exists but is outside the allowed scope or policy.
- **unreadable:** entity exists but permissions, encoding, or I/O prevent safe preview.
- **unavailable:** required provider/store/tool is not available, primarily for later external entities.
- **oversize:** entity exists but exceeds configured limits and cannot be reduced under the selected policy.

The resolver must never silently fall back from a failed canonical reference to a similarly named candidate.

Later entity kinds may expose a separate compatibility result rather than overloading resolution status. For sessions, the resolver must report whether the record is native, imported read-only, archive-only, continuation-capable, or unavailable. Compatibility is informational and actionable; it never authorizes access by itself.

### 6.2 File resolution

1. Parse and normalize the path.
2. Resolve it against the session working directory or an explicitly approved project root.
3. Reject traversal outside the allowed root unless the user explicitly expands scope and policy permits it.
4. Verify it is a regular file, readable, and below file-size limits.
5. Capture metadata and produce a bounded content selection.

### 6.3 Folder resolution

1. Resolve and authorize the root directory.
2. Enumerate descendants deterministically.
3. Apply ignore rules, hidden/generated/binary policy, symlink policy, file-count cap, depth cap, and byte cap.
4. Produce a manifest with included, excluded, truncated, and unreadable entries.
5. Select content according to a documented policy, such as text-first and shallow-first, while retaining deterministic ordering.

Symlinks must not escape the authorized root. Special files, sockets, device files, and executable invocation are never read as ordinary folder content.

## 7. Context preview and attachment

Before submit, the user can inspect a preview for each mention and the aggregate context attachment.

### 7.1 Preview contents

For `@file`:

- canonical/display path
- root/project scope
- file type and encoding classification
- byte size and line count when available
- modified time
- selected range or excerpt policy
- included bytes/tokens
- truncation indicator
- ignored/redacted content indicator

For `@folder`:

- root path
- included file count and total bytes
- excluded file count and reasons
- maximum depth reached
- binary/generated/hidden/ignored counts
- top-level manifest and selected excerpts
- aggregate bytes/tokens

The preview is metadata-first. Content is collapsed by default for large entities.

### 7.2 What the model receives

The model receives a structured, clearly delimited context block separate from the user's prose. The attachment includes the canonical mention, resolved identity, bounded content, and provenance. It must not be presented as an instruction from the file. File content is untrusted data and cannot override system/developer/user instructions.

### 7.3 Explicit expansion

MVP supports one explicit expansion action: increase the selected entity's bounded excerpt within the remaining turn budget. It does not support arbitrary “include everything” behavior.

## 8. Token and size controls

The system needs hard limits before implementation, with configuration defaults that are safe for the smallest supported context window.

Recommended initial defaults:

- 32 mentions per turn.
- 8 MB raw bytes per file mention before truncation.
- 256 files and 20 MB raw bytes per folder mention before selection.
- 4 MB aggregate raw bytes across all entity attachments.
- 15% of the provider context budget reserved as the maximum entity-attachment budget, with a fixed lower ceiling for very large contexts.
- 8,000 model tokens per mention by default and 24,000 aggregate model tokens per turn, subject to provider budget.
- Preview payloads capped independently from model payloads.

The exact numeric values are tunable, but the contract must include:

- per-mention and aggregate limits
- byte-to-token accounting before submit
- deterministic truncation order
- a visible explanation when content is omitted
- no automatic compaction that changes the user's selected entity silently
- fail-safe behavior when accounting is unavailable

When limits are exceeded, offer: reduce to preview policy, remove the mention, or cancel submit. Never silently send an arbitrarily truncated folder.

## 9. Ambiguity, missing, and stale entities

### 9.1 Ambiguity

- Exact canonical paths resolve immediately.
- Basename or fuzzy matches require a user selection when multiple candidates remain.
- The UI shows candidate path, kind, size, modified time, and match reason.
- The model is never asked to choose among unresolved candidates.

### 9.2 Missing

A missing entity renders as `@file:... (missing)` or equivalent structured status. The user can edit, remove, or retry resolution. The original literal remains intact in the transcript.

### 9.3 Stale

Discovery results carry an index timestamp. Submit revalidates against current filesystem metadata. If changed, the preview refreshes and the user gets a lightweight “changed since discovery” indicator. A stale item may resolve if its canonical identity is still valid; if not, it becomes missing or ambiguous.

### 9.4 Partial folder resolution

A folder can be resolved while containing excluded/unreadable children. The user must see the partial status and counts before submit. The attachment records exactly which policy caused omission.

## 10. Security and privacy boundaries

- Resolution is read-only. Mentions never execute files, follow arbitrary commands, mutate files, or grant access to a new root.
- Default scope is the active session working directory/project root. Parent directories, home directories, mounted secrets, and external paths are denied unless an explicit existing policy allows them.
- Normalize paths before authorization. Prevent `..` traversal, symlink escapes, and confused-deputy behavior across remote/local session boundaries.
- Hidden files, environment files, credentials, private keys, and known secret patterns should be excluded by default or marked for explicit confirmation. The product must make redaction behavior visible.
- Never send local file contents to discovery services, telemetry, or third-party search. Autocomplete and resolution run locally for MVP.
- URL mentions are later-stage and must not fetch on parse or autocomplete. Fetching network content is an explicit resolution action with its own permission, timeout, size, content-type, redirect, and prompt-injection controls.
- Cross-session, peer, and imported-session access must obey ownership and visibility boundaries. A mention must not turn a session ID into permission to read another session's private transcript.
- Entity content is untrusted context. Provenance and delimiters must make it impossible to confuse it with system or user instructions.
- Telemetry may record aggregate counts, latency, and resolution status, but not paths, file contents, URLs, prompts, or entity payloads by default.

## 11. Provenance and transcript representation

### 11.1 Provenance contract

For every mention attachment, record:

- literal source span in the user message
- canonical kind and identifier
- display label at time of resolution
- source (`local_filesystem`, later `jcode_session`, `swarm`, `url`, etc.)
- session/project scope used for resolution
- resolver version/policy version
- resolved-at timestamp
- metadata fingerprint sufficient to detect staleness without storing content
- resolution state and omission/truncation reasons
- byte/token counts sent to the model

Do not store full file contents in provenance when the transcript can be replayed by re-resolving the entity. If reproducible replay requires content snapshots, that must be a separate opt-in decision.

For sessions, store `{source, source_id}` as the canonical reference and retain any mutable locator/path only as provenance. The locator is evidence about where the resolver found the record, not a portable identity.

### 11.2 User-visible transcript

The user message remains readable as typed:

```text
User: Review @file:src/app.rs and compare it with @folder:tests.
```

The rendered transcript adds a compact attachment row immediately below the message:

```text
Context attached: 2 entities · 1.8k tokens
- @file:src/app.rs → resolved · 412 lines · 3.1 KB
- @folder:tests → resolved partially · 14 files · 2 excluded · 1.4k tokens
```

Selecting the row opens the same metadata/provenance preview. It must distinguish:

- what the user wrote
- what was resolved
- what was actually sent
- what was omitted or redacted

Exports and replay should preserve the literal mention plus structured attachment metadata. They must not make a stale historical resolution appear current.

### 11.3 Structured mention sidecars

Mention metadata should be represented as an optional structured sidecar on the request/message model, including `Request` and persisted `StoredMessage` where those types own turn history. It must not be encoded as a provider-visible `ContentBlock` and must not rely on the generic external-text extraction path.

The sidecar carries the literal source span, canonical identity, resolution/compatibility status, provenance, preview summary, and byte/token accounting. Provider adapters may derive a bounded, clearly delimited attachment from the sidecar at request-build time, but the sidecar itself remains available for transcript rendering, persistence, replay, and compaction.

### 11.4 Model-visible transcript

The model-facing message contains a clearly labeled context attachment, not a hidden mutation of the user's text. The attachment is stable enough for providers and compaction to preserve, but compact enough that transcript rendering does not duplicate the full payload.

For `@session`, model-visible expansion is metadata-first and bounded. It never implicitly resumes the referenced session, injects its full history, or invokes generic external-text extraction that could recursively surface encrypted reasoning or signatures. A user must explicitly choose an archive preview or a continuation-oriented preview when both are supported.

## 12. Entity roadmap and boundaries

### Stage 1: `@file`, `@folder` MVP

- Local discovery and autocomplete.
- Safe path resolution under current project/session root.
- Deterministic previews and bounded attachments.
- Missing/stale/ambiguous/forbidden/oversize states.
- Transcript attachment rows and provenance.
- No network calls and no cross-session reads.

### Stage 2: `@session`

- Jcode sessions first, then imported Pi/OpenCode/Codex/Cursor sources where compatibility exists.
- Canonical identity is the source-qualified `{source, source_id}` pair. Mutable snapshot/store locators are provenance only. For example, Pi's current path-shaped resume target must not become the portable mention ID.
- Metadata-first preview: title/short name, status, provider/model, working directory, timestamps, and bounded excerpts.
- Resolver exposes compatibility state and an explicit archive-vs-continuation choice when the source supports both. Imported records may be archive-only or read-only.
- Read-only access governed by session visibility and ownership.
- External-store unavailable state is explicit; no fallback to same-name sessions.
- No implicit resume, no full-history injection, and no generic external-text extraction for mention expansion. Any provider/source-specific excerpt path must be deliberately bounded and must exclude encrypted reasoning/signatures.

### Stage 3: `@task`, `@initiative`

- Tasks resolve to durable task IDs and bounded status/description/acceptance context.
- Initiatives resolve to existing goal IDs, scope, status, current milestone, next steps, blockers, and success criteria. Side-panel links may be offered as a UI action, but opening a panel is not implicit model context.
- User/project/global scope must be displayed and enforced.

### Stage 4: `@peer`

- Peers resolve to current swarm member identity, friendly name, session ID, lifecycle/runtime status, ownership relation, and latest bounded completion report.
- Access must respect subtree and swarm visibility. A peer mention does not automatically send private transcript content or allow control actions.
- Live status can become stale; show observed-at time.

### Stage 5: `@url`

- Canonicalize and validate URLs without fetching during autocomplete.
- Explicit fetch/resolve action with network safety limits, redirect policy, content-type handling, cache freshness, size/token caps, and prompt-injection isolation.
- Provenance includes final URL, redirects, fetched-at time, and source status.

## 13. Explicit non-goals

- No implementation in this scope change.
- No general-purpose knowledge graph or arbitrary plugin-defined entity registry in MVP.
- No unbounded recursive folder dump or “attach the whole repository” button.
- No implicit network fetching for URLs.
- No automatic cross-session transcript injection.
- No implicit session resume, continuation, or full-history expansion from an `@session` mention.
- No use of generic external-text extraction for session mention expansion when it can recursively surface encrypted reasoning or signatures.
- No mutation, command execution, task control, peer messaging, or initiative updates through mentions.
- No user-facing requirement that every plain `@word` becomes a mention.
- No provider-specific mention syntax.
- No silent fallback, fuzzy auto-selection, or hidden truncation.
- No telemetry collection of source content.
- No guarantee that a historical mention can be re-resolved identically after files or sessions change; replay must expose drift.

## 14. Acceptance criteria

### Product behavior

1. A user can create, edit, remove, and submit `@file` and `@folder` mentions from the composer without manually copying absolute paths.
2. Autocomplete is scoped to the active project/session root, supports prefix and fuzzy matching, and shows disambiguating metadata.
3. Exact paths resolve deterministically. Ambiguous, missing, stale, forbidden, unreadable, and oversize states are visible and actionable.
4. A pre-submit preview shows the entity identity, inclusion policy, counts, and byte/token impact.
5. Folder resolution is deterministic and bounded. The UI identifies omitted files and reasons.
6. Submit-time revalidation catches changes after autocomplete.
7. The model receives only the selected bounded attachment and a clear statement of omissions/truncation.
8. The user can inspect the difference between literal text, resolved entity, and attached payload in the transcript.
9. A failed resolution never silently substitutes another entity.
10. Limits prevent one or many mentions from exhausting the provider context budget.

### Safety and privacy

11. Path traversal and symlink escapes are rejected by tests and at runtime.
12. Default resolution cannot read outside the active authorized root.
13. Secret-like and hidden-file policy is visible and testable.
14. MVP performs no network access during discovery or resolution.
15. Entity contents are not emitted to telemetry or external services.
16. File content is delimited as untrusted context and cannot override higher-priority instructions.

### Persistence and compatibility

17. Existing prompts without mentions are unchanged.
18. Existing transcript exports remain readable and retain the literal prompt text.
19. Structured mention metadata survives session save/reload and is not duplicated as full payload on every render.
20. Context compaction and replay preserve attachment boundaries, status, and provenance, or explicitly mark unavailable historical payloads.
21. Mention metadata is persisted as an optional structured sidecar on request/message records, including `StoredMessage`, and is not represented as a provider-visible `ContentBlock`.
22. `@session` uses a source-qualified canonical identity separate from mutable locators, reports compatibility state, and requires an explicit archive-vs-continuation choice where applicable.
23. `@session` never implicitly resumes, injects full history, or uses generic external-text extraction that can surface encrypted reasoning/signatures.
24. The contract leaves room for source-qualified `@session` IDs and later entity kinds without a breaking syntax change.

### Quality targets

25. Warm autocomplete suggestions appear within 100 ms and do not block typing during cold scans.
26. Resolution and preview are deterministic for the same filesystem snapshot and policy.
27. Unit, integration, and manual UI coverage exercise at least one happy path and every resolver failure state.

## 15. Staged milestones

### Milestone 0: Contract and policy lock

- Approve syntax, entity attachment shape, resolver states, default limits, path scope, hidden/secret policy, and transcript rendering contract.
- Produce fixture examples for files, folders, stale changes, ambiguity, symlink escape, and partial folder resolution.
- Define metrics that exclude source content.

**Exit:** API/UI owners agree on the stable concepts and unresolved decisions are listed explicitly.

### Milestone 1: Local resolver core

- Integrate `fff-search` and implement a workspace-scoped index lifecycle with cold-scan progress, watcher updates, and bounded idle-index retention.
- Implement parser-independent canonical path normalization and authorization. FFF results are discovery hints, not authorization decisions.
- Implement FFF-backed mixed file/folder discovery plus authoritative resolution, deterministic manifests, metadata fingerprints, and bounded selection.
- Add structured status/provenance output.

**Exit:** resolver tests pass for every security and failure state; FFF discovery latency and cold-scan behavior meet the target; no UI or provider changes are required to inspect results.

### Milestone 2: Composer discovery and preview

- Add `@` invocation, kind/path autocomplete, keyboard selection, editing/removal, and pre-submit preview.
- Add visible limit accounting and partial-resolution warnings.

**Exit:** a user can complete the file/folder workflows end to end without manually invoking a tool.

### Milestone 3: Turn attachment and transcript integration

- Attach bounded entity context to model-facing turns.
- Persist structured mention records with messages.
- Render compact transcript attachment rows and expose provenance/details.
- Verify compaction, export, replay, and reload behavior.

**Exit:** acceptance criteria 1–10 and 17–23 pass in TUI/desktop surfaces that support the composer.

### Milestone 4: Hardening and rollout

- Run performance tests on large repositories and cold indexes.
- Run security tests for traversal, symlinks, secrets, hidden files, and boundary changes.
- Add feature flag, diagnostics, opt-in telemetry counters, and rollback path.
- Dogfood with representative projects and provider context sizes.

**Exit:** rollout checklist is green and failures degrade to literal text plus a clear warning rather than hidden context changes.

### Milestone 5+: Later entity kinds

Ship one kind at a time behind the same resolver/preview/transcript contract: sessions, tasks, initiatives, peers, then URLs. Each kind requires its own identity, visibility, staleness, source availability, and security review before enabling autocomplete.

## 16. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Folder mentions consume the context window | Truncated or failed turns | Per-mention/aggregate budgets, deterministic selection, visible accounting, hard caps |
| Fuzzy matching selects the wrong file | Incorrect model advice | Exact-first ranking, ambiguity state, explicit confirmation, no silent fallback |
| Path/symlink escape leaks secrets | Security/privacy incident | Root authorization after normalization, symlink policy, secret filters, negative tests |
| Index is stale during editing | Surprising previews or missing context | Submit-time revalidation, metadata fingerprint, changed-since-discovery notice |
| Long-lived FFF indexes increase resident memory | Jcode retains excessive memory across many workspaces | One shared index per active root, measurable memory diagnostics, bounded idle-root retention, explicit teardown |
| File content contains prompt injection | Model follows untrusted file instructions | Delimited attachment, provenance, system/developer instruction precedence, security tests |
| Transcript duplicates large payloads | Storage/UI/memory growth | Persist references and bounded metadata, not repeated full payloads; preserve snapshots only by explicit decision |
| Imported sessions have incompatible identity/storage | Broken later `@session` references or unsafe continuation | Source-qualified `{source, source_id}` identity, locator-only provenance, explicit compatibility/archive-vs-continuation state, read-only previews, bridge compatibility tests |
| Session expansion surfaces encrypted reasoning/signatures | Sensitive or unusable context reaches the model | Structured mention sidecars, deliberate bounded source adapters, no generic external-text extraction, no implicit full-history injection |
| Peer names and task titles collide | Wrong coordination context | Canonical IDs plus display labels, ownership/visibility checks, observed-at timestamps |
| URL fetching expands attack surface | SSRF, large payload, prompt injection | Defer URLs, explicit fetch, redirect/content-type/size controls, isolation and permissions |
| Mention syntax harms ordinary prose/code | Compatibility regressions | Require namespaced kind syntax or explicit autocomplete selection; ignore code spans and escaped `@` |
| Multiple UI surfaces diverge | Inconsistent behavior | Shared resolver/attachment contract, surface-specific rendering only |

## 17. Open decisions before implementation

1. Exact default byte/token limits and whether users may configure them per project.
2. Whether unresolved mentions block submit or permit “literal only” submission in MVP.
3. Default hidden/secret file policy and the redaction detector's false-positive handling.
4. Whether to persist only metadata/fingerprints or optional content snapshots for reproducible replay.
5. The canonical attachment serialization shared by TUI, desktop, remote sessions, and providers.
6. Whether the UI stores a structured token in the composer or reparses source text on submit.
7. Which imported session stores are available for Stage 2 and how their source-qualified `{source, source_id}` identities map to existing session search, including Pi's header ID versus path-shaped resume locator and archive-only versus continuation-capable records.
8. Whether `@initiative` is an alias for the existing `Goal` domain type or a compatibility layer over it.
9. Whether URL entities are fetched by the resolver or by an explicit user-approved tool action.
10. Which request/message types own the optional mention sidecar and how provider adapters derive attachments without exposing the sidecar as a `ContentBlock`.
11. Which transcript surfaces ship in Milestone 3 and how compact attachment rows behave in narrow TUI layouts.
12. The exact FFF index ownership boundary: daemon-owned for all clients, client-local for local composers, or a shared abstraction with daemon-authoritative revalidation. Prefer daemon ownership when remote clients must receive identical rankings and lifecycle behavior.

## 18. Definition of done for this scope

This proposal is ready to hand to implementation when Milestone 0 decisions are recorded, the entity attachment/provenance contract is accepted by session/transcript owners, the path-security policy is approved, and the acceptance fixtures are identified. No production code should be changed until those gates are met.
