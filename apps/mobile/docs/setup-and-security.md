# Setup and security

## Pair a device

1. Start a Jcode gateway on a host reachable from the phone. Prefer a Tailscale/MagicDNS or HTTPS endpoint when outside a trusted LAN.
2. Run `jcode pair` on that host and enter its address and short-lived pairing code in the app.
3. The app posts `{ code, device_id, device_name }` to `POST /pair`. The gateway returns a device token, server name, and version.
4. The token is persisted only with `expo-secure-store`. Use **Forget device** to remove it locally. Revoke the device from the gateway if the phone is lost.

## Transport behavior

- The app opens `/ws` after pairing and reconnects with capped exponential backoff (1s to 30s, jittered).
- React Native WebSocket in Expo Go cannot set an `Authorization` header. The current gateway supports the compatibility form `wss://host/ws?token=...`; the app uses it and never logs the complete URL or token.
- Use `https`/`wss` on untrusted networks. A plain `http`/`ws` LAN gateway exposes pairing codes and WebSocket query tokens to the local network.
- Every client request receives a numeric `id`; correlated replies resolve only the matching pending request. Pending requests fail on close or after 15 seconds.

## Wire compatibility

The UI tolerates legacy gateway frames and the newer API naming surface:

- Requests: `list_sessions`, `subscribe` with `target_session_id`, `get_history`, and `message`.
- Events: `session_list`, `session_id`/`session`, `history`, `text_delta`, `text_replace`, `tool_start`, `tool_exec`, `tool_done`, `message_end`, `error`, `done`, and `swarm_status`.

Unknown frames are ignored so server-side additions do not crash the client.
