# F658 Evidence Pack: Tasker Concurrency Artifacts and Adjudicated Promotion

Feature: `#658` "Tasker concurrency artifacts and adjudicated promotion" (`feat_da92d1f418d2`)
Repo: `/home/getbygenius/.jcode/source/jcode`, branch `master`
Assembled: 2026-08-01 by the coordinating session, from measured git/test/DB state.

## Delivery narrative

Ten tasks landed across three coordinated waves of parallel workers (2026-07-31), each
worker owning disjoint file paths and committing directly to master after per-crate
validation (fmt, tests, clippy `-D warnings`, `git diff --check`).

## Commit ledger

| Commit | Task | Delivery |
|---|---|---|
| `e43c36886` | #1877 | Concurrent canonical Tasker DB writes (WAL, busy handling) |
| `ef2d9dc49` | #1870 | Deterministic adjudication policy (pure, hard-gate vetoes, quorum tally, deterministic ties) |
| `b3069a0ac` | #1869 | `jcode-tasker-git`: candidate ref adapter (isolated refs, commit capture, CAS, IsolationProof, CleanupReport) |
| `3b19a289e` | #1868 | `ConcurrencyStore`: durable persistence with revision CAS |
| `669610345` | #1873 | `jcode-tasker-promotion`: recoverable promotion saga (durable intent phases, rollback, restart recovery) |
| `a19196782` | #1872 | `jcode-tasker-rounds`: deterministic round orchestration (ballots, replay checks, bounded status, escalation) |
| `b803212db` | #1871 | `jcode-tasker-orchestration`: triplicate candidate lanes (bounded policies, acceptance contracts, lifecycle, handoff) |
| `ce9eefd3e` | #1874 | `MutationPolicyGate`: canonical mutations gated behind candidate policy, wired into canonical write paths |
| `39b4d2757` | #1875 | End-to-end reconciliation suite (666-line integration harness) |
| `f46dc4aa0` | #1875 | Stabilization: synchronized noise writer, clippy-clean under `-D warnings` |
| `e2f99fd02` | — | Follow-on: codemode parity productization consuming the concurrency store |

Domain types for #1867 predate the wave in `jcode-tasker-types` (`src/concurrency.rs`, `src/adjudication.rs`).

## Crate surface (lines of Rust, src+tests)

- `jcode-tasker-types`: 2,142
- `jcode-tasker-git`: 984
- `jcode-tasker-pi`: 8,461 (includes `concurrency_store.rs`, `mutation_policy.rs`)
- `jcode-tasker-rounds`: 1,518
- `jcode-tasker-promotion`: 1,302
- `jcode-tasker-orchestration`: 2,096

## Triplicate end-to-end scenarios (tests/e2e_reconciliation.rs)

1. `happy_path_reconciles_three_lanes_and_cleans_loser_refs` — Ensemble(3, quorum 2): three isolated lanes commit distinct implementations, ballots select a winner, promotion saga finalizes the canonical ref, loser refs cleaned, store states consistent (`promoted` / `completed`).
2. `mutation_policy_rejects_canonical_conflict_and_admits_valid_speculation` — unscoped canonical mutation rejected naming the holder; valid candidate context admitted as speculative.
3. `promotion_recovery_rolls_back_prepared_and_finalizes_ref_updated_intents` — crash windows on both sides of the ref update recover correctly (rollback vs forward finalize).
4. `stale_base_rejection_preserves_the_foreign_canonical_ref` — CAS refuses to overwrite a foreign canonical mutation; canonical ref untouched.
5. `split_validator_ballots_escalate_without_promoting_any_candidate` — no quorum escalates; canonical untouched.

## Test evidence (measured 2026-08-01, HEAD `e2f99fd02`)

`cargo test -p jcode-tasker-{types,git,pi,rounds,promotion,orchestration}`:

- types: 10 unit + 13 integration
- git: 4
- pi: 41 (includes 5 mutation-policy tests and e2e canonical rejection)
- rounds: 1 unit + 6 integration
- promotion: 8 integration
- orchestration: 6 unit + 5 e2e

**Total: 81 passed, 0 failed.**

## Gate status

- `test_suite`: **passed** (evidence above, recorded in the feature gates).
- `agent_review`: independent read-only architecture review dispatched 2026-08-01; verdict recorded in the feature when delivered.
- `evidence`: this document.
