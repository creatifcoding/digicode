# Jcode React Native via CopilotKit, AG-UI, and XState

Status: accepted direction, superseding the bespoke mobile protocol plan

Date: 2026-07-28

## Decision

Jcode's React Native frontend will use:

1. **`@copilotkit/react-native`** as the headless React Native integration.
2. **AG-UI** as the standard agent-to-frontend protocol for streaming messages, shared state, frontend tool calls, interrupts, human-in-the-loop interactions, and custom events.
3. **Portable XState v5 machine definitions and snapshots** as the application-level workflow representation carried through AG-UI shared state or custom events.
4. **Jcode on a remote host** as the authoritative execution environment for sessions, models, tools, files, processes, browser automation, swarms, and MetaTool workflows.

Jcode will expose an AG-UI-compatible adapter alongside its existing native WebSocket gateway. Existing TUI, desktop, and native protocol clients remain supported.

## Supersedes

This decision explicitly supersedes the earlier plan to make the React Native application a direct, bespoke client of Jcode's native WebSocket protocol and to invent a Jcode-specific dynamic component/reducer wire format.

Reusable work from that direction may remain, including:

- Host discovery and pairing concepts
- Secure credential storage
- Session discovery primitives
- Native transcript and workflow UI components
- Reconnection and offline UX

The following are no longer canonical:

- A mobile-specific extension of Jcode's native wire protocol
- A proprietary state snapshot/delta format where AG-UI already supplies `STATE_SNAPSHOT` and RFC 6902 `STATE_DELTA`
- Downloading or executing arbitrary reducer JavaScript
- Treating a custom component-tree protocol as the primary agent/frontend interoperability layer

## Architecture

```mermaid
flowchart LR
    RN[React Native UI] --> CK[CopilotKit React Native]
    CK <-->|AG-UI| A[Jcode AG-UI adapter]
    A --> J[Jcode runtime]
    J --> S[Sessions and swarms]
    J --> T[Tools and filesystem]
    J --> M[MetaTool workflows]
    CK --> X[XState v5 actors]
```

### Layer ownership

| Layer | Responsibility |
| --- | --- |
| React Native | Native presentation, device interactions, locally registered components and frontend tools |
| CopilotKit React Native | Headless agent hooks, frontend tools, human-in-the-loop bindings, client integration |
| AG-UI | Bidirectional events, streaming, shared state, snapshots, JSON Patch deltas, tool lifecycle, interrupts, custom events |
| XState | Portable workflow semantics, explicit states/events, local actor interpretation, resumable snapshots |
| Jcode AG-UI adapter | Translation between Jcode runtime events/commands and AG-UI events/runs |
| Jcode runtime | Authoritative execution, persistence, permissions, tools, sessions, swarms, background continuity |
| MetaTool | Dynamic composition of bespoke workflows against available frontend and backend capabilities |

## XState contract

Only serializable, versioned XState machine configuration crosses the wire.

- Pin the supported XState major and a Jcode machine-schema version.
- Reference actions, guards, actors, and delays by registered string identifiers.
- Do not transmit executable JavaScript functions.
- Advertise client implementations and versions as capabilities.
- Execute privileged and durable effects on the Jcode server.
- Persist machine hash, machine revision, authoritative snapshot revision, snapshot, and pending effect IDs.
- Use idempotent event and effect identifiers.

AG-UI transports machine definitions, snapshots, events, and state changes. XState interprets the workflow. AG-UI remains the interoperability protocol.

## Dynamic workflow flow

1. The RN client connects through CopilotKit and advertises registered components, frontend tools, XState implementations, and schema versions.
2. Jcode exposes this capability manifest to MetaTool.
3. MetaTool creates a bespoke workflow using available frontend tools and a portable XState machine.
4. Jcode sends the machine and initial state through AG-UI shared state or custom events.
5. The RN client validates the schema, mounts registered components, and starts the XState actor.
6. Client events travel through AG-UI. Local-only transitions may occur immediately.
7. Privileged effects execute authoritatively on Jcode and return success or failure events.
8. Jcode persists the workflow independently of the mobile connection.
9. After reconnect, AG-UI sends an authoritative state snapshot or deltas and restores the XState actor.

