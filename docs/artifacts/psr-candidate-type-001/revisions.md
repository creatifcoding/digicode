# Revisions: Artifact Library PSR Candidate Type 001

## 2026-07-30.initial-dogfood

**Tier:** OBSERVED  
**Change:** Created the initial Candidate Type 001 packet with PSR source, manifests, revision annotation, and standalone HTML rendering.

### Annotation

This revision intentionally separates source artifact material from future server/store implementation. The artifact packet is real repository material. The ingestion interface is proposed. Live ingestion is blocked until an artifact-store CLI or equivalent adapter exists.

### Changelog

- Added PSR source for Candidate Type 001.
- Added artifact manifest JSON.
- Added candidate manifest JSON.
- Added revision annotation/changelog.
- Added standalone Rojo-brutalist rendered HTML.
- Documented local and Docker-minded dry-run operation.
- Marked live ingestion as blocked rather than pretending infrastructure exists. A tiny miracle of restraint.
