# Artifact Library PSR: Candidate Instance 001

**Reusable template key:** `psr`  
**Candidate instance:** `001`  
**Owning API:** MetaTool `mt.artifacts`
**Operational concept:** admission  
**Compatibility script:** `scripts/seed_psr_artifact.sh`

## Claim ledger

| Tier | Claim |
| --- | --- |
| **OBSERVED** | This repository packet contains PSR Markdown, manifests, revision notes, standalone HTML, a compatibility script reference, and a machine-readable MetaTool admission request. |
| **OBSERVED** | Candidate Type 001 means the first candidate instance. The reusable template key is `psr`. |
| **OBSERVED** | The script can validate packet files and JSON manifests locally. |
| **MEASURED** | Jcode revision `9e97fb980` admitted this packet through `mt.artifacts.admitBundle`, returning artifact `807c289b-a710-4fd7-b297-ca1579608919`, revision `ab824a93-3995-48a3-8611-66e8e85faf6e`, annotation `abc86d6e-4634-4b15-abef-d9531387e037`, and proposed candidate `ff4c09d3-c862-4dbe-82c1-a8cd85f7170b`. |
| **MEASURED** | A fresh artifact-server process reopened `JCODE_HOME/artifacts`, listed this artifact, rendered revision 1, and emitted `datastar-patch-elements` for the catalog. |
| **OBSERVED** | MetaTool is the owning API. `mt.artifacts` projects bounded catalog reads and brokers admission effects to the host artifact store without granting guest filesystem or database authority. |

## Intent

Create a small, reviewable PSR artifact packet that can be read by humans, parsed by tools, rendered offline, and admitted through MetaTool without inventing a separate store-ingestion command.

Prime, the noun is `admission`. `seed` is a filename scar kept for compatibility, not a doctrine. Tiny taxonomy, large consequences.

## Boundary and owner

MetaTool owns the API boundary through the host-brokered `mt.artifacts` capability. The guest may inspect a bounded catalog and emit one admission effect, but never receives the artifact database or host filesystem. The artifact store owns persistence, while the Datastar server owns presentation. Downstream adapters do not define the domain.

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

The compatibility script validates the packet and writes the exact `mt evaluate` request for `mt.artifacts.admitBundle`. Admission has now been observed through Jcode itself. A future non-interactive adapter may invoke the same request, but it does not own the operation.

```bash
./scripts/seed_psr_artifact.sh --dry-run
./scripts/seed_psr_artifact.sh --write-request
./scripts/seed_psr_artifact.sh --admit
```

Expected honest outcomes:

- **Dry run:** validates files and prints the request location. No mutation attempted.
- **Write request:** validates files and writes `mt-admission.request.json`.
- **Admit:** executes only when a supported adapter is configured. The first live admission was performed through Jcode's native `mt` tool and returned a durable receipt.

## Acceptance

This candidate is acceptable as dogfood material if a reviewer can read the PSR source, parse both manifests and the request JSON, open the standalone HTML offline, inspect revision notes, run the compatibility script without false execution claims, and verify the distinction between candidate instance `001` and template key `psr`.

## Measured dogfood result

On 2026-07-30, Jcode revision `9e97fb980` executed the generated request against `mt.artifacts`. The host reconciler admitted immutable revision 1, attached the revision annotation, and registered template candidate `psr` in `proposed` status. A separate artifact-server process then reopened the durable store and rendered both the catalog and revision. The catalog SSE endpoint emitted the official `datastar-patch-elements` event.

## Follow-up

Admit this updated PSR as revision 2 so the record itself carries the evidence that closed revision 1's blocked claim. Candidate ratification remains deliberately pending; one successful specimen is evidence, not doctrine.
