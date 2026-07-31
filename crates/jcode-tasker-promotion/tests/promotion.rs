use jcode_tasker_git::GitCandidateAdapter;
use jcode_tasker_pi::{ConcurrencyStore, PromotionRecoveryAction};
use jcode_tasker_promotion::{
    PreparedPromotion, PromotionDecision, PromotionReconciler, PromotionRequest, PromotionSagaError,
};
use jcode_tasker_types::{CandidateId, CandidateSetId};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

const CANONICAL_REF: &str = "refs/heads/main";

struct Fixture {
    _directory: TempDir,
    repository: PathBuf,
    database: PathBuf,
    project_root: String,
    project_id: String,
    task_id: String,
    candidate_set_id: String,
    candidate_id: String,
    base_oid: String,
    target_oid: String,
}

impl Fixture {
    fn request(&self) -> PromotionRequest {
        PromotionRequest::promote(
            &self.project_id,
            &self.task_id,
            &self.candidate_set_id,
            &self.candidate_id,
            CANONICAL_REF,
            2,
        )
        .with_intent_id("promote_fixture")
    }

    fn reconciler(&self) -> PromotionReconciler {
        let store = ConcurrencyStore::open_path(&self.database, &self.project_root)
            .expect("open fixture concurrency store");
        let git = GitCandidateAdapter::try_new(&self.repository).expect("open fixture Git repo");
        PromotionReconciler::new(store, git)
    }

    fn canonical_oid(&self) -> String {
        git(&self.repository, &["rev-parse", CANONICAL_REF])
    }
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("fixture tempdir");
    let repository = directory.path().join("repo");
    fs::create_dir_all(&repository).expect("create repository directory");
    git(&repository, &["init", "-b", "main"]);
    git(&repository, &["config", "user.name", "Promotion Tests"]);
    git(
        &repository,
        &["config", "user.email", "promotion-tests@example.com"],
    );

    fs::write(repository.join("README.md"), "base\n").expect("write base file");
    git(&repository, &["add", "README.md"]);
    git(&repository, &["commit", "-m", "base"]);
    let base_oid = git(&repository, &["rev-parse", "HEAD"]);

    let candidate_set_id = CandidateSetId::new().to_string();
    let candidate_id = CandidateId::new().to_string();
    let adapter = GitCandidateAdapter::try_new(&repository).expect("open candidate adapter");
    let candidate_ref = adapter
        .create_candidate_ref(
            candidate_set_id.parse().expect("candidate set ID"),
            candidate_id.parse().expect("candidate ID"),
            &base_oid,
        )
        .expect("create candidate ref");

    fs::write(repository.join("README.md"), "candidate\n").expect("write candidate file");
    git(&repository, &["add", "README.md"]);
    let tree_oid = git(&repository, &["write-tree"]);
    let target_oid = adapter
        .capture_change(&candidate_ref, &tree_oid, "candidate change")
        .expect("capture candidate commit");

    let database = directory.path().join("tasker.sqlite");
    let project_root = directory.path().to_string_lossy().into_owned();
    let project_id = "proj_promotion_fixture".to_owned();
    let task_id = "task_promotion_fixture".to_owned();
    let mut store =
        ConcurrencyStore::open_path(&database, &project_root).expect("open fixture store for seed");
    store
        .create_candidate_set(
            json!({
                "id": candidate_set_id,
                "projectId": project_id,
                "taskId": task_id,
                "baseRevision": 0,
                "baseCommit": base_oid,
                "acceptanceDigest": "acceptance-digest",
                "policy": {"kind": "ensemble", "candidateCount": 2, "quorum": 1},
                "policyVersion": 1,
                "state": "decided",
                "createdAt": "2026-07-31T00:00:00Z",
                "updatedAt": "2026-07-31T00:00:00Z"
            }),
            0,
        )
        .expect("seed candidate set");
    store
        .register_candidate(
            json!({
                "id": candidate_id,
                "candidateSetId": candidate_set_id,
                "state": "selected",
                "baseCommit": base_oid,
                "resultCommit": target_oid,
                "diffDigest": "candidate-digest",
                "summary": "selected fixture candidate",
                "provenance": {
                    "sessionId": "session-promotion",
                    "agentId": "agent-promotion",
                    "modelId": "model-promotion",
                    "workUnitId": "wu-promotion",
                    "lineageDigest": "lineage-promotion"
                },
                "resourceIntents": [],
                "createdAt": "2026-07-31T00:00:00Z",
                "updatedAt": "2026-07-31T00:00:00Z",
                "submittedAt": "2026-07-31T00:00:00Z"
            }),
            1,
        )
        .expect("seed candidate");

