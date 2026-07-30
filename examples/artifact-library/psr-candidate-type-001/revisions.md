# Revisions: Artifact Library PSR Candidate Instance 001

## 2026-07-30 - MetaTool admission rewrite

**Change:** Rewrote the PSR packet so MetaTool is the owning API and admission is the operational concept.

**Observed:** The packet now includes a machine-readable `mt-admission.request.json`, updated manifests, source PSR, rendered HTML, and compatibility script language. Candidate Type 001 is explicitly the first candidate instance, while the reusable template key is `psr`.

**Removed:** Invented ingestion doctrine, downstream persistence vocabulary, placeholder Docker image claims, and wording that let external runtime terms define the core boundary.

**Blocked:** Admission is not claimed complete. A receipt or observed `mt evaluate` result is still required.

**Compatibility note:** `scripts/seed_psr_artifact.sh` keeps its historical filename to avoid breaking callers. Its behavior and prose now describe admission, not seeding.

## 2026-07-30 - Initial packet

**Change:** Created the initial PSR candidate packet with source, manifests, revision notes, and standalone HTML rendering.

**Superseded claim:** The initial packet proposed a separate future ingestion path. That claim is intentionally replaced by the MetaTool-owned admission request.
