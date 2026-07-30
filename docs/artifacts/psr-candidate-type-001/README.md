# PSR: Artifact Library Candidate Type 001

**Document status:** Candidate dogfood material  
**Candidate type:** `001`  
**Artifact family:** Artifact Library / Product-Semantic Record  
**Audience:** Digimasons operators, artifact-server implementers, reviewers, and future store adapters  
**Last revised:** 2026-07-30

## Claim ledger

| Tier | Claim |
| --- | --- |
| **OBSERVED** | This packet is authored as repository material under `docs/artifacts/psr-candidate-type-001/`, `examples/artifact-library/psr-candidate-type-001/`, and `scripts/seed_psr_artifact.sh`. |
| **OBSERVED** | The packet includes Markdown, artifact manifest JSON, candidate manifest JSON, revision annotation/changelog, standalone rendered HTML, and a seed script. |
| **PROPOSED** | Candidate Type 001 should be the first dogfood shape for a PSR artifact: a portable, reviewable, renderable bundle that can enter a planned artifact server/store. |
| **PROPOSED** | The planned artifact server/store CLI should support dry-run seeding, local filesystem-backed stores, and Docker-mounted stores before live publication. |
| **BLOCKED** | No Rust artifact-server/store crates were modified here, so executable CLI behavior is not implemented by this packet. |
| **BLOCKED** | Live server ingestion is blocked until the planned CLI contract exists or an adapter implements the interface documented below. |

## Summary

Candidate Type 001 defines the smallest complete dogfood packet for the Artifact Library: one PSR Markdown source, two manifests, a revision annotation, a rendered HTML artifact, and a shell script that seeds the packet through a future artifact server/store CLI.

Prime, the noun is the artifact, not the renderer, not the store, and certainly not the vibes wearing a nametag. Type 001 proves the artifact can stand alone before we let infrastructure make promises on its behalf.

## Goals

1. **Portable review:** A reviewer can inspect the PSR without running services.
2. **Renderable proof:** A standalone HTML rendering demonstrates the intended design language and artifact affordances.
3. **Manifested identity:** Machine-readable manifests name artifact identity, candidate type, files, provenance, and blocked/proposed claims.
4. **Seed-ready operation:** A script documents the intended server/store ingestion path without pretending the CLI exists today.
5. **Claim hygiene:** Every material assertion is marked observed, proposed, or blocked.

## Non-goals

- Implementing the Rust artifact server/store.
- Defining a stable public API for all artifact kinds.
- Publishing to a live artifact registry.
- Treating the Rojo-brutalist HTML as the final visual system.

## Candidate Type 001 shape

```text
psr-candidate-type-001/
├── psr.md
├── artifact.manifest.json
├── candidate.manifest.json
├── revisions.md
└── rendered.html
```

### Required files

| File | Role | Required for Type 001 |
| --- | --- | --- |
| `psr.md` | Human-readable Product-Semantic Record source | Yes |
| `artifact.manifest.json` | Artifact identity, rendering contract, provenance, and file inventory | Yes |
| `candidate.manifest.json` | Candidate dogfood metadata, acceptance checks, and claim tiers | Yes |
| `revisions.md` | Revision annotation and changelog | Yes |
| `rendered.html` | Self-contained visual rendering | Yes |

## PSR content contract

A Type 001 PSR should include artifact intent, boundary and owner, source inputs, observed facts, proposed semantics, blocked claims, acceptance checks, local operation notes, Docker-minded operation notes, and follow-up work.

## Store and server interface proposal

The seed script assumes a future CLI with this shape:

```bash
artifact-store seed \
  --artifact-manifest examples/artifact-library/psr-candidate-type-001/artifact.manifest.json \
  --candidate-manifest examples/artifact-library/psr-candidate-type-001/candidate.manifest.json \
  --source examples/artifact-library/psr-candidate-type-001/psr.md \
  --rendered examples/artifact-library/psr-candidate-type-001/rendered.html \
  --revisions examples/artifact-library/psr-candidate-type-001/revisions.md \
  --store ./var/artifact-store \
  --mode local
```

### Proposed CLI behavior

| Behavior | Tier | Notes |
| --- | --- | --- |
| Validate JSON manifests before ingestion | **PROPOSED** | The script performs local `python -m json.tool` validation now. |
| Copy or register all referenced files atomically | **PROPOSED** | The future store should reject partial bundles. |
| Preserve revision annotations | **PROPOSED** | Revisions are part of the artifact, not incidental prose. |
| Emit a machine-readable receipt | **PROPOSED** | Receipt should include artifact id, candidate type, store path, and content hashes. |
| Execute live server mutation | **BLOCKED** | Requires implemented CLI and operator-approved live path. |

## Local operation

```bash
./scripts/seed_psr_artifact.sh --dry-run
./scripts/seed_psr_artifact.sh --store ./var/artifact-store --dry-run
```

Observed behavior from this packet is limited to local file validation and command construction. If `artifact-store` is not installed, the script exits successfully in `--dry-run` mode after printing the proposed command. Without `--dry-run`, it fails closed when the CLI is absent.

## Docker-minded operation

```bash
docker run --rm \
  -v "$PWD:/workspace" \
  -w /workspace \
  ghcr.io/digimasons/artifact-store:proposed \
  ./scripts/seed_psr_artifact.sh \
    --store /workspace/var/artifact-store \
    --mode local \
    --dry-run
```

**PROPOSED:** The image name above is a placeholder for the future artifact-store runtime image.  
**BLOCKED:** This repository change does not build or publish that image.  
**OBSERVED:** The command documents the expected mount shape for a local store and workspace-relative artifact packet.

## Acceptance checks

- [x] Markdown PSR source exists.
- [x] Artifact manifest exists and parses as JSON.
- [x] Candidate manifest exists and parses as JSON.
- [x] Revision annotation/changelog exists.
- [x] Standalone rendered HTML exists.
- [x] Seed script validates local files before constructing the proposed CLI command.
- [x] Claims are tiered as observed, proposed, or blocked.
- [ ] Live artifact server/store ingestion succeeds. **BLOCKED:** CLI not implemented here.

## Follow-up work

1. Implement or bind the `artifact-store seed` CLI.
2. Add content hash verification to manifests once the canonical hashing policy is chosen.
3. Add an artifact-server fixture test that ingests this candidate and emits a receipt.
4. Promote or reject Rojo-brutalist styling after visual review.
