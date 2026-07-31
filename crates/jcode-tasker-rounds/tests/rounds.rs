use chrono::{DateTime, Utc};
use jcode_tasker_pi::ConcurrencyStore;
use jcode_tasker_rounds::{RoundCompletion, RoundError, RoundOrchestrator};
use jcode_tasker_types::{
    AdjudicationBallot, AdjudicationPolicyDecision, AdjudicationRoundId, BallotId,
    CandidateAssessment, CandidateId, CandidateSetId, ValidatorIdentity,
};
use serde_json::{Value, json};
use tempfile::tempdir;

const PROJECT_ROOT: &str = "/rounds-test";
const CREATED_AT: &str = "2026-01-01T00:00:00Z";

fn timestamp(value: &str) -> DateTime<Utc> {
    value.parse().expect("valid test timestamp")
}

fn candidate_set(candidate_set_id: CandidateSetId) -> Value {
    json!({
        "id": candidate_set_id,
        "projectId": "proj_00000000-0000-0000-0000-000000000001",
        "taskId": "task_00000000-0000-0000-0000-000000000002",
        "baseRevision": 0,
        "baseCommit": "base-commit",
        "acceptanceDigest": "acceptance-digest",
        "policy": {
            "kind": "ensemble",
            "candidate_count": 3,
            "quorum": 2,
        },
        "policyVersion": 1,
        "state": "open",
        "createdAt": CREATED_AT,
        "updatedAt": CREATED_AT,
    })
}

fn candidate(candidate_set_id: CandidateSetId, candidate_id: CandidateId, ordinal: usize) -> Value {
    let created_at = format!("2026-01-01T00:00:0{ordinal}Z");
    json!({
        "id": candidate_id,
        "candidateSetId": candidate_set_id,
        "state": "eligible",
        "baseCommit": "base-commit",
        "resultCommit": format!("result-{ordinal}"),
        "diffDigest": format!("digest-{ordinal}"),
        "summary": format!("candidate-{ordinal}"),
        "provenance": {
            "sessionId": format!("author-session-{ordinal}"),
            "agentId": format!("author-agent-{ordinal}"),
            "lineageDigest": format!("author-lineage-{ordinal}"),
        },
        "resourceIntents": [],
        "createdAt": created_at,
        "updatedAt": created_at,
        "submittedAt": created_at,
    })
}

fn setup(
    path: &std::path::Path,
    candidate_ids: [CandidateId; 3],
) -> (ConcurrencyStore, CandidateSetId, [CandidateId; 3]) {
    let candidate_set_id = CandidateSetId::new();
    let mut store = ConcurrencyStore::open_path(path, PROJECT_ROOT).expect("open temp store");
    store
        .create_candidate_set(candidate_set(candidate_set_id), 0)
        .expect("create candidate set");
    for (ordinal, candidate_id) in candidate_ids.into_iter().enumerate() {
        store
            .register_candidate(
                candidate(candidate_set_id, candidate_id, ordinal + 1),
                ordinal as u64 + 1,
            )
            .expect("register candidate");
    }
    (store, candidate_set_id, candidate_ids)
}

fn validator(name: &str) -> ValidatorIdentity {
    ValidatorIdentity {
        session_id: format!("session-{name}"),
        agent_id: format!("validator-{name}"),
        model_id: Some("model-test".into()),
        lineage_digest: format!("lineage-{name}"),
    }
}

fn ballot(
    round_id: AdjudicationRoundId,
    ballot_id: BallotId,
    validator_name: &str,
    assessments: Vec<CandidateAssessment>,
    abstained: bool,
    second: u32,
) -> AdjudicationBallot {
    AdjudicationBallot {
        id: ballot_id,
        round_id,
        validator: validator(validator_name),
        assessments,
        ranking: Vec::new(),
        abstained,
        created_at: timestamp(&format!("2026-01-01T00:01:{second:02}Z")),
    }
}

fn approval(candidate_id: CandidateId) -> CandidateAssessment {
    CandidateAssessment {
        candidate_id,
        eligible: true,
        approve: true,
        acceptance_score: 90,
        risk_score: 10,
        complexity_score: 10,
        notes: vec!["evidence: recorded validation output".into()],
    }
}

