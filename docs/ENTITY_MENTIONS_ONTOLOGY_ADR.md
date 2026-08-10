# ADR: First-Class Entity Mention Ontology

> **Status:** Ratified
> **Date:** 2026-08-10
> **Owner:** Tasker T47, `Implement first-class entity mentions`
> **Scope:** Semantic contract and ownership boundaries. This ADR does not implement the protocol or resolver.

## 1. Decision

Jcode models an entity mention as a staged, host-owned pipeline:

```text
literal span
  -> canonical entity reference
  -> host discovery and submit-time resolution
  -> bounded context attachment
  -> provider-specific request projection
```

The stages are separate because they have different owners, lifetimes, security properties, and failure modes. The canonical flow is:

- **Entity** is the object owned by a domain authority.
- **Entity reference** is the stable address of that object.
- **Mention** is one occurrence of a reference in a user's request.
- **Resolution** is the host's time-bound, policy-bound observation of that reference.
- **Context attachment** is bounded data derived from a successful resolution for one turn.
- **Provider projection** is an ephemeral encoding of that attachment for one provider request.

Only the entity's owner defines canonical identity. A display label, filesystem path, provider continuation ID, Tasker alias, search result, or UI attachment is not an entity identity unless the owning authority explicitly says so.

This ADR ratifies the semantic decisions in [`ENTITY_MENTIONS_SCOPE.md`](proposals/ENTITY_MENTIONS_SCOPE.md). It is the durable contract for T48-T51. Later implementation documents may refine field names or wire envelopes, but they must not merge these concepts or move authority across the boundaries below.

## 2. Digicode prose and architecture standard applied

The applicable Digicode/jcode standard is the repository's existing architecture discipline:

1. One noun names one concept. Do not call a client view, a request payload, and a stored reference an “attachment.”
2. State the owner before describing behavior. A projection is not the source of truth.
3. Distinguish canonical identity from locators, labels, snapshots, and provenance.
4. Keep current behavior, proposed behavior, and non-goals explicit.
5. Make security and failure behavior visible. Never let a model guess an unresolved reference or manufacture authority-bearing fields.
6. Prefer additive, provider-neutral contracts and bounded projections over hidden prompt rewriting.

This follows the existing Tasker rule that one noun should represent one concept and the existing docs convention that current architecture belongs in top-level `docs/*.md` while implementation proposals remain under `docs/proposals/`.

## 3. Vocabulary

### 3.1 Canonical terms

| Term | Definition | Canonical owner | Persisted by default? |
| --- | --- | --- | --- |
| **Entity** | A domain object or observed resource that can be addressed, such as a local file, Jcode session, or Tasker task. An entity is not an arbitrary JSON object or UI row. | The domain store or capability that owns it | The owner's normal policy |
| **Entity reference** | A source-qualified, scope-qualified address of one entity. It is an `EntityReference`, not a path lookup hint. | The entity owner | Yes, in mention metadata |
| **Locator** | A mutable or environment-specific way to find an entity, such as a path, snapshot filename, provider session path, URL spelling, or current working directory. | The resolver adapter | Only as provenance when needed |
| **Literal mention** | The exact user-authored source span and selector, for example `@file:src/app.rs`. It is preserved even when resolution fails. | Request/message owner | Yes |
| **Mention** | A structured occurrence of one literal mention linked to an entity reference, resolution summary, provenance, and optional attachment summary. A mention is not the entity. | Request/message layer, populated by the host | Yes, as an optional sidecar |
| **Resolution** | The result of attempting to bind a mention to an entity under one authorization context, resolver version, and policy version. Resolution is time-bound and can drift. | Host resolver and entity owner | Yes, as bounded metadata |
| **Context attachment** | A bounded, derived payload sent as additional model context for one request. It records what was included, omitted, redacted, or truncated. It is not the canonical entity and is not a permission grant. | Turn/request assembly | Summary by default; full content only by explicit policy |
| **Provider projection** | The provider-specific request representation derived from a context attachment. It is ephemeral and never becomes canonical jcode state. | Provider adapter | No |
| **Provenance** | Evidence about where, when, and under which policy a resolution or projection was produced. Provenance explains a result; it does not define identity or authorization. | Host/runtime | Bounded metadata |
| **Compatibility** | Whether the resolved source supports a requested operation, such as native read, archive preview, continuation, or read-only import. Compatibility is separate from resolution status. | Source adapter | Bounded metadata |
| **Session surface** | A client-side view of a server-owned session. Use this term instead of “attachment” for UI/client relationships. | Client/server session layer | Runtime state |
| **Session binding** | A client connection's association with a session surface and server session. It is not a request attachment and does not change session identity. | Server/client lifecycle | Runtime state |

