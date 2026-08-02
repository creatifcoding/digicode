# Revisions: Artifact Library PSR Candidate Instance 001

## 2026-07-30 - Measured MetaTool admission and presentation

**Change:** Recorded the first live admission through `mt.artifacts.admitBundle` and the subsequent restart/presentation verification.

**Measured:** Jcode revision `9e97fb980` returned artifact `807c289b-a710-4fd7-b297-ca1579608919`, revision `ab824a93-3995-48a3-8611-66e8e85faf6e`, annotation `abc86d6e-4634-4b15-abef-d9531387e037`, and proposed `psr` candidate `ff4c09d3-c862-4dbe-82c1-a8cd85f7170b`.

**Measured:** A fresh artifact-server process reopened the persistent store, rendered revision 1, listed it in the catalog, and emitted the official `datastar-patch-elements` SSE event.

**Decision:** MetaTool owns artifact operations. The artifact store owns persistence. Datastar owns presentation. Candidate `psr` remains proposed pending repeated use and ratification evidence.

## 2026-07-30 - MetaTool admission rewrite

**Change:** Rewrote the PSR packet so MetaTool is the owning API and admission is the operational concept.

**Observed:** The packet now includes a machine-readable `mt-admission.request.json`, updated manifests, source PSR, rendered HTML, and compatibility script language. Candidate Type 001 is explicitly the first candidate instance, while the reusable template key is `psr`.

**Removed:** Invented ingestion doctrine, downstream persistence vocabulary, placeholder Docker image claims, and wording that let external runtime terms define the core boundary.

**Blocked:** Admission is not claimed complete. A receipt or observed `mt evaluate` result is still required.

**Compatibility note:** `scripts/seed_psr_artifact.sh` keeps its historical filename to avoid breaking callers. Its behavior and prose now describe admission, not seeding.

## 2026-07-30 - Initial packet

**Change:** Created the initial PSR candidate packet with source, manifests, revision notes, and standalone HTML rendering.

**Superseded claim:** The initial packet proposed a separate future ingestion path. That claim is intentionally replaced by the MetaTool-owned admission request.
