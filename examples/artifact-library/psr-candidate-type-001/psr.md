# Artifact Library PSR: Candidate Type 001

**PSR id:** `psr.artifact-library.candidate-type-001`  
**Candidate type:** `001`  
**Status:** Dogfood candidate  
**Owner boundary:** Artifact Library source packet  
**Revision:** `2026-07-30.initial-dogfood`

## Claim ledger

| Tier | Claim |
| --- | --- |
| **OBSERVED** | This PSR packet exists in `examples/artifact-library/psr-candidate-type-001/`. |
| **OBSERVED** | The packet includes `psr.md`, `artifact.manifest.json`, `candidate.manifest.json`, `revisions.md`, and `rendered.html`. |
| **PROPOSED** | Candidate Type 001 is the minimum viable PSR bundle for Artifact Library dogfooding. |
| **PROPOSED** | A future artifact server/store CLI should ingest this bundle with one command and return a durable receipt. |
| **BLOCKED** | Server ingestion and receipt generation are not observed because the planned CLI is outside this change. |

## Intent

Create a small but complete PSR artifact that can be read by humans, validated by scripts, rendered in a browser, and later seeded into the Artifact Library store.

## Boundary

The boundary is the packet. The packet owns source prose, manifests, revision annotation, and rendered preview. The future server/store owns ingestion, persistence, receipts, indexing, and retrieval.

## Source inputs

- Operator request on 2026-07-30 to author Candidate Type 001 dogfood material.
- Repository path ownership constraint: only `docs/artifacts/**`, `examples/artifact-library/**`, and `scripts/seed_psr_artifact.sh`.
- Project instruction to avoid unsupported absolutes and mark observed/proposed/blocked claims.

## Proposed artifact semantics

A PSR artifact is a reviewable semantic record with stable identity, explicit claim tiers, file inventory, rendering contract, revision annotation, local and Docker-minded operation notes, and a future ingestion path.

## Local operation

Run:

```bash
./scripts/seed_psr_artifact.sh --dry-run
```

Expected local effect:

- JSON manifests are parsed.
- Required files are checked.
- The proposed `artifact-store seed` command is printed.
- No live mutation occurs.

## Docker-minded operation

Mount the repository at `/workspace`, use `/workspace/var/artifact-store` for local persistence, and run the same script with `--dry-run` until the CLI exists.

```bash
docker run --rm -v "$PWD:/workspace" -w /workspace artifact-store:proposed \
  ./scripts/seed_psr_artifact.sh --store /workspace/var/artifact-store --dry-run
```

**BLOCKED:** `artifact-store:proposed` names a future runtime image, not an observed published image.

## Acceptance

This candidate is acceptable as dogfood material if a reviewer can read the PSR source, parse both JSON manifests, open the standalone HTML without network access, inspect a revision annotation, run the seed script in dry-run mode, and identify which claims are observed, proposed, and blocked.

## Follow-up

The next implementation should make `artifact-store seed` real, then run this candidate through the live local store path and record the emitted receipt.