### 3.2 Terms that must not be overloaded

- **Attachment** means request/context data only in this ontology. A TUI surface, client connection, window, side panel, or session binding is not an attachment.
- **Reference** means an `EntityReference` only when qualified by its owner and scope. A search result, path, display label, or `#` alias is a locator or label.
- **Session** means the server-owned runtime and persisted conversation. It is not a terminal window, process, provider session, or resume command.
- **Prompt** means the provider-visible instruction/context assembly. The user's literal message, the stored message, the wire request, and the provider payload are related representations, not synonyms.
- **Context** means bounded input context. It must not be used as a generic name for authority, session state, or a UI panel.

## 4. Canonical reference shape

The semantic shape is stable even if Rust names change:

```text
EntityReference {
  kind:   EntityKind,
  source: SourceNamespace,
  scope:  AuthorityScope,
  id:     CanonicalId,
}
```

Rules:

- `kind` is a closed, versioned vocabulary at each release boundary. MVP kinds are `file` and `folder`; the staged roadmap adds `session`, `task`, `initiative`, `peer`, and `url`.
- `source` identifies the owning authority, not the provider used for the current model turn. Examples include `local_filesystem`, `jcode_session`, `tasker`, and a source-qualified imported-session namespace.
- `scope` prevents an identifier from being interpreted outside its owner boundary. It may be a workspace-root identity, Jcode session catalog, Tasker project ID, or source catalog. Scope is not a filesystem path string supplied by the model.
- `id` is opaque to generic mention code. Owner-specific parsers validate it. Human aliases and labels never replace it.
- A reference contains no bearer capability, access token, approval, or mutation intent.

### 4.1 Reference identity by entity kind

| Kind | Canonical reference | Locator/label that must not become identity |
| --- | --- | --- |
| `file` | `source=local_filesystem`, `scope=authorized_root_id`, `id=normalized_relative_path` | Absolute path, current working directory string, FFF result row, basename, or display label |
| `folder` | Same as `file`, with the owner-validated normalized directory path | Recursive scan result, descendant list, or “whole repository” label |
| `session` | `source=<session source>`, `scope=<source catalog>`, `id=<stable source ID>`; for Jcode, the persisted `Session.id` | Terminal/process identity, provider continuation ID, JSONL/snapshot path, `ResumeTarget` locator, or short name |
| `task` | `source=tasker`, `scope=<Tasker project ID>`, `id=<opaque TaskId>` | Tasker display alias such as `#47`, title, list position, project root path, or provider/model text |
| `initiative` | `source=initiative`, `scope=<initiative scope>`, `id=<initiative ID>` | Side-panel path, title, milestone label, or current prompt text |
| `peer` | `source=swarm`, `scope=<swarm ID>`, `id=<stable member/session identity>` | Friendly name, process ID, live client connection, or stale status row |
| `url` | `source=url`, `scope=<URL namespace>`, `id=<canonicalized URL>` | Raw spelling, redirect target before explicit fetch, cached page path, or search result |

