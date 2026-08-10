# Setup and security

## Pair a device

1. Enable the gateway in the host config, restart the Jcode server, and use the default port `7643` unless you configured another port. The host must be reachable from the phone. Prefer a Tailscale/MagicDNS or HTTPS endpoint when outside a trusted LAN.
2. Run `jcode pair` on that host and enter its address and short-lived pairing code in the app.
3. The app posts `{ code, device_id, device_name }` to `POST /pair`. The gateway returns a device token, server name, and version.
4. The token is persisted only with `expo-secure-store`. Use **Forget device** to remove it locally. Revoke the device from the gateway if the phone is lost.

## Transport behavior

- The app opens `/ws` after pairing and reconnects with capped exponential backoff (1s to 30s, jittered). If a session is open, reconnect also re-subscribes to that session and reloads its history.
- React Native WebSocket in Expo Go cannot set an `Authorization` header. The current gateway supports the compatibility form `wss://host/ws?token=...`; the app uses it and never logs the complete URL or token.
- Use `https`/`wss` on untrusted networks. A plain `http`/`ws` LAN gateway exposes pairing codes and WebSocket query tokens to the local network.
- Every client request receives a numeric `id`; correlated replies resolve only the matching pending request. Pending requests fail on close or after 15 seconds.

## Wire compatibility

The UI tolerates legacy gateway frames and the newer API naming surface:

- Requests: `list_sessions`, `subscribe` with `target_session_id` and an absolute `working_dir`, `get_history`, and `message`.
- Events: `session_list`, `session_id`/`session`, `history`, `text_delta`, `text_replace`, `tool_start`, `tool_exec`, `tool_done`, `message_end`, `error`, `done`, and `swarm_status`.
- Resuming an existing session returns `history` as part of the correlated `subscribe` response. The app does not issue a duplicate `get_history` in that case.
- `swarm_status` is a structured payload from the gateway. It remains forward-compatible even when the mobile UI has no dedicated swarm dashboard.

Unknown frames are ignored so server-side additions do not crash the client.

## Source versus shipped status

The Rust gateway routes and protocol are wired into the Jcode server in
`crates/jcode-base/src/gateway.rs` and `crates/jcode-app-core/src/server.rs`.
The Expo companion in `apps/mobile` is still a source-only adapter: it has no
release binary, app-store artifact, or owner-defined lint/build script. The
mobile baseline gate is therefore the reproducible dependency install plus
`npm test`, `npm run typecheck`, and Expo config validation. This gate does not
claim that a phone or simulator was activated.
