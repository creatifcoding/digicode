# Proposal: Tasker Concurrency Artifacts and Adjudicated Promotion

> **Status:** Proposed architecture, implementation started
> **Date:** 2026-07-30
> **Initiative:** `native-tasker`
> **Related:** [`NATIVE_TASKER.md`](./NATIVE_TASKER.md), [`NATIVE_METATOOL.md`](./NATIVE_METATOOL.md)

## 1. Decision

Tasker will separate **speculative authorship** from **canonical mutation**.

Multiple agents may implement the same task concurrently as isolated **candidates**. Candidates form an immutable Git-backed merge DAG and carry Tasker-owned provenance, resource intent, evidence, and validation results. A single governed **promotion reconciler** may update the canonical branch after hard gates and an evidence-bearing **adjudication quorum** select, compose, or reject candidates.

The exact claim is:

> Exclusivity belongs at canonical promotion, not at candidate authorship.

This permits triplicate implementations without allowing three agents to overwrite the same canonical files.

## 2. Terminology

### Candidate set

One bounded speculative round for a Tasker task. It fixes:

- canonical task and project;
- base Git commit and Tasker revision;
- shared acceptance contract;
- concurrency policy;
- maximum candidate count;
- adjudication policy version.

### Candidate

One immutable implementation proposal. Its code content is addressed by Git commit/tree identity. Tasker stores metadata, not source blobs.

A candidate records:

- candidate-set and work-unit identity;
- author/session/agent provenance;
- base commit and result commit;
- declared resource intent;
- diff digest and bounded summary;
- evidence references;
- validation status;
- supersession lineage.

### Merge DAG

The immutable candidate and reconciliation lineage. Git owns commits, trees, and parent edges. Tasker owns semantic links between candidates, evidence, ballots, and promotion decisions.

“Merge tree” is reserved for Git's computed tree result. The larger artifact is the **merge DAG**. One noun, one concept. Civilization narrowly preserved.

### Ballot

An independent validator's structured assessment of eligible candidates against the shared acceptance contract. A ballot records hard-gate results, scored criteria, detected risks, abstention, and ranking.

### Adjudication quorum

A deterministic policy that aggregates eligible ballots. This is not Raft, Paxos, Byzantine consensus, or leader election. It is evidence-backed selection under one authoritative Tasker service.

### Reconciliation candidate

A new candidate synthesized from two or more candidates when no existing candidate is directly promotable. Synthesis never mutates canonical state and must pass the same gates.

### Promotion

The governed transition that advances the canonical Git ref and Tasker state to one selected candidate. Promotion is single-writer, optimistic, audited, and recoverable.

## 3. Concurrency policies

Tasks may declare one policy:

```text
exclusive
speculative(max_candidates)
ensemble(candidate_count, quorum)
```

- `exclusive` preserves ordinary lock behavior.
- `speculative` permits independent candidates but leaves selection operator- or policy-driven.
- `ensemble` requests a fixed candidate count and adjudication quorum. Triplicate mode is `ensemble(3, 2)` by default.

Candidate authors do not acquire the canonical promotion lock. They receive candidate-scoped work units and isolated Git refs/worktrees. The promotion reconciler alone acquires the canonical task/resource lock.

## 4. Resource intent

Each candidate declares a bounded resource-intent set before execution:

```text
ResourceIntent {
  kind: file | directory | task | schema | external,
  selector,
  access: read | propose_write,
  rationale?,
}
```

Overlap between candidate intents is allowed and expected. Overlap with a foreign canonical promotion lock is not.

Resource intent is evidence and scheduling input, not a security boundary. Runtime filesystem and tool policy remain authoritative.

## 5. Persistence ownership

### Git owns

- candidate commits and trees;
- candidate refs;
- reconciliation commits;
- computed merge bases and merge trees;
- canonical branch/ref updates.

Candidate refs use a reserved namespace:

```text
refs/jcode/tasker/<task-id>/<candidate-set-id>/<candidate-id>
```

### Tasker owns

- candidate-set policy and base identities;
- candidate provenance and state;
- resource intent;
- evidence references;
- validation results;
- ballots and adjudication decisions;
- promotion intent and recovery state;
- links to work units, sessions, agents, and commits.

Proposed native tables:

```text
candidate_sets
candidates
candidate_resource_intents
candidate_evidence
adjudication_rounds
adjudication_ballots
promotion_intents
promotion_events
```

The Pi-compatible database remains a bridge. Native concurrency artifacts belong to the Jcode-native Tasker store and project revision model.

## 6. Candidate lifecycle

```text
registered
  -> authoring
  -> submitted
  -> validating
  -> eligible | rejected | failed
  -> selected | superseded
  -> promoted
```

A submitted candidate is immutable. Any correction creates a successor candidate linked by `supersedes_candidate_id`.

Candidate failure does not fail the canonical task while another eligible candidate or synthesis path remains.

