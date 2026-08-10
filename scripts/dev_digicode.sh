#!/usr/bin/env bash
set -euo pipefail

# Build the primary executable without changing the shared daemon, channel
# symlinks, launcher, or any other activation state. Use the existing cargo
# wrapper so the repository's memory and remote-build policy is preserved.
repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
profile="${DIGICODE_PROFILE:-selfdev}"

exec "$repo_root/scripts/dev_cargo.sh" build \
  --profile "$profile" \
  -p jcode \
  --bin digicode \
  "$@"
