#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKET_DIR="$ROOT_DIR/examples/artifact-library/psr-candidate-type-001"
REQUEST_FILE=""
DRY_RUN=0
WRITE_REQUEST=0
ADMIT=0
MT_CLI="${JCODE_MT_CLI:-}"

usage() {
  cat <<'USAGE'
Usage: scripts/seed_psr_artifact.sh [options]

Compatibility entrypoint for the Artifact Library PSR Candidate Instance 001
admission request. The filename is retained for older callers; the operation is
admission through MetaTool, not seeding through a separate persistence adapter.

Options:
  --packet DIR         Packet directory. Default: examples/artifact-library/psr-candidate-type-001
  --request-file FILE  Request JSON path. Default: PACKET_DIR/mt-admission.request.json
  --mt-cli COMMAND     Supported non-interactive mt evaluator command. Also JCODE_MT_CLI.
  --dry-run            Validate files and report the request path without mutation
  --write-request      Validate files and write the mt evaluate payload
  --admit              Attempt admission only if a supported mt CLI command exists
  -h, --help           Show this help

Claim tiers:
  OBSERVED: This script validates local files and JSON manifests.
  OBSERVED: This script can write the exact machine-readable mt evaluate request.
  BLOCKED: Admission is not claimed unless the mt CLI command actually succeeds.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --packet) PACKET_DIR="$2"; shift 2 ;;
    --request-file) REQUEST_FILE="$2"; shift 2 ;;
    --mt-cli) MT_CLI="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --write-request) WRITE_REQUEST=1; shift ;;
    --admit) ADMIT=1; shift ;;
    -h|--help) usage; exit 0 ;;
    --store|--mode|--cli)
      echo "Unsupported legacy option for MetaTool admission: $1" >&2
      echo "Use --request-file, --mt-cli, --dry-run, --write-request, or --admit." >&2
      exit 2 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ -z "$REQUEST_FILE" ]]; then
  REQUEST_FILE="$PACKET_DIR/mt-admission.request.json"
fi

required_files=(psr.md artifact.manifest.json candidate.manifest.json revisions.md rendered.html)
for file in "${required_files[@]}"; do
  if [[ ! -f "$PACKET_DIR/$file" ]]; then
    echo "Missing required packet file: $PACKET_DIR/$file" >&2
    exit 1
  fi
done

python -m json.tool "$PACKET_DIR/artifact.manifest.json" > /dev/null
python -m json.tool "$PACKET_DIR/candidate.manifest.json" > /dev/null

write_request() {
  mkdir -p "$(dirname "$REQUEST_FILE")"
  python - "$REQUEST_FILE" <<'PY'
import json, sys
request_file = sys.argv[1]
files = ["psr.md", "artifact.manifest.json", "candidate.manifest.json", "revisions.md", "rendered.html", "mt-admission.request.json"]
payload = {
  "tool": "mt",
  "action": "evaluate",
  "profile": "pure",
  "tasker_mode": "off",
  "code": "const packet = inputs.packet;\nawait mt.put('artifact-library', packet.artifact.key, {\n  _meta: { summary: packet.artifact.summary },\n  type: packet.artifact.type,\n  templateKey: packet.artifact.templateKey,\n  candidateInstance: packet.artifact.candidateInstance,\n  files: packet.files,\n  claims: packet.claims\n});\nreturn { admitted: true, artifactKey: packet.artifact.key, templateKey: packet.artifact.templateKey, candidateInstance: packet.artifact.candidateInstance };",
  "inputs": {
    "packet": {
      "artifact": {
        "key": "psr-candidate-type-001",
        "templateKey": "psr",
        "candidateInstance": "001",
        "type": "PSR candidate instance",
        "summary": "First PSR candidate instance admitted through MetaTool-owned artifact-library API request."
      },
      "files": files,
      "claims": {
        "observed": [
          "Repository packet files exist under the owned docs/examples paths.",
          "The admission request payload is represented as JSON and can be handed to mt evaluate."
        ],
        "blocked": [
          "This request file is not proof of execution. A receipt is required before claiming admission occurred."
        ]
      }
    }
  }
}
with open(request_file, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2)
    handle.write("\n")
PY
}

printf 'OBSERVED: packet files exist and manifests parse as JSON.\n'

if [[ "$DRY_RUN" -eq 1 ]]; then
  printf 'OBSERVED: dry-run requested; admission was not attempted.\n'
  printf 'REQUEST_FILE=%s\n' "$REQUEST_FILE"
  exit 0
fi

if [[ "$WRITE_REQUEST" -eq 1 || "$ADMIT" -eq 1 ]]; then
  write_request
  printf 'OBSERVED: wrote MetaTool admission request: %s\n' "$REQUEST_FILE"
fi

if [[ "$ADMIT" -ne 1 ]]; then
  exit 0
fi

if [[ -z "$MT_CLI" ]]; then
  printf 'BLOCKED: no supported non-interactive mt CLI command configured. Set JCODE_MT_CLI or --mt-cli.\n' >&2
  printf 'BLOCKED_REQUEST_FILE=%s\n' "$REQUEST_FILE" >&2
  exit 1
fi

first_word="${MT_CLI%% *}"
if ! command -v "$first_word" > /dev/null 2>&1; then
  printf 'BLOCKED: configured mt CLI command is not available: %s\n' "$MT_CLI" >&2
  printf 'BLOCKED_REQUEST_FILE=%s\n' "$REQUEST_FILE" >&2
  exit 1
fi

# shellcheck disable=SC2086
$MT_CLI "$REQUEST_FILE"
printf 'OBSERVED: MetaTool admission command completed successfully.\n'
