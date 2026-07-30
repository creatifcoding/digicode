# Artifact Library PSR: Candidate Instance 001

**Reusable template key:** `psr`  
**Candidate instance:** `001`  
**Owning API:** MetaTool `mt evaluate`  
**Operational concept:** admission  
**Compatibility script:** `scripts/seed_psr_artifact.sh`

## Claim ledger

| Tier | Claim |
| --- | --- |
| **OBSERVED** | This repository packet contains PSR Markdown, manifests, revision notes, standalone HTML, a compatibility script reference, and a machine-readable MetaTool admission request. |
| **OBSERVED** | Candidate Type 001 means the first candidate instance. The reusable template key is `psr`. |
| **OBSERVED** | The script can validate packet files and JSON manifests locally. |
| **PROPOSED** | MetaTool is the owning API for Artifact Library admission through an `mt evaluate` payload against the `artifact-library` namespace. |
| **BLOCKED** | Admission is not claimed complete until a supported non-interactive `mt` CLI path executes the payload or an operator runs the same payload through the Jcode `mt` tool and records the result. |

## Intent

Create a small, reviewable PSR artifact packet that can be read by humans, parsed by tools, rendered offline, and admitted through MetaTool without inventing a separate store-ingestion command.

Prime, the noun is `admission`. `seed` is a filename scar kept for compatibility, not a doctrine. Tiny taxonomy, large consequences.

## Boundary and owner

MetaTool owns the API boundary. The packet does not define a downstream persistence adapter, server CLI, Docker image, or external runtime vocabulary. Those may exist later, but this PSR only asserts the MetaTool admission request shape and the source files it carries.

## Source inputs

- Operator request on 2026-07-30 to rewrite the PSR packet so MetaTool is the owning API.
- Repository path ownership constraint: only `docs/artifacts/**`, `examples/artifact-library/**`, and `scripts/seed_psr_artifact.sh`.
- Project instruction to separate observed, proposed, and blocked claims.

## Packet shape

A PSR candidate instance includes:

1. `psr.md` as durable source prose.
2. `artifact.manifest.json` as artifact identity and owning API metadata.
3. `candidate.manifest.json` as candidate-instance checks and admission state.
4. `mt-admission.request.json` as the exact machine-readable MetaTool request payload.
5. `revisions.md` as the packet changelog.
6. `rendered.html` as an offline review artifact.
7. `scripts/seed_psr_artifact.sh` as a compatibility entrypoint whose operation is admission.

## Admission operation

The compatibility script validates the packet. If a supported non-interactive MetaTool CLI command is configured, it may invoke it. If not, it writes the exact `mt evaluate` request JSON and exits without claiming execution.

```bash
./scripts/seed_psr_artifact.sh --dry-run
./scripts/seed_psr_artifact.sh --write-request
./scripts/seed_psr_artifact.sh --admit
```

Expected honest outcomes:

- **Dry run:** validates files and prints the request location. No mutation attempted.
- **Write request:** validates files and writes `mt-admission.request.json`.
- **Admit:** executes only when a supported `mt` CLI command is configured. Otherwise it writes the request and reports the admission as blocked.

## Acceptance

This candidate is acceptable as dogfood material if a reviewer can read the PSR source, parse both manifests and the request JSON, open the standalone HTML offline, inspect revision notes, run the compatibility script without false execution claims, and verify the distinction between candidate instance `001` and template key `psr`.

## Follow-up

Record an observed MetaTool result or receipt after admission actually runs. Do not promote this packet from request-ready to admitted on prose alone.
