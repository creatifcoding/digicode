# Expo mobile gateway baseline validation

Date: 2026-08-10

## Disposition

`apps/mobile` is a source-only, gated Expo companion. The Rust gateway it consumes is wired into the Jcode server and its focused pairing/auth tests pass, but this repository does not contain a mobile release binary, app-store artifact, phone activation, simulator activation, or mobile publish workflow.

## Owner-defined checks

- `cd apps/mobile && npm install`: initially failed with npm `ERESOLVE` because Expo Router's optional `react-dom` peer resolved to `19.2.8` while the Expo 54 app uses React `19.1.0`. The bounded fix pins `react-dom: 19.1.0` as a dev dependency and regenerates the lockfile. The same command then succeeded and `npm ls --depth=0` reported a consistent tree.
- `cd apps/mobile && npm test`: **5 suites, 15 tests passed**.
- `cd apps/mobile && npm run typecheck`: **passed** under strict TypeScript settings.
- `cd apps/mobile && npx --no-install expo config --type public`: **passed**, reporting Expo SDK `54.0.0` and iOS, Android, and web config platforms.
- `npm run`: confirms the package declares `start`, `android`, `ios`, `test`, and `typecheck`; there is no owner-defined `lint` or `build` script.
- `npx --no-install expo-doctor`: not available because `expo-doctor` is not installed locally. It is not an owner-defined gate.

The install emitted npm audit advisories for transitive dependencies (10 moderate, 15 high). No broad dependency upgrade was attempted in this bounded baseline slice.

## Gateway contract evidence

The shipped Rust gateway was inspected at `crates/jcode-base/src/gateway.rs`, `crates/jcode-base/src/gateway/auth.rs`, and `crates/jcode-app-core/src/server.rs`:

- `POST /pair` accepts `{ code, device_id, device_name }` and returns `{ token, server_name, server_version }`.
- `GET /health` is available for reachability checks.
- WebSocket clients connect to `/ws`; the gateway accepts `Authorization: Bearer <token>` and the Expo-compatible `?token=<hex-token>` fallback. Query auth is logged as deprecated by the server, but remains supported for React Native WebSocket clients.
- The default gateway port is `7643`, and the server only starts the listener when gateway config is enabled.
- `list_sessions` is a shipped lightweight request and returns `session_list` entries with `session_id`, optional `working_dir`, title, status, and liveness metadata.
- Stateful `subscribe` requires an absolute `working_dir`. A target-session subscribe resumes the existing session and emits `history` with the subscribe request id before the terminal `done` event. The mobile client now supplies the required directory and avoids a duplicate `get_history` in this path.
- The mobile reducer consumes the shipped `session`, `history`, text, tool, message-end, error, and done event names while ignoring unknown additions.

Focused shipped-contract check:

```text
cargo test --profile selfdev -p jcode-base gateway_tests --lib
11 passed, 0 failed, 1282 filtered out
```

## Bounded changes

- Pin the Expo-compatible `react-dom` peer so a clean `npm install` succeeds.
- Include the session's absolute working directory in mobile subscribe requests, with `/` as a deep-link fallback.
- Reattach the selected session after WebSocket reconnect and avoid duplicate history requests when resume already returned history.
- Add focused session protocol regression tests.
- Correct the pairing placeholder/default port and document source-live versus shipped boundaries.

No activation, publish, push, or app-store operation was performed.
