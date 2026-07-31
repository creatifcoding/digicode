use chrono::Utc;
use jcode_tasker_git::{CandidateChange, CandidateRef, CommitIdentity, GitCandidateAdapter};
use jcode_tasker_orchestration::{
    AcceptanceContract, CandidateOrchestrator, CandidateSetOpened, CandidateSubmission,
    OpenCandidateSetRequest, ProvenanceTemplate, ValidationCommand,
};
use jcode_tasker_pi::{
    CandidateMutationContext, CanonicalMutation, ConcurrencyStore, MutationPolicyDecision,
    MutationPolicyGate, MutationPolicyRejection,
};
use jcode_tasker_promotion::{PromotionReconciler, PromotionRequest, PromotionSagaError};
use jcode_tasker_rounds::RoundCompletion;
use jcode_tasker_types::{
    AdjudicationBallot, AdjudicationRoundId, BallotId, CandidateAssessment, CandidateId,
    CandidateSetId, ConcurrencyPolicy, ProjectId, ResourceAccess, ResourceIntent, ResourceKind,
    TaskId, ValidatorIdentity,
};
use rusqlite::{Connection, TransactionBehavior, params};
use serde_json::json;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use tempfile::{TempDir, tempdir};

const CANONICAL_REF: &str = "refs/heads/main";

struct Fixture {
    _directory: TempDir,
    repository: PathBuf,
    database: PathBuf,
    project_root: String,
    project_id: ProjectId,
    task_id: TaskId,
    base_commit: String,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempdir().expect("create fixture tempdir");
        let repository = directory.path().join("repo");
        fs::create_dir_all(&repository).expect("create repository");
        git(&repository, &["init", "-b", "main"]);
        git(&repository, &["config", "user.name", "Tasker E2E Tests"]);
        git(
            &repository,
            &["config", "user.email", "tasker-e2e@example.invalid"],
        );
        fs::write(repository.join("README.md"), "base\n").expect("write base file");
        git(&repository, &["add", "README.md"]);
        git(&repository, &["commit", "-m", "base"]);
        let base_commit = git(&repository, &["rev-parse", CANONICAL_REF]);