The current `ResumeTarget` is an operational resume command input. Its source-specific paths remain locators. In particular, a path-only imported Pi record is not a portable `@session` identity until the adapter supplies a stable source ID. Such a record may be archive-only or unavailable for mention resolution; it must not silently use its path as a global ID.

The current Tasker model already separates opaque IDs (`proj_`, `feat_`, `task_`) from human aliases (`#F12`, `#184`) and treats project paths as locators. A mention must preserve that distinction. For the Pi-compatible bridge, the list/project partition is part of the authority scope; a display number is never sufficient outside that scope.

## 5. Mention sidecar shape

A request or persisted message may carry an optional structured sidecar. The sidecar is semantic data, not provider content:

```text
EntityMention {
  literal: {
    raw: string,
    start: character_offset,
    end: character_offset,
    selector: normalized_selector,
  },
  reference: EntityReference?,
  display_label: string,
  resolution: ResolutionSummary,
  provenance: ResolutionProvenance,
  attachment: ContextAttachmentSummary?,
}
```

Required invariants:

1. `literal.raw` and its source span remain recoverable exactly enough for transcript export and replay.
2. `reference` is optional while a mention is unresolved. When present, it is canonical only after host resolution or an explicit selector has produced an owner-validated identity. A discovery suggestion alone is not a reference.
3. `display_label` is presentation only. Changing it cannot change identity.
4. `resolution` and `provenance` explain the observation and policy used. They must not be interpreted as current truth after their freshness boundary passes.
5. `attachment` is optional and summarizes one bounded request projection. It is not the full entity and does not authorize later reads.
6. Multiple mentions of the same entity remain distinct occurrences. Deduplication is an attachment optimization, never an identity rewrite.
7. An unknown or unsupported entity kind is an explicit `unavailable` result with an `unsupported_kind` reason, not an arbitrary plugin object.

For `ambiguous`, `missing`, `forbidden`, `unreadable`, `unavailable`, or `oversize` mentions, the literal selector and resolution attempt remain visible even when no canonical reference is available. A failed attempt never invents one from a display label or fuzzy candidate.

A persisted sidecar should use an additive versioned envelope, for example `entity_mentions` on request/message records. Exact field names are an implementation concern for T48, but the semantics above are not optional. The sidecar must not be encoded as a provider-visible `ContentBlock` and must not depend on generic external-text extraction.

### 5.1 Resolution summary

```text
ResolutionSummary {
  status: resolved | stale | missing | ambiguous | forbidden
           | unreadable | unavailable | oversize,
  compatibility: native | read_only | archive_only
                 | continuation_capable | unavailable | unknown,
  authorized_scope: scope identifier?,
  resolver_version: string,
  policy_version: string,
  resolved_at: timestamp?,
  observed_fingerprint: opaque fingerprint?,
  preview: bounded metadata summary?,
  omissions: bounded reason list,
  sent_bytes: integer?,
  sent_tokens: integer?,
}
```

`compatibility` is optional for local file/folder MVP and required when a source can be archive-only, read-only, or continuation-capable. It is not a second resolution status.

## 6. Ownership and authority boundaries

### 6.1 Owner matrix

