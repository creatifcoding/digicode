# Jcode Mobile

An Expo Go-compatible React Native companion for a Jcode gateway. It pairs a device using a short-lived code, stores the resulting device token in the platform secure store, and displays session transcripts over the gateway WebSocket.

## Development

```bash
cd apps/mobile
npm install
npm start
npm test
npm run typecheck
```

Start the gateway on a reachable LAN or Tailscale address, run `jcode pair` there, then use the generated host, port, and pairing code in the mobile app.

See [docs/setup-and-security.md](docs/setup-and-security.md) for connection and token handling details.
See [docs/baseline-validation.md](docs/baseline-validation.md) for the source-live/shipped disposition and validation evidence.