        Self {
            project_root: repository.to_string_lossy().into_owned(),
            database: directory.path().join("tasker.sqlite"),
            repository,
            project_id: ProjectId::new(),
            task_id: TaskId::new(),
            base_commit,
            _directory: directory,
        }
    }

    fn orchestrator(&self) -> CandidateOrchestrator {
        let store = ConcurrencyStore::open_path(&self.database, &self.project_root)
            .expect("open concurrency store");
        let git = GitCandidateAdapter::try_new(&self.repository).expect("open Git adapter");
        CandidateOrchestrator::new(store, git).expect("open candidate orchestrator")
    }

    fn promotion(&self) -> PromotionReconciler {
        let store = ConcurrencyStore::open_path(&self.database, &self.project_root)
            .expect("open promotion store");
        let git = GitCandidateAdapter::try_new(&self.repository).expect("open promotion Git");
        PromotionReconciler::new(store, git)
    }

    fn open_three_lanes(&self) -> (CandidateOrchestrator, CandidateSetOpened) {
        let mut orchestrator = self.orchestrator();
        let acceptance = AcceptanceContract::new(
            vec![ValidationCommand::new("true", std::iter::empty::<&str>())],
            "Every candidate passes the recorded validation contract.",
            vec![write_intent("candidate-output.txt")],
        );
        let request = OpenCandidateSetRequest::new(
            self.project_id,
            self.task_id,
            self.base_commit.clone(),
            ConcurrencyPolicy::Ensemble {
                candidate_count: 3,
                quorum: 2,
            },
            3,
            acceptance,
            ProvenanceTemplate::new("author-session", "author-agent"),
            self._directory.path().join("worktrees"),
        );
        let opened = orchestrator
            .open_candidate_set(request)
            .expect("open triplicate candidate set");
        (orchestrator, opened)
    }

    fn submit_all(
        &self,
        orchestrator: &mut CandidateOrchestrator,
        opened: &CandidateSetOpened,
    ) -> (u64, Vec<String>) {
        let mut revision = opened.revision;
        let mut commits = Vec::with_capacity(opened.lanes.len());
        for (ordinal, lane) in opened.lanes.iter().enumerate() {
            revision = orchestrator
                .start_lane(lane.candidate_id, revision)
                .expect("start candidate lane")
                .revision;
            let commit = self.capture_candidate(
                &lane.candidate_ref,
                &format!("candidate-{ordinal}\n"),
                &format!("candidate {ordinal} implementation"),
            );
            let mut submission = CandidateSubmission::new(commit.clone());
            submission.diff_digest = Some(format!("sha1:candidate-{ordinal}"));
            submission.summary = Some(format!("candidate {ordinal} implementation"));
            submission.evidence = vec![json!({
                "ref": format!("validator://candidate/{ordinal}"),
                "command": "true",
                "status": "passed",
                "candidate": ordinal,
            })];
            revision = orchestrator
                .submit_lane(lane.candidate_id, revision, submission)
                .expect("submit candidate lane")
                .revision;
            commits.push(commit);
        }
        let ids = opened
            .lanes
            .iter()
            .map(|lane| lane.candidate_id)
            .collect::<Vec<_>>();
        let revision = self.mark_candidates_eligible(&ids, revision);
        (revision, commits)
    }

    fn capture_candidate(&self, candidate_ref: &str, contents: &str, message: &str) -> String {
        fs::write(self.repository.join("candidate-output.txt"), contents)
            .expect("write candidate implementation");
        git(&self.repository, &["add", "candidate-output.txt"]);
        let tree = git(&self.repository, &["write-tree"]);
        let candidate_ref = CandidateRef::parse(candidate_ref).expect("parse candidate ref");
        let commit = GitCandidateAdapter::try_new(&self.repository)
            .expect("open capture Git adapter")
            .capture_candidate_change(
                &candidate_ref,
                &CandidateChange::new(tree, message).with_identity(CommitIdentity::new(
                    "Candidate Writer",
                    "candidate-writer@example.invalid",
                )),
            )
            .expect("capture candidate commit");
        // Keep each lane's tree isolated from the next fixture write. The
        // candidate commit is already immutable, so resetting only the index
        // cannot affect the captured tip.
        git(&self.repository, &["reset", "--quiet"]);
        commit
    }

    /// Simulate the external validator's eligibility write as one durable batch.
    /// The production lane boundary records `submitted`; validation is the next
    /// adapter-owned transition and is intentionally kept out of this fixture.
    fn mark_candidates_eligible(
        &self,
        candidate_ids: &[CandidateId],
        expected_revision: u64,
    ) -> u64 {
        let mut connection = Connection::open(&self.database).expect("open eligibility database");
        connection
            .busy_timeout(Duration::from_secs(30))
            .expect("set eligibility busy timeout");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin eligibility transaction");
        let actual: i64 = transaction
            .query_row(
                "SELECT revision FROM concurrency_project_revisions WHERE project_id = ?1",
                [self.project_id.to_string()],
                |row| row.get(0),
            )
            .expect("read eligibility revision");
        assert_eq!(
            u64::try_from(actual).expect("non-negative revision"),
            expected_revision
        );

        for candidate_id in candidate_ids {
            let changed = transaction
                .execute(
                    "UPDATE candidates SET state = 'eligible', updated_at = ?1 WHERE id = ?2",
                    params![Utc::now().to_rfc3339(), candidate_id.to_string()],
                )
                .expect("mark candidate eligible");
            assert_eq!(changed, 1, "candidate must exist before validation");
        }
        let changed = transaction
            .execute(
                "UPDATE concurrency_project_revisions
                 SET revision = revision + 1, updated_at = ?1
                 WHERE project_id = ?2 AND revision = ?3",
                params![
                    Utc::now().to_rfc3339(),
                    self.project_id.to_string(),
                    i64::try_from(expected_revision).expect("revision fits SQLite")
                ],
            )
            .expect("bump validation revision");
        assert_eq!(changed, 1, "eligibility must use the expected revision");
        transaction.commit().expect("commit eligibility batch");
        expected_revision + 1
    }
}

struct SubmittedPipeline {
    fixture: Fixture,
    orchestrator: CandidateOrchestrator,
    opened: CandidateSetOpened,
    revision: u64,
    commits: Vec<String>,
}

struct SelectedPipeline {
    fixture: Fixture,
    candidate_set_id: CandidateSetId,
    candidate_ids: Vec<CandidateId>,
    commits: Vec<String>,
    winner: CandidateId,
    request: PromotionRequest,
}