| Surface | Owns | Does not own |
| --- | --- | --- |
| Filesystem/project root | File and directory bytes, metadata, symlink behavior, and root identity | FFF ranking, mention syntax, provider payloads, or cross-root authorization |
| FFF discovery | Local suggestions, ranking, frecency, and index freshness hints | Canonical identity, authorization, final metadata, content reads, or telemetry of source content |
| Host resolver | Parsing-independent canonicalization, authorization checks, revalidation, budgets, resolution status, and provenance | Entity ownership, provider continuation, UI labels, or Tasker mutations |
| Jcode session/server | Session IDs, persisted messages, session visibility, session lifecycle, and client/session bindings | Provider session identity, terminal/window identity, or permission implied by a mention |
| Tasker service/store | Project/task/feature IDs, aliases, revision, dependency state, and bounded read projections | User prompt text, provider serialization, or mutation authority granted by a mention |
| Swarm/session runtime | Peer membership, subtree visibility, live status, and bounded reports | Private transcript access, task mutation, or peer control through a mention |
| Composer/client | Literal text, selection state, display chips, and local draft/view state | Canonical IDs, authorization decisions, or provider payload construction |
| Message/session persistence | Literal prompt plus structured mention summaries and provenance | Freshness renewal, entity content ownership, or provider-specific encoding |
| Provider adapter | Request encoding, provider limits, and ephemeral projection | Canonical reference identity, source authorization, or user-visible mention syntax |
| Model/provider | Generated text and tool calls under the existing runtime policy | Authority to create references, bypass resolution, read entities, resume sessions, or mutate Tasker |
| Telemetry | Aggregate status/latency/counters without source content | Paths, prompts, entity payloads, task titles, project roots, or secrets by default |

### 6.2 Authority rule

The host, not the model, owns all authority-bearing values. Runtime context derives the active session, client binding, working directory, project scope, swarm relationship, provider policy, and capability grants. Any `session_id`, `project_id`, `root_id`, task ID, or permission supplied in model-visible text is data to validate, never trusted authority.

A mention is read-only. It cannot execute a file, follow a command, mutate a file, create or update a Tasker task, message a peer, open a side panel as implicit context, resume a session, or authorize a network fetch. Existing policy gates remain authoritative.

## 7. Authorization contract

Authorization is evaluated after parsing/discovery and before content read or attachment construction.

```text
ResolutionContext {
  current_session_id,
  current_session_visibility,
  client_binding_id,
  working_dir,
  authorized_root_or_project_scope,
  swarm/subtree visibility,
  provider/network policy,
  policy_version,
}
```

The resolver must:

1. Normalize and validate a locator before authorization.
2. Resolve paths only within the current authorized root or an explicitly existing policy scope. Reject traversal, symlink escape, special files, and confused-deputy paths.
3. Read local entities only under the existing local read-only boundary. Secret-like and hidden-file policy is visible and testable.
4. Require session visibility/ownership for cross-session reads. A session ID alone is not permission.
5. Require the Tasker service to resolve task references in the current project/list partition. A task mention is a bounded read projection, not a claim, lock, mutation command, or gate bypass.
6. Treat peer status as an observed, potentially stale snapshot. A peer mention does not expose private transcript data or control actions.
7. Make URL fetching a later explicit network capability. Parsing and autocomplete never fetch.
8. Return `forbidden` rather than substituting a same-name candidate or leaking whether a hidden entity exists.

Authorization results are runtime decisions. They must not be serialized into a reusable bearer capability or inferred from an old successful resolution.

## 8. Discovery and resolution

Discovery and resolution are different operations:

1. **Parse:** recognize a namespaced selector while preserving original text. Bare `@word` remains ordinary text unless explicitly selected.
2. **Discover:** offer bounded candidates from an authorized local index or owner query. FFF is a hint provider for local files/folders.
3. **Select:** the user chooses a candidate or supplies an explicit canonical selector. Fuzzy matches never auto-select when ambiguous.
4. **Resolve:** the host revalidates the selected reference against the current filesystem/store snapshot and authorization context.
5. **Preview:** show identity, scope, metadata, compatibility, inclusion policy, and budget impact.
6. **Attach:** construct a deterministic bounded context attachment only when policy allows.
7. **Project:** let the provider adapter encode the attachment without changing canonical identity.

For the same owner snapshot, reference, resolver version, and policy version, resolution and preview are deterministic. A stale index may suggest a candidate, but submit-time revalidation is authoritative.

### 8.1 Resolution states

Every selected mention has exactly one primary resolution state:

- `resolved`: identity verified and an allowed preview is available.
- `stale`: the discovery or prior metadata no longer matches; revalidation may recover it.
- `missing`: no entity exists at the canonical location/ID.
- `ambiguous`: more than one candidate remains and no explicit selection exists.
- `forbidden`: the entity exists but is outside the authorized scope or policy.
- `unreadable`: the entity exists but permissions, encoding, or I/O prevent a safe preview.
- `unavailable`: the required owner/store/capability is not available.
- `oversize`: the entity exists but cannot fit the selected bounded policy.

The resolver never silently changes a failed canonical reference into a similarly named entity. A folder may be `resolved` with partial omissions; the attachment must record excluded, ignored, unreadable, binary, generated, depth-limited, and byte-limited entries as applicable.

## 9. Context attachment and provider serialization

A context attachment is derived from a resolved mention for one turn. It must carry enough bounded accounting to answer “what was sent?”:

```text
ContextAttachmentSummary {
  attachment_kind: entity_context,
  mention_keys: ordered occurrence keys,
  encoding: metadata | text_excerpt | deterministic_manifest,
  source_digest: opaque digest?,
  included_bytes: integer,
  included_tokens: integer?,
  omitted_bytes: integer?,
  omissions: bounded reason list,
  projection_policy_version: string,
}
```

Rules:

- The user's literal text is unchanged. Context is added as clearly delimited, untrusted data associated with the user turn.
- The attachment is metadata-first and bounded. “Attach the whole repository” and unbounded recursive folder expansion do not exist in this contract.
- Entity content cannot override system, developer, or user instructions. Delimiters and provenance must make that distinction explicit.
- `system_reminder` is not an entity attachment channel. Entity context must not be smuggled into hidden reminders.
- Existing `Request::Message.images` and `SoftInterrupt.images` are direct media inputs. Existing `ContentBlock::Image` is a provider-facing media block. Neither is an entity reference or an entity context sidecar.
- Provider adapters may use a text block, structured content part, or other native representation, but the projection must be clearly labeled, bounded, and provider-neutral at the semantic layer.
- A provider adapter may reduce or reject a payload due to provider limits only under a recorded policy. It must preserve omission/truncation accounting in the sidecar or terminal result.
- Provider request encoding is repeatable for the same sidecar, source snapshot, provider policy, and provider adapter version. Provider-native IDs, signatures, encrypted reasoning, and continuation handles remain provider-owned and must not leak into entity identity.
- Mention resolution is never delegated to the model or provider.

The current wire contract has `Request::Message { content, images, system_reminder, no_reply }`; the current shared message model has `ContentBlock` variants for text, reasoning, tool calls/results, images, and provider-specific state. T48 must add the mention sidecar additively at the request/message boundary rather than overload `content`, `images`, or a provider `ContentBlock`.

## 10. History, compaction, export, and resume

### 10.1 Persistence

Persist the literal prompt and the structured mention sidecar on the request/message record that owns turn history. Persist bounded provenance, resolution status, policy versions, fingerprints, omission reasons, and byte/token accounting. Do not persist full local file contents in provenance by default.

If a later replay re-resolves an entity, it must compare the stored fingerprint/policy boundary and expose drift. A historical successful resolution must never be rendered as current truth without an observed-at/freshness indication.

### 10.2 Wire history

The current `HistoryMessage` wire projection contains role/content/tool summaries and has no mention sidecar. T48 may add optional fields so old clients continue to decode and new clients can render mention summaries. A client that does not understand the fields must still receive the literal prompt, not a provider-only expanded payload.

History exports and transcript rendering must distinguish:

1. what the user wrote;
2. which canonical reference was selected;
3. what resolution observed and under which policy;
4. what context was actually attached;
5. what was omitted, redacted, stale, or unavailable.

The full provider projection is not the canonical transcript. It may be reconstructed, bounded, or explicitly unavailable.

### 10.3 Compaction and replay