    Fixture {
        _directory: directory,
        repository,
        database,
        project_root,
        project_id,
        task_id,
        candidate_set_id,
        candidate_id,
        base_oid,
        target_oid,
    }
}

#[test]
fn happy_path_promotes_selected_candidate_and_finalizes_tasker() {
    let fixture = fixture();
    let mut reconciler = fixture.reconciler();

    let receipt = reconciler
        .promote(&fixture.request())
        .expect("happy-path promotion");

    assert_eq!(receipt.target_commit, fixture.target_oid);
    assert_eq!(fixture.canonical_oid(), fixture.target_oid);
    assert_eq!(
        reconciler
            .store()
            .promotion_intent("promote_fixture")
            .unwrap()
            .unwrap()["state"],
        "finalized"
    );
    assert_eq!(
        reconciler
            .store()
            .candidate(&fixture.candidate_id)
            .unwrap()
            .unwrap()["state"],
        "promoted"
    );
    assert_eq!(
        reconciler
            .store()
            .candidate_set(&fixture.candidate_set_id)
            .unwrap()
            .unwrap()["state"],
        "completed"
    );
    assert_eq!(
        reconciler
            .store()
            .current_revision(&fixture.project_id)
            .unwrap(),
        5
    );
}

#[test]
fn stale_canonical_base_is_typed_and_never_overwritten() {
    let fixture = fixture();
    let foreign_oid = foreign_commit(&fixture.repository, &fixture.base_oid);
    update_ref(&fixture.repository, CANONICAL_REF, &foreign_oid);
    let mut reconciler = fixture.reconciler();

    let error = reconciler
        .promote(&fixture.request())
        .expect_err("stale canonical base must reject promotion");
    assert!(matches!(
        error,
        PromotionSagaError::StaleCanonicalBase { .. }
    ));
    assert_eq!(fixture.canonical_oid(), foreign_oid);
    assert_eq!(
        reconciler
            .store()
            .current_revision(&fixture.project_id)
            .unwrap(),
        2
    );
    assert!(
        reconciler
            .store()
            .promotion_intent("promote_fixture")
            .unwrap()
            .is_none()
    );
}

#[test]
fn stale_tasker_revision_is_typed_before_git_mutation() {
    let fixture = fixture();
    let mut request = fixture.request();
    request.expected_revision = 1;
    let mut reconciler = fixture.reconciler();

    let error = reconciler
        .promote(&request)
        .expect_err("stale Tasker revision must reject promotion");
    assert!(matches!(
        error,
        PromotionSagaError::StaleTaskerRevision {
            expected: 1,
            actual: 2
        }
    ));
    assert_eq!(fixture.canonical_oid(), fixture.base_oid);
}

#[test]
fn kill_after_intent_recovers_by_aborting_when_ref_did_not_move() {
    let fixture = fixture();
    let mut reconciler = fixture.reconciler();
    let prepared = reconciler
        .prepare_promotion(&fixture.request())
        .expect("record promotion intent");
    assert_eq!(
        reconciler
            .store()
            .promotion_intent(&prepared.intent_id)
            .unwrap()
            .unwrap()["state"],
        "prepared"
    );
    drop(reconciler);

    let mut restarted = fixture.reconciler();
    let recovery = restarted
        .recover_promotion(&prepared.intent_id)
        .expect("recover prepared intent");
    assert_eq!(recovery.action, PromotionRecoveryAction::Rollback);
    assert_eq!(recovery.intent["state"], "aborted");
    assert_eq!(fixture.canonical_oid(), fixture.base_oid);
}

