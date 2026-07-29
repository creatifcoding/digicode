# MetaTool codemode engine

First-party fork of `@tmnl/metatool` (origin: `~/.pi/agent/packages/metatool`,
forked 2026-07-29 per the ratified codemode architecture). The engine runs
INSIDE the AgentOS guest: Effect v4 store graph over guest `node:sqlite` at
`/data/store.db`, backed write-through by a host-side chunked-local mount under
`~/.jcode/metatool/stores/<workspace>/`.

Build:

```sh
npm install
npx tsc
npx esbuild src/boot.ts --bundle --format=esm --platform=node \
  --external:node:* --outfile=dist/guest-engine.mjs
```

Then copy `dist/guest-engine.mjs` into
`crates/jcode-metatool-runtime/assets/guest-engine.mjs` (embedded at compile
time and materialized into the runtime directory with digest checks).

Changes from origin:

- `src/boot.ts`: guest bundle entry (engine + adapters + clone-safe sanitizer).
- `src/clone-safe.ts`: ported from the Pi extension host (dependency-free).
- `package.json`: renamed `@jcode/metatool-engine`; effect pinned as a direct
  dependency; standalone tsconfig.
