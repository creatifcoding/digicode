# PSR: Artifact Library Candidate Instance 001

**Status:** request-ready, not admitted  
**Owning API:** MetaTool `mt evaluate`  
**Reusable template key:** `psr`  
**Candidate Type 001 means:** first candidate instance  
**Compatibility script:** `scripts/seed_psr_artifact.sh`

## Claim ledger

| Tier | Claim |
| --- | --- |
| **OBSERVED** | This packet is authored as repository material under `docs/artifacts/psr-candidate-type-001/`, mirrored under `examples/artifact-library/psr-candidate-type-001/`, and referenced by `scripts/seed_psr_artifact.sh`. |
| **OBSERVED** | The packet includes Markdown, artifact manifest JSON, candidate manifest JSON, revision notes, rendered HTML, and a machine-readable `mt-admission.request.json`. |
| **OBSERVED** | The operational noun is admission. The script filename still says `seed` only for compatibility. |
| **PROPOSED** | MetaTool should admit this PSR packet into the `artifact-library` namespace using the checked-in `mt evaluate` payload. |
| **BLOCKED** | No admission receipt is present, so this packet must not claim live execution. |

## What this proves

Candidate Instance 001 defines the smallest PSR dogfood packet for the Artifact Library. It proves that a PSR can stand alone as source, manifest, rendered review artifact, revision record, and exact MetaTool request.

It does not prove that a server, downstream persistence adapter, Docker image, or external runtime CLI exists. That would be a dependency misdemeanor, Prime, and the court is in session.

## Files

| File | Role |
| --- | --- |
| `psr.md` | Durable source prose for the PSR candidate instance. |
| `artifact.manifest.json` | Artifact identity, claim tiers, owning API, and file inventory. |
| `candidate.manifest.json` | Candidate-instance meaning, checks, and admission state. |
| `mt-admission.request.json` | Exact machine-readable MetaTool request payload. |
| `revisions.md` | Packet changelog and claim corrections. |
| `rendered.html` | Standalone offline review rendering. |

## Admission contract

MetaTool is the owning API. The request file has this top-level shape:

```json
{
  "tool": "mt",
  "action": "evaluate",
  "profile": "pure",
  "tasker_mode": "off",
  "code": "...",
  "inputs": { "packet": "..." }
}
```

The compatibility script supports:

```bash
./scripts/seed_psr_artifact.sh --dry-run
./scripts/seed_psr_artifact.sh --write-request
./scripts/seed_psr_artifact.sh --admit
```

If no supported non-interactive `mt` CLI path exists, `--admit` writes the request file and exits blocked. It does not pretend admission happened.

## Acceptance checklist

- [x] Candidate Type 001 is defined as first candidate instance.
- [x] Reusable template key is `psr`.
- [x] MetaTool is the only owning API named for admission.
- [x] Invented separate-ingestion doctrine is absent.
- [x] Blocked and observed claims are separated.
- [x] Request file exists for machine handoff.
- [ ] Admission receipt/result is recorded after actual `mt evaluate` execution.