fn submitted_pipeline() -> SubmittedPipeline {
    let fixture = Fixture::new();
    let (mut orchestrator, opened) = fixture.open_three_lanes();
    let (revision, commits) = fixture.submit_all(&mut orchestrator, &opened);
    SubmittedPipeline {
        fixture,
        orchestrator,
        opened,
        revision,
        commits,
    }
}

fn selected_pipeline() -> SelectedPipeline {
    let mut pipeline = submitted_pipeline();
    let candidate_set_id = pipeline.opened.candidate_set.id;
    let candidate_ids = pipeline
        .opened
        .lanes
        .iter()
        .map(|lane| lane.candidate_id)
        .collect::<Vec<_>>();
    let handoff = pipeline
        .orchestrator
        .handoff_to_round(candidate_set_id, pipeline.revision)
        .expect("handoff submitted lanes to adjudication");
    let mut round = handoff.round;
    let first = round
        .submit_ballot(
            ballot(handoff.opened.round_id, "validator-one", candidate_ids[0]),
            handoff.opened.revision,
        )
        .expect("submit first validator ballot");
    assert_eq!(first.completion, RoundCompletion::Pending);
    let selected = round
        .submit_ballot(
            ballot(handoff.opened.round_id, "validator-two", candidate_ids[0]),
            first.revision,
        )
        .expect("submit quorum validator ballot");
    assert_eq!(selected.completion, RoundCompletion::QuorumReached);
    assert_eq!(selected.selected_candidate_id(), Some(candidate_ids[0]));
    assert!(matches!(
        selected.decision,
        Some(jcode_tasker_types::AdjudicationPolicyDecision::Select { candidate_id })
            if candidate_id == candidate_ids[0]
    ));
    let expected_revision = selected.revision;
    let winner = candidate_ids[0];
    let project_id = pipeline.fixture.project_id.to_string();
    let task_id = pipeline.fixture.task_id.to_string();
    assert_eq!(pipeline.commits.len(), 3);
    assert_eq!(
        pipeline.commits.iter().collect::<BTreeSet<_>>().len(),
        3,
        "each candidate must capture a distinct implementation commit"
    );
    assert_eq!(
        pipeline
            .orchestrator
            .git()
            .list_candidate_ref_names()
            .expect("list isolated candidate refs")
            .len(),
        3
    );
    assert!(
        pipeline
            .orchestrator
            .git()
            .assert_isolated()
            .expect("prove candidate/canonical isolation")
            .is_isolated()
    );
    let fixture = pipeline.fixture;
    let commits = pipeline.commits;
    drop(round);
    drop(pipeline.orchestrator);

    SelectedPipeline {
        fixture,
        candidate_set_id,
        candidate_ids,
        commits,
        winner,
        request: PromotionRequest::promote(
            project_id,
            task_id,
            candidate_set_id.to_string(),
            winner.to_string(),
            CANONICAL_REF,
            expected_revision,
        )
        .with_intent_id("e2e-promote-winner"),
    }
}

#[test]
fn happy_path_reconciles_three_lanes_and_cleans_loser_refs() {
    let pipeline = selected_pipeline();
    let expected_winner_ref = CandidateRef::new(pipeline.candidate_set_id, pipeline.winner);
    let mut reconciler = pipeline.fixture.promotion();

    let receipt = reconciler
        .promote(&pipeline.request)
        .expect("promote adjudicated winner");
    assert_eq!(receipt.target_commit, pipeline.commits[0]);
    assert_eq!(
        git(&pipeline.fixture.repository, &["rev-parse", CANONICAL_REF]),
        pipeline.commits[0]
    );

    let candidate_refs = reconciler
        .git()
        .list_candidate_ref_names()
        .expect("list post-promotion candidate refs");
    assert_eq!(candidate_refs, vec![expected_winner_ref.to_string()]);
    assert_eq!(
        reconciler
            .store()
            .promotion_intent(&receipt.intent_id)
            .expect("read promotion intent")
            .expect("promotion intent exists")["state"],
        "finalized"
    );
    assert_eq!(
        reconciler
            .store()
            .candidate(&pipeline.winner.to_string())
            .expect("read winner")
            .expect("winner exists")["state"],
        "promoted"
    );
    assert_eq!(
        reconciler
            .store()
            .candidate_set(&pipeline.candidate_set_id.to_string())
            .expect("read candidate set")
            .expect("candidate set exists")["state"],
        "completed"
    );
    for loser in pipeline.candidate_ids.iter().skip(1) {
        assert_ne!(
            reconciler
                .store()
                .candidate(&loser.to_string())
                .expect("read loser")
                .expect("loser exists")["state"],
            "promoted"
        );
    }
}

