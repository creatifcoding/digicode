# Side-panel ownership and view-state contract

This crate is the renderer-neutral boundary for the durable Alt+M workspace.
The existing `SidePanelPage` and `SidePanelSnapshot` payloads remain compatible
with the current session-only tool, server, and TUI pipeline. The types below
describe the ownership model that the later storage, picker, action, and UI
tasks can adopt without changing the document model again.

## Ownership

`PageScope` identifies who owns a document:

| Scope | Identity | Meaning |
| --- | --- | --- |
| `Session(SessionId)` | conversation ID | Temporary working set for one conversation. |
| `Project(CanonicalProjectRoot)` | canonical project root | Shared by sessions in the same project. |
| `Global(UserProfileId)` | user profile ID | User library available across projects. |

Project scope must use a canonical root, not the spelling of the current
working directory. `CanonicalProjectRoot::from_path` provides the filesystem
boundary for callers that need to establish that identity.

`Ephemeral` pages are legal only in Session scope. Managed and linked pages may
be owned by any scope. The type-level `PageDocumentRecord::new` constructor
and `PageScope::allows_source` enforce this boundary without knowing how a
storage adapter works.

## Document versus workspace reference

`PageDocumentRecord` is shared document metadata:

- scoped page identity
- source kind
- title
- monotonic document revision

`WorkspacePageReference` is local session state:

- pin and local ordering
- last-viewed recency
- per-page `PageViewState`

A Project or Global document is stored once. Multiple sessions can reference
it and keep different reading position, focus, graph transform, search, and
selection state. Closing a reference is therefore distinct from deleting its
document, and linked source files remain outside the reference lifecycle.

`SidePanelWorkspaceState` makes the session ownership of references explicit.
Its references may point to Session, Project, or Global documents, while all
focus, pin, order, recency, and view fields belong to that session.

## Per-page view state

`PageViewState` contains portable values only:

- horizontal and vertical reading scroll offsets
- integer graph zoom and pan values
- collapsed section IDs
- search query and match index
- selected and focused element IDs

It deliberately does not name Ratatui widgets, Mermaid models, Tasker nodes,
swarm members, or other domain adapters. Later renderer contracts can map
these values to terminal or desktop coordinates without changing ownership.

## Non-goals for this contract slice

- No Project or Global storage migration is performed here.
- No page picker, rail, action menu, or Alt+M layout is implemented here.
- No graph input routing or renderer adapter is implemented here.
- No provider/model behavior or app-core agent test is changed here.

The current legacy session directory remains a storage and migration concern
for the downstream scoped-catalog task. This contract only makes the ownership
and view-state boundaries explicit and serializable.