#[test]
fn kill_after_ref_update_recovers_by_completing_forward() {
    let fixture = fixture();
    let mut reconciler = fixture.reconciler();
    let prepared = reconciler
        .prepare_promotion(&fixture.request())
        .expect("record promotion intent");
    reconciler
        .compare_and_swap_canonical_ref(&prepared)
        .expect("CAS canonical ref");
    assert_eq!(fixture.canonical_oid(), fixture.target_oid);
    drop(reconciler);

    let mut restarted = fixture.reconciler();
    let recovery = restarted
        .resume_promotion(&prepared.intent_id)
        .expect("recover ref-updated intent");
    assert_eq!(recovery.action, PromotionRecoveryAction::Finalize);
    assert_eq!(recovery.intent["state"], "finalized");
    assert_eq!(
        restarted
            .store()
            .candidate(&fixture.candidate_id)
            .unwrap()
            .unwrap()["state"],
        "promoted"
    );
    assert_eq!(fixture.canonical_oid(), fixture.target_oid);
}

#[test]
fn stale_cas_can_be_rolled_back_without_touching_foreign_canonical_commit() {
    let fixture = fixture();
    let mut reconciler = fixture.reconciler();
    let prepared = reconciler
        .prepare_promotion(&fixture.request())
        .expect("record promotion intent");
    let foreign_oid = foreign_commit(&fixture.repository, &fixture.base_oid);
    update_ref(&fixture.repository, CANONICAL_REF, &foreign_oid);

    let error = reconciler
        .compare_and_swap_canonical_ref(&prepared)
        .expect_err("CAS must reject the foreign canonical ref");
    assert!(matches!(
        error,
        PromotionSagaError::StaleCanonicalBase { .. }
    ));
    reconciler
        .rollback_promotion(&prepared, "stale canonical base")
        .expect("abort prepared intent");
    assert_eq!(
        reconciler
            .store()
            .promotion_intent(&prepared.intent_id)
            .unwrap()
            .unwrap()["state"],
        "aborted"
    );
    assert_eq!(fixture.canonical_oid(), foreign_oid);
}

#[test]
fn foreign_ref_recovery_marks_conflict_and_aborts_intent() {
    let fixture = fixture();
    let mut reconciler = fixture.reconciler();
    let prepared = reconciler
        .prepare_promotion(&fixture.request())
        .expect("record promotion intent");
    let foreign_oid = foreign_commit(&fixture.repository, &fixture.base_oid);
    update_ref(&fixture.repository, CANONICAL_REF, &foreign_oid);

    let recovery = reconciler
        .recover_promotion(&prepared.intent_id)
        .expect("recover foreign ref");
    assert_eq!(recovery.action, PromotionRecoveryAction::Conflict);
    assert_eq!(recovery.intent["state"], "aborted");
    assert_eq!(fixture.canonical_oid(), foreign_oid);
}

#[test]
fn only_promote_decisions_can_be_deserialized_at_boundary() {
    let decision: PromotionDecision = serde_json::from_value(json!({
        "outcome": "promote",
        "candidateId": "candidate"
    }))
    .expect("promote decision");
    assert_eq!(decision.candidate_id(), "candidate");
    assert!(
        serde_json::from_value::<PromotionDecision>(json!({
            "outcome": "select",
            "candidateId": "candidate"
        }))
        .is_err()
    );
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

fn update_ref(repository: &Path, ref_name: &str, oid: &str) {
    git(repository, &["update-ref", ref_name, oid]);
}

fn foreign_commit(repository: &Path, parent: &str) -> String {
    let tree = git(repository, &["rev-parse", &format!("{parent}^{{tree}}")]);
    let output = Command::new("git")
        .current_dir(repository)
        .args(["commit-tree", &tree, "-p", parent, "-m", "foreign"])
        .output()
        .expect("spawn git commit-tree");
    assert!(
        output.status.success(),
        "git commit-tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("commit OID is UTF-8")
        .trim()
        .to_owned()
}

#[allow(dead_code)]
fn assert_prepared(prepared: &PreparedPromotion) {
    assert_eq!(prepared.verified.canonical_ref, CANONICAL_REF);
}