Compaction preserves literal text, mention occurrence boundaries, canonical reference metadata, status, provenance, and attachment summaries. It must not silently re-run discovery, choose a new candidate, or replace a mention with the provider's expanded text. If an attachment payload is not retained or can no longer be safely reconstructed, mark it unavailable and keep the literal mention visible.

Generic external-text extraction must not be used for `@session` expansion when it could recursively surface encrypted reasoning, signatures, or provider-private state. Session previews are metadata-first and explicitly bounded.

### 10.4 Resume and client attachment

A Jcode session remains identified by its persisted session ID and is owned by the server/session store. `Subscribe.target_session_id`, `ResumeSession.session_id`, `client_instance_id`, local-history flags, and takeover flags are session lifecycle/binding protocol fields, not mention references.

A mention of `@session` never implicitly resumes, continues, takes over, or injects the referenced session. An explicit user action may invoke the normal resume flow, subject to existing ownership and takeover policy. Use **session surface** or **session binding** for the client relationship; reserve **context attachment** for request data.

## 11. Protocol and prompt boundary

The current architecture composes provider prompts from the base system prompt, capability modules, `AGENTS.md` and global instructions, project/global overlays, preferred-tools guidance, skills, memory, turn reminders, and provider-visible tool schemas. Entity mentions do not become a new authority layer.

The host pipeline is:

```text
wire/request literal + optional sidecar
  -> session/message persistence
  -> host authorization and resolution
  -> bounded attachment summary/payload
  -> provider adapter projection
```

The model sees the resulting provider context but cannot author the canonical sidecar, choose among unresolved candidates, or persist a new authority-bearing reference by emitting text. Prompt guidance may explain the feature, but runtime policy and owner stores decide.

Wire evolution requirements:

- Add optional fields; do not change the meaning of existing `content`, `images`, `system_reminder`, `no_reply`, or resume fields.
- Preserve exact literal content for old clients and transcript exports.
- Keep provider-specific serialization behind provider adapters.
- Reject malformed or scope-incomplete references explicitly.
- Never trust model-authored project roots, session IDs, task IDs, or capability flags.

## 12. T48-T51 implementation boundaries

The dependency order is T47 -> T48/T49 -> T50 -> T51. Each task must preserve this ADR.

| Task | Owns | Explicitly does not own |
| --- | --- | --- |
| **T47: Ratify ontology** | This vocabulary, identity/locator split, owner matrix, authorization rule, resolution states, sidecar semantics, history/resume rules, and non-goals | Rust types, provider code, FFF integration, UI behavior, activation, publishing, or pushing |
| **T48: Canonical mention and attachment serialization** | Additive/versioned request and stored-message sidecars; deterministic sidecar serde; bounded attachment summaries; provider adapter projection hooks; backward-compatible wire behavior | FFF discovery, final authorization, entity lookup policy, UI autocomplete, Tasker mutation, or provider-specific canonical identity |
| **T49: FFF entity discovery and insertion** | Workspace-scoped file/folder suggestions, ranking, composer insertion, explicit selection, and index freshness hints | Final authorization, canonical root identity, content reads, provider serialization, session/task resolution, or hidden telemetry of paths/content |
| **T50: Host-owned mention resolution** | Owner adapters, canonicalization, authorization context, submit-time revalidation, resolution states, compatibility, deterministic manifests, budgets, provenance, and read-only enforcement | Provider payload encoding, UI rendering, Tasker mutation, implicit resume, or model candidate selection |
| **T51: Cross-surface validation** | Tests and observed evidence across TUI, remote wire, provider adapters, history, compaction, export, and resume; compatibility fixtures for old clients | New ontology decisions, new resolver authority, new provider syntax, or unrelated runtime activation |

T48 and T49 may proceed independently after this ADR. T50 must consume both their outputs without moving authority into either UI insertion or provider serialization. T51 is a validation gate, not a place to redefine semantics.

## 13. Non-goals

