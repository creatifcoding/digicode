#!/usr/bin/env bash
# Install the bounded Rust target collector as a per-user system service.
# The service applies to every configured Rust workspace, not only this checkout.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
libexec_dir="${XDG_DATA_HOME:-$HOME/.local/share}/jcode/libexec"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/jcode"
systemd_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
collector="$libexec_dir/rust-cache-gc"
roots_file="$config_dir/rust-cache-roots"

mkdir -p "$libexec_dir" "$config_dir" "$systemd_dir"
install -m 0755 "$repo_root/scripts/rust_cache_gc.py" "$collector"

touch "$roots_file"
add_root() {
  local path="$1"
  [[ -d "$path" ]] || return 0
  grep -Fxq "$path" "$roots_file" 2>/dev/null || printf '%s\n' "$path" >>"$roots_file"
}
add_root "$HOME/.jcode"
add_root "$HOME/projects"
add_root "$HOME/src"
add_root "$HOME/code"
add_root "$HOME/getbyzenbook/projects"

cat >"$systemd_dir/jcode-rust-cache-gc.service" <<EOF
[Unit]
Description=Bound regenerable Rust/Cargo target storage
Documentation=https://github.com/creatifcoding/digicode

[Service]
Type=oneshot
ExecStart=$collector --apply --roots-file $roots_file
Nice=19
IOSchedulingClass=idle
IOSchedulingPriority=7
PrivateTmp=true
NoNewPrivileges=true
EOF

cat >"$systemd_dir/jcode-rust-cache-gc.timer" <<'EOF'
[Unit]
Description=Periodically bound regenerable Rust/Cargo target storage

[Timer]
OnBootSec=10min
OnUnitActiveSec=2h
RandomizedDelaySec=10min
Persistent=true
Unit=jcode-rust-cache-gc.service

[Install]
WantedBy=timers.target
EOF

if command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then
  systemctl --user daemon-reload
  systemctl --user enable --now jcode-rust-cache-gc.timer
  printf 'installed and enabled user timer: jcode-rust-cache-gc.timer\n'
else
  printf 'installed collector and units; user systemd is unavailable, so enable later with:\n' >&2
  printf '  systemctl --user daemon-reload && systemctl --user enable --now jcode-rust-cache-gc.timer\n' >&2
fi

printf 'collector: %s\nroots: %s\n' "$collector" "$roots_file"