#[test]
fn mutation_policy_rejects_canonical_conflict_and_admits_valid_speculation() {
    let fixture = Fixture::new();
    let (orchestrator, opened) = fixture.open_three_lanes();
    let mutation = CanonicalMutation::new(
        fixture.task_id.to_string(),
        vec![write_intent("candidate-output.txt")],
        None,
    );
    let decision = MutationPolicyGate::new(orchestrator.store())
        .evaluate(&mutation)
        .expect("evaluate canonical mutation");
    let MutationPolicyDecision::Reject { reason } = decision else {
        panic!("active ensemble must reject an unscoped canonical mutation");
    };
    assert!(matches!(reason, MutationPolicyRejection::Conflict { .. }));
    assert!(reason.to_string().contains("author-agent"));
    assert!(
        reason
            .to_string()
            .contains(&opened.candidate_set.id.to_string())
    );

    let speculative = CanonicalMutation::new(
        fixture.task_id.to_string(),
        vec![write_intent("candidate-output.txt")],
        Some(CandidateMutationContext::new(
            opened.candidate_set.id.to_string(),
            opened.lanes[0].candidate_id.to_string(),
        )),
    );
    assert_eq!(
        MutationPolicyGate::new(orchestrator.store())
            .evaluate(&speculative)
            .expect("evaluate candidate-scoped mutation"),
        MutationPolicyDecision::AdmitSpeculative {
            candidate_id: opened.lanes[0].candidate_id.to_string(),
        }
    );
}

#[test]
fn promotion_recovery_rolls_back_prepared_and_finalizes_ref_updated_intents() {
    let rollback = selected_pipeline();
    let mut reconciler = rollback.fixture.promotion();
    let prepared = reconciler
        .prepare_promotion(&rollback.request)
        .expect("record prepared promotion intent");
    drop(reconciler);

    let mut restarted = rollback.fixture.promotion();
    let recovered = restarted
        .recover_promotion(&prepared.intent_id)
        .expect("recover intent after crash before ref update");
    assert_eq!(
        recovered.action,
        jcode_tasker_pi::PromotionRecoveryAction::Rollback
    );
    assert_eq!(recovered.intent["state"], "aborted");
    assert_eq!(
        git(&rollback.fixture.repository, &["rev-parse", CANONICAL_REF]),
        rollback.fixture.base_commit
    );
    assert_eq!(
        restarted
            .git()
            .list_candidate_ref_names()
            .expect("list refs after rollback")
            .len(),
        3
    );

    let forward = selected_pipeline();
    let mut reconciler = forward.fixture.promotion();
    let prepared = reconciler
        .prepare_promotion(&forward.request)
        .expect("record second prepared promotion intent");
    reconciler
        .compare_and_swap_canonical_ref(&prepared)
        .expect("update canonical ref before simulated crash");
    drop(reconciler);

    let mut restarted = forward.fixture.promotion();
    let recovered = restarted
        .recover_promotion(&prepared.intent_id)
        .expect("recover intent after canonical ref update");
    assert_eq!(
        recovered.action,
        jcode_tasker_pi::PromotionRecoveryAction::Finalize
    );
    assert_eq!(recovered.intent["state"], "finalized");
    assert_eq!(
        git(&forward.fixture.repository, &["rev-parse", CANONICAL_REF]),
        forward.commits[0]
    );
    assert_eq!(
        restarted
            .git()
            .list_candidate_ref_names()
            .expect("list refs after forward recovery"),
        vec![CandidateRef::new(forward.candidate_set_id, forward.winner).to_string()]
    );
}