## 7. Adjudication

Hard gates run before scoring. A candidate with a failed non-waivable gate is ineligible.

Default triplicate adjudication:

1. Three candidates start from the same base commit, Tasker revision, acceptance contract, and policy version.
2. At least two independent validators produce usable ballots.
3. Candidates are ordered by:
   1. hard-gate eligibility;
   2. quorum approval count;
   3. acceptance score;
   4. risk score, ascending;
   5. change complexity, ascending;
   6. candidate ID for deterministic ties.
4. A candidate requires the configured approval quorum.
5. A tie or incompatible strengths may create a reconciliation candidate.
6. No eligible quorum result blocks promotion and returns the decision to the operator or a higher-cost adjudicator.

Validator independence is provenance-aware. Duplicate ballots from the same effective model/session lineage do not satisfy quorum unless policy explicitly permits correlated voting.

## 8. Canonical promotion

SQLite and Git do not share one transaction. Promotion therefore uses a durable, idempotent saga:

1. **Prepare**
   - verify candidate eligibility and decision;
   - acquire canonical promotion lock;
   - verify Tasker revision, canonical ref, base commit, and policy version;
   - compute the promoted Git object without updating the canonical ref.
2. **Record intent**
   - persist a `prepared` promotion intent containing expected and target identities;
   - append an outbox event.
3. **Compare-and-swap Git ref**
   - update the canonical ref only if it still equals the expected commit.
4. **Finalize Tasker**
   - mark the candidate promoted;
   - close or advance the work unit/task according to gates;
   - release locks;
   - increment the project revision and append evidence.
5. **Recover**
   - startup reconciliation inspects incomplete intents;
   - if the ref equals the target, finalize idempotently;
   - if the ref equals the expected base, retry or abort by policy;
   - otherwise mark conflicted and require rebase/re-adjudication.

No direct candidate writer may update the canonical ref.

## 9. Enforcement

Before any canonical edit/write tool, Tasker policy checks:

- active task and work unit;
- canonical promotion lock ownership;
- resource scope;
- expected Tasker revision;
- expected Git ref/base;
- unresolved promotion intents.

Candidate workspaces are exempt from canonical resource locks only inside their isolated candidate ref/worktree. They remain subject to filesystem containment and tool policy.

## 10. Generalized candidate design

The same candidate/reconcile/promote pattern applies when a domain has:

1. independently generable proposals;
2. objective or reviewable evidence;
3. reversible candidate construction;
4. one governed canonical promotion boundary.

Suitable domains:

- code implementations and refactors;
- migrations and infrastructure plans before permit-gated execution;
- prompts, policies, and agent procedures;
- architecture and technical prose;
- UI designs and generated assets;
- test strategies and optimization experiments;
- Tasker plans themselves.

Unsuitable domains:

- direct external side effects such as payments, email sends, DNS cutovers, or production mutation;
- work whose candidates cannot be isolated or replayed;
- voting over facts that require measurement rather than preference.

External side effects may have candidate **plans**, but execution remains singular and governed.

## 11. Initial public surface

The initial native API should be semantic rather than thirty new microtools:

```text
create_candidate_set(task, policy, base)
register_candidate(set, work_unit, ref, intent)
submit_candidate(candidate, evidence)
record_ballot(round, ballot)
adjudicate(round)
prepare_promotion(decision)
execute_promotion(intent)
recover_promotions(project)
```

MetaTool receives a bounded `mt.tasker.candidates` capability over these operations. Guest code never receives Git or SQLite handles.

## 12. Validation contract

The feature is not complete until one end-to-end triplicate exercise proves:

- three isolated candidates can modify overlapping files without canonical mutation;
- candidate commits and evidence are retained after agent exit;
- failed candidates do not poison eligible siblings;
- ballots are attributable and quorum rules deterministic;
- a changed canonical base rejects promotion;
- a crash between Git compare-and-swap and Tasker finalization recovers correctly;
- only one candidate becomes canonical;
- losing candidates remain inspectable;
- Tasker revision, Git ref, evidence, and task state agree after recovery.

## 13. Implementation sequence

1. Land domain types and ontology tests.
2. Add native store migrations and repository operations.
3. Add Git candidate-ref adapter and isolation tests.
4. Add adjudication policy and pure deterministic tests.
5. Add promotion saga and crash-recovery tests.
6. Enforce canonical mutation policy at tool boundaries.
7. Add headless triplicate orchestration.
8. Run a real three-candidate implementation and retain an evidence pack.

## 14. Open grounding

The exact Git implementation crate requires a focused spike. Jcode currently has no established `gix` or `git2` dependency. Prefer `gix` if its compile cost and ref-transaction APIs are acceptable; otherwise use a narrowly wrapped non-interactive Git CLI adapter. This requires additional grounding before we assert the adapter as doctrine.