This ADR does not authorize or require:

- a general-purpose knowledge graph or arbitrary plugin-defined entity registry;
- a generic “entity” database table that takes ownership from filesystem, session, Tasker, swarm, or URL authorities;
- provider-specific mention syntax;
- silent fuzzy selection, same-name fallback, or hidden truncation;
- network fetches during parse, autocomplete, or ordinary local resolution;
- automatic cross-session transcript injection, implicit resume, continuation, takeover, or full-history expansion;
- mention-driven file mutation, command execution, Tasker mutation, peer messaging, initiative updates, or side-panel context injection;
- persistence of all source content in provenance or telemetry;
- treating a Tasker alias, path, provider session ID, short name, process ID, or client attachment as canonical identity;
- changing the current prompt stack or adding a hidden instruction channel;
- activation, publication, deployment, push, or release work as part of T47.

## 14. Consequences

Positive consequences:

- Entity owners retain authority and can evolve their stores without a global identity collision.
- The model receives bounded, inspectable context instead of hidden prompt mutation.
- Provider adapters can differ without changing user-facing mention syntax or canonical identity.
- History and resume can expose drift rather than pretending old resolutions are current.
- T48-T51 have narrow, testable boundaries and can be implemented without creating a second session, Tasker, or filesystem authority.

Costs and accepted risks:

- A mention carries more metadata than a plain string.
- Historical replay may be unable to reproduce the original payload when full content was not retained; the contract requires an explicit unavailable/drift result rather than silent substitution.
- Imported sources such as path-only Pi sessions need an adapter-provided stable ID before they can support portable `@session` references.
- Provider adapters must maintain bounded projection and omission accounting for their own limits.

## 15. Validation anchors

The following current sources were inspected and are normative grounding, not new ownership claims:

- [`ENTITY_MENTIONS_SCOPE.md`](proposals/ENTITY_MENTIONS_SCOPE.md): syntax, resolver states, path safety, bounded attachment, provenance, roadmap, and explicit non-goals.
- [`NATIVE_TASKER.md`](proposals/NATIVE_TASKER.md): Tasker ownership, opaque IDs versus aliases, project/path identity, prompt layers, snapshot boundaries, and security rules.
- [`TASKER_CONCURRENCY_ARTIFACTS.md`](proposals/TASKER_CONCURRENCY_ARTIFACTS.md): one-noun terminology and Git/Tasker ownership separation.
- [`SERVER_ARCHITECTURE.md`](SERVER_ARCHITECTURE.md): server-owned sessions, client connections, session lifecycle, and working-directory boundaries.
- [`MULTI_SESSION_CLIENT_ARCHITECTURE.md`](MULTI_SESSION_CLIENT_ARCHITECTURE.md): session versus surface/client state. This ADR intentionally reserves “attachment” for request context.
- [`RESUME_BEHAVIOR.md`](RESUME_BEHAVIOR.md): explicit resume behavior and saved-session UI actions.
- [`wire.rs`](../crates/jcode-protocol/src/wire.rs): current message, subscribe, resume, and media wire fields.
- [`jcode-message-types`](../crates/jcode-message-types/src/lib.rs): current `Message` and `ContentBlock` model.
- [`jcode-session-types`](../crates/jcode-session-types/src/lib.rs): current `StoredMessage`, `ResumeTarget`, session status, and rendered media model.
- [`Tasker domain types`](../crates/jcode-tasker-types/src/domain.rs): opaque Tasker IDs, project scope, task aliases, and task state.
- [`OpenAI request builder`](../crates/jcode-provider-openai/src/request.rs) and [`Anthropic adapter`](../crates/jcode-provider-anthropic/src/lib.rs): provider-specific projection boundaries.

A future ADR may change this ontology only by naming the replacement concepts, migrating the references, and explicitly revising the affected T48-T51 contract. A code change or provider quirk alone does not supersede this document.
