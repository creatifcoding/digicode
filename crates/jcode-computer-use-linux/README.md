# Jcode Linux computer use

This crate is Jcode's source-owned Linux desktop automation backend. The Jcode
application links it directly and exposes it as `linux_computer_use`; no MCP
sidecar, downloaded executable, or external checkout is required at runtime.

## Headless compositor runner

`jcode-computer-use-headless` runs one command inside an isolated Sway
compositor backed by wlroots' headless output:

```sh
jcode-computer-use-headless -- jcode
```

The runner requires `sway`, `swaymsg`, and `grim` on `PATH`. Applications needed
by the child command must also be installed. It creates a private
`XDG_RUNTIME_DIR`, removes host compositor selectors, configures the owned
backend to use `grim`, and tears down only the Sway process it started. Runtime
files are removed after the child exits.

For an ephemeral development environment on NixOS:

```sh
nix shell nixpkgs#sway nixpkgs#grim nixpkgs#foot --command \
  jcode-computer-use-headless -- jcode
```

The current headless contract covers compositor window discovery and screenshot
capture. Input backends remain capability-driven: portal input may be absent in
an isolated compositor, while host-level uinput or ydotool permissions are not
silently granted by this runner.

## Screenshot backend override

Set `JCODE_COMPUTER_USE_SCREENSHOT_BACKEND` to one of `gnome-shell`,
`gnome-extension`, `portal`, `grim`, or `gnome-screenshot`. The imported legacy
`CODEX_COMPUTER_USE_SCREENSHOT_BACKEND` name remains accepted for compatibility.

See [UPSTREAM.md](UPSTREAM.md) and [LICENSE-MIT](LICENSE-MIT) for provenance.
