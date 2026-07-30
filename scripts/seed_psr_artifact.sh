#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKET_DIR="$ROOT_DIR/examples/artifact-library/psr-candidate-type-001"
STORE_DIR="$ROOT_DIR/var/artifact-store"
MODE="local"
DRY_RUN=0
CLI_BIN="${ARTIFACT_STORE_CLI:-artifact-store}"

usage() {
  cat <<'USAGE'
Usage: scripts/seed_psr_artifact.sh [options]

Seeds the Artifact Library PSR Candidate Type 001 packet through the planned
artifact server/store CLI interface.

Options:
  --packet DIR       Packet directory. Default: examples/artifact-library/psr-candidate-type-001
  --store DIR        Local artifact store path. Default: ./var/artifact-store
  --mode MODE        Store mode passed to the planned CLI. Default: local
  --cli BIN          CLI binary name. Default: artifact-store or ARTIFACT_STORE_CLI
  --dry-run          Validate files and print the proposed command without executing it
  -h, --help         Show this help

Claim tiers:
  OBSERVED: This script validates local files and JSON manifests.
  PROPOSED: The artifact-store seed command shape below is the planned interface.
  BLOCKED: Non-dry-run execution requires a CLI implementation outside this change.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --packet)
      PACKET_DIR="$2"
      shift 2
      ;;
    --store)
      STORE_DIR="$2"
      shift 2
      ;;
    --mode)
      MODE="$2"
      shift 2
      ;;
    --cli)
      CLI_BIN="$2"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

required_files=(
  "psr.md"
  "artifact.manifest.json"
  "candidate.manifest.json"
  "revisions.md"
  "rendered.html"
)

for file in "${required_files[@]}"; do
  if [[ ! -f "$PACKET_DIR/$file" ]]; then
    echo "Missing required packet file: $PACKET_DIR/$file" >&2
    exit 1
  fi
done

python -m json.tool "$PACKET_DIR/artifact.manifest.json" >/dev/null
python -m json.tool "$PACKET_DIR/candidate.manifest.json" >/dev/null

cmd=(
  "$CLI_BIN" seed
  --artifact-manifest "$PACKET_DIR/artifact.manifest.json"
  --candidate-manifest "$PACKET_DIR/candidate.manifest.json"
  --source "$PACKET_DIR/psr.md"
  --rendered "$PACKET_DIR/rendered.html"
  --revisions "$PACKET_DIR/revisions.md"
  --store "$STORE_DIR"
  --mode "$MODE"
)

printf 'OBSERVED: packet files exist and manifests parse as JSON.\n'
printf 'PROPOSED: planned artifact-store seed command:\n'
printf '  %q' "${cmd[@]}"
printf '\n'

if [[ "$DRY_RUN" -eq 1 ]]; then
  printf 'OBSERVED: dry-run requested, no store mutation attempted.\n'
  exit 0
fi

if ! command -v "$CLI_BIN" >/dev/null 2>&1; then
  printf 'BLOCKED: %s is not installed or not on PATH. Re-run with --dry-run or provide --cli.\n' "$CLI_BIN" >&2
  exit 1
fi

"${cmd[@]}"