## Initial translation boundary

| Jcode runtime concept | AG-UI surface |
| --- | --- |
| Assistant text streaming | Message streaming events |
| Tool start/input/result | Tool-call lifecycle events |
| User prompt | Agent run input |
| Cancel or steering | Interrupt/steering events |
| Session/workflow state | `STATE_SNAPSHOT` and `STATE_DELTA` |
| Swarm status and plan | Shared state or typed custom events |
| Frontend device operation | Frontend tool call |
| Approval request | Human-in-the-loop interaction |
| Portable XState machine | Shared state or typed custom event |

## Safety and compatibility

- Validate all AG-UI payloads and XState schemas before interpretation.
- Allow only registered frontend tools, actions, guards, and actors.
- Keep host-side permissions authoritative.
- Never expose the gateway directly to the public Internet without a secure transport and access-control boundary. Prefer Tailscale initially.
- Preserve Jcode's existing native protocol and clients.
- Unsupported capabilities must produce explicit degradation or refusal, never silent substitution.

## Delivery sequence

1. Document the Jcode runtime-to-AG-UI event mapping.
2. Implement a minimal AG-UI adapter for prompt input and streaming messages.
3. Add tool lifecycle, cancellation, reconnect, and session identity.
4. Build the RN client with `@copilotkit/react-native`.
5. Add shared state snapshots and RFC 6902 deltas.
6. Carry and restore one portable XState machine.
7. Register frontend tools and human approval.
8. Let MetaTool generate one bespoke end-to-end workflow.
9. Add swarm state and richer dynamic native components.

## Evidence consulted

- CopilotKit React Native quickstart: `@copilotkit/react-native` is a headless React Native package exposing `CopilotKitProvider`, `useAgent`, `useFrontendTool`, `useHumanInTheLoop`, and related hooks.
- AG-UI protocol documentation: AG-UI provides bidirectional agent/frontend events, shared state, `STATE_SNAPSHOT`, and RFC 6902 `STATE_DELTA`.
- Jcode source: the existing WebSocket gateway exposes Jcode's native session protocol and remains useful for current clients, but is not the canonical dynamic RN interoperability layer under this decision.

## Source-grounded implementation findings

A controlled read-only swarm analyzed pinned checkouts of CopilotKit, AG-UI, XState, and the user's local MetaTool implementation. These findings refine the delivery plan.

### CopilotKit React Native

- `@copilotkit/react-native` is a headless package. We own the React Native presentation layer.
- The provider expects a `runtimeUrl` and reuses CopilotKit's platform-neutral hooks, including agent, frontend-tool, and human-in-the-loop behavior.
- The normal client path is a JSON HTTP request with an SSE response, not Jcode's existing WebSocket wire protocol.
- Hermes requires CopilotKit's streams, encoding, crypto, DOM, and location polyfills to load before other CopilotKit imports.
- Mobile validation must cover long-running streams, because the inspected React Native streaming-fetch path has timeout behavior that may affect runs longer than roughly one minute.
- Authentication headers/cookies, app backgrounding, reload replay, frontend-tool calls, and approval round trips require on-device tests rather than browser-only confidence.

### AG-UI

- The practical canonical event contract is the TypeScript Zod schema in the AG-UI repository.
- `RunAgentInput` carries `threadId`, `runId`, current state, messages, tools, context, forwarded properties, optional parent run, and optional resume data.
- The standard TypeScript `HttpAgent` performs a JSON `POST` with `Accept: text/event-stream` and expects JSON events in SSE `data:` fields.
- The minimum lifecycle is bounded by `RUN_STARTED` and `RUN_FINISHED` or `RUN_ERROR`.
- Assistant output uses explicit text-message start/content/end events. Tool calls likewise have a typed lifecycle.
- Shared state already provides `STATE_SNAPSHOT` and RFC 6902 `STATE_DELTA`; Jcode must not invent replacements.
- AG-UI is transport-agnostic conceptually, but its standard TypeScript client path is HTTP/SSE. WebSocket and resumability are capabilities rather than one fully specified universal handshake.
- The base protocol does not provide a mandatory global event sequence suitable for durable replay. Jcode must define a compatible resume extension or recover with authoritative message and state snapshots.
- Session discovery is not the same operation as an AG-UI run. A generic authenticated host/session discovery API may remain useful alongside AG-UI.