#[test]
fn full_round_selects_candidate_and_persists_a_promote_ready_decision() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("rounds.sqlite");
    let candidate_ids = [CandidateId::new(), CandidateId::new(), CandidateId::new()];
    let (store, candidate_set_id, candidate_ids) = setup(&path, candidate_ids);
    let mut runner = RoundOrchestrator::new(store);
    let opened = runner.open_round(candidate_set_id, 4).expect("open round");

    let first = runner
        .submit_ballot(
            ballot(
                opened.round_id,
                BallotId::new(),
                "one",
                vec![approval(candidate_ids[0])],
                false,
                1,
            ),
            opened.revision,
        )
        .expect("first ballot");
    assert_eq!(first.completion, RoundCompletion::Pending);
    assert!(!first.complete);

    let second = runner
        .submit_ballot(
            ballot(
                opened.round_id,
                BallotId::new(),
                "two",
                vec![approval(candidate_ids[0])],
                false,
                2,
            ),
            first.revision,
        )
        .expect("second ballot");
    assert_eq!(second.completion, RoundCompletion::QuorumReached);
    assert_eq!(second.selected_candidate_id(), Some(candidate_ids[0]));
    assert!(second.complete);

    let persisted = runner
        .store()
        .adjudication_round(&opened.round_id.to_string())
        .expect("read persisted round")
        .expect("round exists");
    assert_eq!(persisted["decision"]["outcome"], "select");
    assert_eq!(
        persisted["decision"]["candidate_id"],
        candidate_ids[0].to_string()
    );

    let status = runner
        .round_status(opened.round_id, 1)
        .expect("bounded round status");
    assert_eq!(status.ballot_count, 2);
    assert_eq!(status.ballots.len(), 1);
    assert!(status.truncated);
    assert_eq!(status.decision, Some(second.outcome.decision.clone()));

    let reopened = ConcurrencyStore::open_path(&path, PROJECT_ROOT).expect("reopen store");
    let mut replay_runner = RoundOrchestrator::new(reopened);
    let replay = replay_runner
        .replay_round(opened.round_id)
        .expect("replay persisted round");
    assert_eq!(
        serde_json::to_vec(&second.outcome.decision).expect("serialize decision"),
        serde_json::to_vec(&replay.outcome.decision).expect("serialize replay decision")
    );
}

#[test]
fn no_quorum_escalates_when_the_remaining_ballot_budget_cannot_change_the_result() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("rounds.sqlite");
    let candidate_ids = [CandidateId::new(), CandidateId::new(), CandidateId::new()];
    let (store, candidate_set_id, candidate_ids) = setup(&path, candidate_ids);
    let mut runner = RoundOrchestrator::new(store);
    let opened = runner.open_round(candidate_set_id, 4).expect("open round");

    let first = runner
        .submit_ballot(
            ballot(
                opened.round_id,
                BallotId::new(),
                "one",
                vec![approval(candidate_ids[0])],
                false,
                1,
            ),
            opened.revision,
        )
        .expect("first ballot");
    let second = runner
        .submit_ballot(
            ballot(
                opened.round_id,
                BallotId::new(),
                "two",
                vec![approval(candidate_ids[1])],
                false,
                2,
            ),
            first.revision,
        )
        .expect("second ballot");
    assert_eq!(second.completion, RoundCompletion::Pending);
    let third = runner
        .submit_ballot(
            ballot(
                opened.round_id,
                BallotId::new(),
                "three",
                vec![approval(candidate_ids[2])],
                false,
                3,
            ),
            second.revision,
        )
        .expect("third ballot");

    assert_eq!(third.completion, RoundCompletion::QuorumImpossible);
    assert!(matches!(
        third.decision,
        Some(AdjudicationPolicyDecision::Escalation { .. })
    ));
    let persisted = runner
        .store()
        .adjudication_round(&opened.round_id.to_string())
        .expect("read persisted round")
        .expect("round exists");
    assert_eq!(persisted["decision"]["outcome"], "escalation");
}

#[test]
fn duplicate_validator_ballot_is_rejected_before_persistence() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("rounds.sqlite");
    let candidate_ids = [CandidateId::new(), CandidateId::new(), CandidateId::new()];
    let (store, candidate_set_id, candidate_ids) = setup(&path, candidate_ids);
    let mut runner = RoundOrchestrator::new(store);
    let opened = runner.open_round(candidate_set_id, 4).expect("open round");
    let first_ballot_id = BallotId::new();
    let first = runner
        .submit_ballot(
            ballot(
                opened.round_id,
                first_ballot_id,
                "same-validator",
                vec![approval(candidate_ids[0])],
                false,
                1,
            ),
            opened.revision,
        )
        .expect("first ballot");

    let error = runner
        .submit_ballot(
            ballot(
                opened.round_id,
                BallotId::new(),
                "same-validator",
                vec![approval(candidate_ids[1])],
                false,
                2,
            ),
            first.revision,
        )
        .expect_err("duplicate validator must be rejected");
    assert!(matches!(error, RoundError::DuplicateValidator { .. }));
    let status = runner.round_status(opened.round_id, 10).expect("status");
    assert_eq!(status.ballot_count, 1);
}