#[test]
fn stale_base_rejection_preserves_the_foreign_canonical_ref() {
    let pipeline = selected_pipeline();
    let foreign = foreign_commit(&pipeline.fixture.repository, &pipeline.fixture.base_commit);
    git(
        &pipeline.fixture.repository,
        &["update-ref", CANONICAL_REF, &foreign],
    );
    let mut reconciler = pipeline.fixture.promotion();

    let error = reconciler
        .promote(&pipeline.request)
        .expect_err("candidate built from a stale canonical base must be rejected");
    assert!(matches!(
        error,
        PromotionSagaError::StaleCanonicalBase { .. }
    ));
    assert_eq!(
        git(&pipeline.fixture.repository, &["rev-parse", CANONICAL_REF]),
        foreign
    );
    assert!(
        reconciler
            .store()
            .incomplete_promotions(&pipeline.fixture.project_id.to_string(), 10)
            .expect("read incomplete promotions")
            .is_empty()
    );
}

#[test]
fn split_validator_ballots_escalate_without_promoting_any_candidate() {
    let mut pipeline = submitted_pipeline();
    let candidate_ids = pipeline
        .opened
        .lanes
        .iter()
        .map(|lane| lane.candidate_id)
        .collect::<Vec<_>>();
    let handoff = pipeline
        .orchestrator
        .handoff_to_round(pipeline.opened.candidate_set.id, pipeline.revision)
        .expect("handoff submitted lanes to split-ballot round");
    let mut round = handoff.round;
    let first = round
        .submit_ballot(
            ballot(handoff.opened.round_id, "validator-one", candidate_ids[0]),
            handoff.opened.revision,
        )
        .expect("submit first split ballot");
    let second = round
        .submit_ballot(
            ballot(handoff.opened.round_id, "validator-two", candidate_ids[1]),
            first.revision,
        )
        .expect("submit second split ballot");
    let final_progress = round
        .submit_ballot(
            ballot(handoff.opened.round_id, "validator-three", candidate_ids[2]),
            second.revision,
        )
        .expect("submit final split ballot");
    assert_eq!(final_progress.completion, RoundCompletion::QuorumImpossible);
    assert!(matches!(
        final_progress.decision,
        Some(jcode_tasker_types::AdjudicationPolicyDecision::Escalation { .. })
    ));
    assert_eq!(
        round
            .store()
            .candidate_set(&pipeline.opened.candidate_set.id.to_string())
            .expect("read escalated candidate set")
            .expect("candidate set exists")["state"],
        "adjudicating"
    );
    assert_eq!(
        git(&pipeline.fixture.repository, &["rev-parse", CANONICAL_REF]),
        pipeline.fixture.base_commit
    );
    assert!(
        round
            .store()
            .incomplete_promotions(&pipeline.fixture.project_id.to_string(), 10)
            .expect("read promotions after escalation")
            .is_empty()
    );
}

fn write_intent(selector: &str) -> ResourceIntent {
    ResourceIntent {
        kind: ResourceKind::File,
        selector: selector.into(),
        access: ResourceAccess::ProposeWrite,
        rationale: Some("candidate implementation output".into()),
    }
}

fn ballot(
    round_id: AdjudicationRoundId,
    validator_name: &str,
    candidate_id: CandidateId,
) -> AdjudicationBallot {
    AdjudicationBallot {
        id: BallotId::new(),
        round_id,
        validator: ValidatorIdentity {
            session_id: format!("session-{validator_name}"),
            agent_id: validator_name.into(),
            model_id: Some("validator-model".into()),
            lineage_digest: format!("lineage-{validator_name}"),
        },
        assessments: vec![CandidateAssessment {
            candidate_id,
            eligible: true,
            approve: true,
            acceptance_score: 90,
            risk_score: 10,
            complexity_score: 10,
            notes: vec!["recorded validation passed".into()],
        }],
        ranking: vec![candidate_id],
        abstained: false,
        created_at: Utc::now(),
    }
}

fn git(repository: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(repository)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("spawn git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .to_owned()
}

fn foreign_commit(repository: &Path, parent: &str) -> String {
    let tree = git(repository, &["rev-parse", &format!("{parent}^{{tree}}")]);
    let output = Command::new("git")
        .current_dir(repository)
        .args([
            "commit-tree",
            &tree,
            "-p",
            parent,
            "-m",
            "foreign canonical change",
        ])
        .output()
        .expect("spawn git commit-tree");
    assert!(
        output.status.success(),
        "git commit-tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("foreign commit OID is UTF-8")
        .trim()
        .to_owned()
}