### XState v5

- A machine definition and a persisted actor snapshot are separate serializable artifacts.
- `machine.definition` or `machine.toJSON()` preserves topology and symbolic descriptors, not executable implementation functions.
- `actor.getPersistedSnapshot()` produces restorable actor state, including nested children when supported.
- Restoration uses `createActor(machine, { snapshot }).start()` after reconstructing the machine with the correct implementation registry.
- `setup(...)` and `.provide(...)` implementations remain local runtime maps and do not serialize into the machine definition.
- Remote machine definitions must therefore reference allow-listed actions, guards, actors, and delays by stable identifiers.
- The workflow envelope must carry the XState major, Jcode schema version, machine version/hash, implementation requirements, definition, and optional persisted snapshot.

### MetaTool

- The live Pi host exposes a fixed `mt` tool, while package and stored procedure code also contain `cm` and `ms` vocabulary. Migration must preserve aliases at execution boundaries while choosing one canonical Jcode vocabulary.
- MetaTool already separates the host extension, a portable compositional package, and a short-lived evaluator child.
- Its evaluator isolation protects the host event loop with timeout, abort, process termination, console capture, and clone-safe results, but it is not a security sandbox and currently inherits the host environment.
- The portable package composes a stable API object from overlays/plugins and store/runtime services. This is the valuable seam for adding AG-UI/XState workflow capabilities.
- Renderer and host-boundary abstractions support Pi/OMP portability and should become explicit Jcode frontend-capability adapters rather than being discarded.
- The procedure workbench is an implementation-ready design direction, not fully live functionality. The first Jcode integration should target the current `mt` execution and overlay model rather than depending on proposed v2 behavior.

## Refined adapter decision

The first AG-UI adapter should be a **Jcode-hosted authenticated HTTP/SSE endpoint** rather than a WebSocket translation in the RN client:

1. Accept AG-UI `RunAgentInput` via `POST`.
2. Bind `threadId` to a Jcode session and `runId` to one turn attempt.
3. Translate Jcode server events into canonical AG-UI run, message, tool, state, and custom events.
4. Emit SSE keepalives and clean terminal run events.
5. Recover reconnects with authoritative message/state snapshots until a versioned Jcode replay extension is defined.
6. Keep the existing Jcode WebSocket gateway unchanged for native clients.
7. Keep generic host/session discovery separate and reusable.

Jcode currently has no Axum or Hyper dependency. The implementation choice is therefore between a focused HTTP/SSE extension of the existing Tokio gateway and introducing a dedicated HTTP framework. This must be decided with compile-size, maintainability, streaming correctness, authentication, and testability measurements rather than preference.

## Validation gates added by the reference analysis

- Hermes device test with a stream lasting longer than 60 seconds.
- Bearer-token authentication test for AG-UI HTTP/SSE.
- App background, disconnect, reconnect, and authoritative snapshot recovery.
- Frontend-tool invocation and human approval round trip.
- Exact AG-UI event-schema conformance fixtures.
- XState machine definition plus persisted-snapshot restoration with nested actors.
- Refusal of unknown XState actions, guards, actors, delays, and component capabilities.
- MetaTool evaluator timeout, cancellation, clone-safe output, and environment-isolation review.

## Open implementation decisions

- Implement AG-UI directly in Rust or initially run a thin adapter service.
- Whether session discovery belongs in the AG-UI adapter, a separate authenticated host API, or both.
- Exact component capability-manifest shape above AG-UI.
- Which XState effects may execute locally versus requiring server confirmation.
- Snapshot storage ownership and retention policy.