#[test]
fn evidence_free_non_abstaining_ballot_is_rejected() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("rounds.sqlite");
    let candidate_ids = [CandidateId::new(), CandidateId::new(), CandidateId::new()];
    let (store, candidate_set_id, _) = setup(&path, candidate_ids);
    let mut runner = RoundOrchestrator::new(store);
    let opened = runner.open_round(candidate_set_id, 4).expect("open round");

    let error = runner
        .submit_ballot(
            ballot(
                opened.round_id,
                BallotId::new(),
                "evidence-free",
                Vec::new(),
                false,
                1,
            ),
            opened.revision,
        )
        .expect_err("evidence-free ballot must be rejected");
    assert!(matches!(error, RoundError::EvidenceFreeBallot { .. }));
    let status = runner.round_status(opened.round_id, 10).expect("status");
    assert_eq!(status.ballot_count, 0);
    assert_eq!(
        runner
            .store()
            .current_revision("proj_00000000-0000-0000-0000-000000000001")
            .unwrap(),
        opened.revision
    );
}

#[test]
fn abstention_starvation_fails_the_validity_threshold() {
    let directory = tempdir().expect("temp directory");
    let path = directory.path().join("rounds.sqlite");
    let candidate_ids = [CandidateId::new(), CandidateId::new(), CandidateId::new()];
    let (store, candidate_set_id, _) = setup(&path, candidate_ids);
    let mut runner = RoundOrchestrator::new(store);
    let opened = runner.open_round(candidate_set_id, 4).expect("open round");

    let first = runner
        .submit_ballot(
            ballot(
                opened.round_id,
                BallotId::new(),
                "abstainer-one",
                Vec::new(),
                true,
                1,
            ),
            opened.revision,
        )
        .expect("first abstention");
    assert_eq!(first.completion, RoundCompletion::Pending);

    let second = runner
        .submit_ballot(
            ballot(
                opened.round_id,
                BallotId::new(),
                "abstainer-two",
                Vec::new(),
                true,
                2,
            ),
            first.revision,
        )
        .expect("second abstention");
    assert_eq!(second.completion, RoundCompletion::ValidityThresholdFailed);
    assert!(matches!(
        second.decision,
        Some(AdjudicationPolicyDecision::Escalation { .. })
    ));
}

#[test]
fn replay_is_bit_identical_when_ballots_are_inserted_in_a_different_order() {
    let first_dir = tempdir().expect("first temp directory");
    let second_dir = tempdir().expect("second temp directory");
    let first_path = first_dir.path().join("rounds.sqlite");
    let second_path = second_dir.path().join("rounds.sqlite");
    let candidate_ids = [CandidateId::new(), CandidateId::new(), CandidateId::new()];

    let (first_store, first_set, candidate_ids) = setup(&first_path, candidate_ids);
    let (second_store, second_set, _) = setup(&second_path, candidate_ids);
    let mut first_runner = RoundOrchestrator::new(first_store);
    let mut second_runner = RoundOrchestrator::new(second_store);
    let first_round = first_runner.open_round(first_set, 4).expect("first round");
    let second_round = second_runner
        .open_round(second_set, 4)
        .expect("second round");

    let first_ballots = [
        ballot(
            first_round.round_id,
            BallotId::new(),
            "one",
            vec![approval(candidate_ids[0])],
            false,
            1,
        ),
        ballot(
            first_round.round_id,
            BallotId::new(),
            "two",
            vec![approval(candidate_ids[1])],
            false,
            2,
        ),
        ballot(
            first_round.round_id,
            BallotId::new(),
            "three",
            vec![approval(candidate_ids[2])],
            false,
            3,
        ),
    ];
    let second_ballots = [
        ballot(
            second_round.round_id,
            first_ballots[2].id,
            "three",
            vec![approval(candidate_ids[2])],
            false,
            3,
        ),
        ballot(
            second_round.round_id,
            first_ballots[0].id,
            "one",
            vec![approval(candidate_ids[0])],
            false,
            1,
        ),
        ballot(
            second_round.round_id,
            first_ballots[1].id,
            "two",
            vec![approval(candidate_ids[1])],
            false,
            2,
        ),
    ];

    let mut first_progress = None;
    let mut revision = first_round.revision;
    for ballot in first_ballots {
        let progress = first_runner
            .submit_ballot(ballot, revision)
            .expect("first ballot order");
        revision = progress.revision;
        first_progress = Some(progress);
    }
    let mut second_progress = None;
    let mut revision = second_round.revision;
    for ballot in second_ballots {
        let progress = second_runner
            .submit_ballot(ballot, revision)
            .expect("second ballot order");
        revision = progress.revision;
        second_progress = Some(progress);
    }

    let first_decision = first_progress
        .expect("first terminal progress")
        .outcome
        .decision;
    let second_decision = second_progress
        .expect("second terminal progress")
        .outcome
        .decision;
    assert_eq!(
        serde_json::to_vec(&first_decision).expect("serialize first decision"),
        serde_json::to_vec(&second_decision).expect("serialize second decision")
    );
}
