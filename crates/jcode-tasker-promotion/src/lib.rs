//! Recoverable, single-writer promotion of an adjudicated Tasker candidate.
//!
//! SQLite and Git cannot share one transaction.  [`PromotionReconciler`]
//! therefore makes the boundary explicit: it verifies the winning decision,
//! observes and validates the candidate objects and canonical base, records a
//! durable intent, performs a Git compare-and-swap, and only then finalizes the
//! Tasker state.  Recovery observes the canonical ref and either finalizes an
//! already-applied update or aborts an intent whose ref never moved.  A foreign
//! canonical commit is never overwritten.

use anyhow::Error as AnyhowError;
use jcode_tasker_git::{CandidateRef, GitCandidateAdapter};
use jcode_tasker_pi::{ConcurrencyStore, ConcurrencyStoreError, PromotionRecoveryAction};
use jcode_tasker_types::{CandidateId, CandidateSetId, PromotionIntentId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::Path, process::Command};
use thiserror::Error;

pub type PromotionResult<T> = Result<T, PromotionSagaError>;

/// The only decision outcome accepted by the promotion boundary.
///
/// Adjudication may produce other outcomes, but canonical mutation only
/// accepts an explicit promote decision.  This keeps the promotion boundary
/// distinct from adjudication quorum calculation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PromotionDecision {
    Promote {
        #[serde(rename = "candidateId", alias = "candidate_id")]
        candidate_id: String,
    },
}

impl PromotionDecision {
    /// Construct an explicit promote decision for one candidate.
    pub fn promote(candidate_id: impl Into<String>) -> Self {
        Self::Promote {
            candidate_id: candidate_id.into(),
        }
    }

    /// Return the candidate referenced by the decision.
    pub fn candidate_id(&self) -> &str {
        match self {
            Self::Promote { candidate_id } => candidate_id,
        }
    }
}

/// Alias matching the language used by adjudication callers.
pub type PromotionOutcome = PromotionDecision;

/// Inputs captured at the promotion boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionRequest {
    pub project_id: String,
    pub task_id: String,
    pub candidate_set_id: String,
    pub decision: PromotionDecision,
    pub canonical_ref: String,
    pub expected_revision: u64,
    #[serde(default)]
    pub intent_id: Option<String>,
}

impl PromotionRequest {
    /// Build a request from an adjudicated promotion decision.
    pub fn new(
        project_id: impl Into<String>,
        task_id: impl Into<String>,
        candidate_set_id: impl Into<String>,
        decision: PromotionDecision,
        canonical_ref: impl Into<String>,
        expected_revision: u64,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            task_id: task_id.into(),
            candidate_set_id: candidate_set_id.into(),
            decision,
            canonical_ref: canonical_ref.into(),
            expected_revision,
            intent_id: None,
        }
    }

    /// Build the common explicit-promote form.
    pub fn promote(
        project_id: impl Into<String>,
        task_id: impl Into<String>,
        candidate_set_id: impl Into<String>,
        candidate_id: impl Into<String>,
        canonical_ref: impl Into<String>,
        expected_revision: u64,
    ) -> Self {
        Self::new(
            project_id,
            task_id,
            candidate_set_id,
            PromotionDecision::promote(candidate_id),
            canonical_ref,
            expected_revision,
        )
    }

    /// Pin a caller-provided promotion intent ID for deterministic retries.
    pub fn with_intent_id(mut self, intent_id: impl Into<String>) -> Self {
        self.intent_id = Some(intent_id.into());
        self
    }
}

/// The verified, immutable inputs for the Git and persistence phases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPromotion {
    pub project_id: String,
    pub task_id: String,
    pub candidate_set_id: String,
    pub candidate_id: String,
    pub candidate_ref: CandidateRef,
    pub candidate_state: String,
    pub candidate_base_commit: String,
    pub expected_commit: String,
    pub target_commit: String,
    pub canonical_ref: String,
    pub expected_revision: u64,
}

/// A promotion intent after the durable `prepared` transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedPromotion {
    pub intent_id: String,
    pub verified: VerifiedPromotion,
    /// Tasker revision after the intent was recorded.
    pub revision: u64,
}

impl PreparedPromotion {
    /// The project associated with this intent.
    pub fn project_id(&self) -> &str {
        &self.verified.project_id
    }

    /// The expected canonical base OID.
    pub fn expected_commit(&self) -> &str {
        &self.verified.expected_commit
    }

    /// The candidate tip OID that will become canonical.
    pub fn target_commit(&self) -> &str {
        &self.verified.target_commit
    }
}

/// A successful finalized promotion receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionReceipt {
    pub intent_id: String,
    pub target_commit: String,
    pub revision: u64,
}

/// Result of reconciling a persisted intent after a process restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionRecoveryResult {
    pub intent_id: String,
    pub observed_commit: Option<String>,
    pub action: PromotionRecoveryAction,
    pub intent: Value,
    pub revision: u64,
}

/// Errors emitted by the promotion boundary.
#[derive(Debug, Error)]
pub enum PromotionSagaError {
    #[error("promotion decision must reference a promote outcome")]
    InvalidDecision,
    #[error("promotion decision is missing a candidate ID")]
    MissingCandidateId,
    #[error("candidate set {candidate_set_id} was not found")]
    CandidateSetNotFound { candidate_set_id: String },
    #[error("candidate {candidate_id} was not found")]
    CandidateNotFound { candidate_id: String },
    #[error("promotion metadata is missing {entity}.{field}")]
    MissingMetadata { entity: String, field: String },
    #[error("candidate {candidate_id} does not belong to candidate set {candidate_set_id}")]
    CandidateSetMismatch {
        candidate_id: String,
        candidate_set_id: String,
    },
    #[error("candidate set {candidate_set_id} belongs to project {actual}, not {expected}")]
    ProjectMismatch {
        candidate_set_id: String,
        expected: String,
        actual: String,
    },
    #[error("candidate set {candidate_set_id} belongs to task {actual}, not {expected}")]
    TaskMismatch {
        candidate_set_id: String,
        expected: String,
        actual: String,
    },
    #[error("candidate {candidate_id} is not promotable in state {state}")]
    CandidateNotPromotable { candidate_id: String, state: String },
    #[error(
        "candidate {candidate_id} base commit {candidate_base} does not match candidate-set base {set_base}"
    )]
    CandidateBaseMismatch {
        candidate_id: String,
        candidate_base: String,
        set_base: String,
    },
    #[error("candidate {candidate_id} Git objects do not match persisted metadata: {reason}")]
    CandidateGitMismatch {
        candidate_id: String,
        reason: String,
    },
    #[error("candidate identity {value} is not valid for Git: {reason}")]
    InvalidCandidateIdentity { value: String, reason: String },
    #[error("canonical ref {canonical_ref} is stale: expected {expected}, observed {observed}")]
    StaleCanonicalBase {
        canonical_ref: String,
        expected: String,
        observed: String,
    },
    #[error("Tasker revision is stale: expected {expected}, actual {actual}")]
    StaleTaskerRevision { expected: u64, actual: u64 },
    #[error("canonical ref {canonical_ref} already moved to {observed}; refusing rollback")]
    CanonicalRefAlreadyMoved {
        canonical_ref: String,
        observed: String,
    },
    #[error("failed to observe canonical ref {canonical_ref}: {message}")]
    CanonicalObservation {
        canonical_ref: String,
        message: String,
    },
    #[error("rollback of promotion intent {intent_id} failed: {source}")]
    RollbackFailed {
        intent_id: String,
        #[source]
        source: ConcurrencyStoreError,
    },
    #[error("Tasker concurrency error: {0}")]
    Store(#[from] ConcurrencyStoreError),
    #[error("Git adapter error: {0}")]
    Git(#[from] AnyhowError),
}

/// The sole canonical writer for candidate promotion.
#[derive(Debug)]
pub struct PromotionReconciler {
    store: ConcurrencyStore,
    git: GitCandidateAdapter,
}

/// Semantic alias for callers that name the workflow rather than its role.
pub type PromotionSaga = PromotionReconciler;

impl PromotionReconciler {
    /// Compose the existing persistence and Git layers.
    pub fn new(store: ConcurrencyStore, git: GitCandidateAdapter) -> Self {
        Self { store, git }
    }

    /// Open a reconciler over a SQLite path and Git repository.
    pub fn open(
        db_path: impl Into<std::path::PathBuf>,
        project_root: impl Into<String>,
        repo_path: impl AsRef<Path>,
    ) -> PromotionResult<Self> {
        let store = ConcurrencyStore::open_path(db_path, project_root)
            .map_err(PromotionSagaError::Store)?;
        let git = GitCandidateAdapter::try_new(repo_path).map_err(PromotionSagaError::Git)?;
        Ok(Self::new(store, git))
    }

    /// Borrow the composed concurrency store.
    pub fn store(&self) -> &ConcurrencyStore {
        &self.store
    }

    /// Mutably borrow the composed concurrency store for phase-level callers.
    pub fn store_mut(&mut self) -> &mut ConcurrencyStore {
        &mut self.store
    }

    /// Borrow the Git adapter.
    pub fn git(&self) -> &GitCandidateAdapter {
        &self.git
    }

    /// Verify the decision and persisted candidate metadata before Git access.
    pub fn verify(&self, request: &PromotionRequest) -> PromotionResult<VerifiedPromotion> {
        if request.canonical_ref.trim().is_empty() {
            return Err(PromotionSagaError::CanonicalObservation {
                canonical_ref: request.canonical_ref.clone(),
                message: "canonical ref must not be empty".into(),
            });
        }
        let candidate_id = request.decision.candidate_id().trim();
        if candidate_id.is_empty() {
            return Err(PromotionSagaError::MissingCandidateId);
        }

        let actual_revision = self
            .store
            .current_revision(&request.project_id)
            .map_err(PromotionSagaError::Store)?;
        if actual_revision != request.expected_revision {
            return Err(PromotionSagaError::StaleTaskerRevision {
                expected: request.expected_revision,
                actual: actual_revision,
            });
        }

        let candidate_set = self
            .store
            .candidate_set(&request.candidate_set_id)
            .map_err(PromotionSagaError::Store)?
            .ok_or_else(|| PromotionSagaError::CandidateSetNotFound {
                candidate_set_id: request.candidate_set_id.clone(),
            })?;
        let candidate = self
            .store
            .candidate(candidate_id)
            .map_err(PromotionSagaError::Store)?
            .ok_or_else(|| PromotionSagaError::CandidateNotFound {
                candidate_id: candidate_id.to_owned(),
            })?;

        let set_project = metadata_string(&candidate_set, "candidateSet", "projectId")?;
        if set_project != request.project_id {
            return Err(PromotionSagaError::ProjectMismatch {
                candidate_set_id: request.candidate_set_id.clone(),
                expected: request.project_id.clone(),
                actual: set_project,
            });
        }
        let set_task = metadata_string(&candidate_set, "candidateSet", "taskId")?;
        if set_task != request.task_id {
            return Err(PromotionSagaError::TaskMismatch {
                candidate_set_id: request.candidate_set_id.clone(),
                expected: request.task_id.clone(),
                actual: set_task,
            });
        }
        let candidate_set_reference = metadata_string(&candidate, "candidate", "candidateSetId")?;
        if candidate_set_reference != request.candidate_set_id {
            return Err(PromotionSagaError::CandidateSetMismatch {
                candidate_id: candidate_id.to_owned(),
                candidate_set_id: request.candidate_set_id.clone(),
            });
        }

        let candidate_state = metadata_string(&candidate, "candidate", "state")?;
        if !matches!(
            candidate_state.as_str(),
            "eligible" | "selected" | "promoted"
        ) {
            return Err(PromotionSagaError::CandidateNotPromotable {
                candidate_id: candidate_id.to_owned(),
                state: candidate_state,
            });
        }
        let candidate_base_commit = metadata_string(&candidate, "candidate", "baseCommit")?;
        let expected_commit = metadata_string(&candidate_set, "candidateSet", "baseCommit")?;
        if candidate_base_commit != expected_commit {
            return Err(PromotionSagaError::CandidateBaseMismatch {
                candidate_id: candidate_id.to_owned(),
                candidate_base: candidate_base_commit,
                set_base: expected_commit,
            });
        }
        let target_commit = metadata_string(&candidate, "candidate", "resultCommit")?;

        let candidate_set_identity =
            request
                .candidate_set_id
                .parse::<CandidateSetId>()
                .map_err(|error| PromotionSagaError::InvalidCandidateIdentity {
                    value: request.candidate_set_id.clone(),
                    reason: error.to_string(),
                })?;
        let candidate_identity = candidate_id.parse::<CandidateId>().map_err(|error| {
            PromotionSagaError::InvalidCandidateIdentity {
                value: candidate_id.to_owned(),
                reason: error.to_string(),
            }
        })?;

        Ok(VerifiedPromotion {
            project_id: request.project_id.clone(),
            task_id: request.task_id.clone(),
            candidate_set_id: request.candidate_set_id.clone(),
            candidate_id: candidate_id.to_owned(),
            candidate_ref: CandidateRef::new(candidate_set_identity, candidate_identity),
            candidate_state,
            candidate_base_commit,
            expected_commit,
            target_commit,
            canonical_ref: request.canonical_ref.clone(),
            expected_revision: request.expected_revision,
        })
    }

    /// Verify candidate objects and capture the current canonical base.
    pub fn prepare_git_objects(
        &self,
        request: &PromotionRequest,
    ) -> PromotionResult<VerifiedPromotion> {
        let verified = self.verify(request)?;
        let metadata = self
            .git
            .read_candidate_metadata(&verified.candidate_ref)
            .map_err(PromotionSagaError::Git)?;
        if metadata.base_oid != verified.candidate_base_commit {
            return Err(PromotionSagaError::CandidateGitMismatch {
                candidate_id: verified.candidate_id.clone(),
                reason: format!(
                    "Git base {} differs from persisted base {}",
                    metadata.base_oid, verified.candidate_base_commit
                ),
            });
        }
        if metadata.tip_oid != verified.target_commit {
            return Err(PromotionSagaError::CandidateGitMismatch {
                candidate_id: verified.candidate_id.clone(),
                reason: format!(
                    "Git tip {} differs from persisted result {}",
                    metadata.tip_oid, verified.target_commit
                ),
            });
        }

        let observed = self.observe_canonical_ref(&verified.canonical_ref)?;
        if observed.as_deref() != Some(verified.expected_commit.as_str()) {
            return Err(stale_canonical_error(
                &verified.canonical_ref,
                &verified.expected_commit,
                observed,
            ));
        }
        Ok(verified)
    }

    /// Record the durable `prepared` intent after Git preparation succeeds.
    pub fn record_intent(
        &mut self,
        verified: VerifiedPromotion,
        intent_id: impl Into<String>,
    ) -> PromotionResult<PreparedPromotion> {
        let intent_id = intent_id.into();
        if intent_id.trim().is_empty() {
            return Err(PromotionSagaError::MissingMetadata {
                entity: "promotion intent".into(),
                field: "id".into(),
            });
        }
        let intent = json!({
            "id": intent_id,
            "projectId": verified.project_id,
            "taskId": verified.task_id,
            "candidateSetId": verified.candidate_set_id,
            "candidateId": verified.candidate_id,
            "expectedRevision": verified.expected_revision,
            "expectedCommit": verified.expected_commit,
            "targetCommit": verified.target_commit,
            "canonicalRef": verified.canonical_ref,
            "state": "prepared",
        });
        let mutation = self
            .store
            .prepare_promotion(intent, verified.expected_revision)
            .map_err(|error| self.map_store_error(error))?;
        Ok(PreparedPromotion {
            intent_id: mutation.id,
            verified,
            revision: mutation.revision,
        })
    }

    /// Execute the first three saga phases and return the persisted intent.
    pub fn prepare_promotion(
        &mut self,
        request: &PromotionRequest,
    ) -> PromotionResult<PreparedPromotion> {
        let verified = self.prepare_git_objects(request)?;
        let intent_id = request
            .intent_id
            .clone()
            .unwrap_or_else(|| PromotionIntentId::new().to_string());
        self.record_intent(verified, intent_id)
    }

    /// CAS-update the canonical ref for a prepared intent.
    pub fn compare_and_swap_canonical_ref(
        &self,
        prepared: &PreparedPromotion,
    ) -> PromotionResult<()> {
        let observed = self.observe_canonical_ref(&prepared.verified.canonical_ref)?;
        if observed.as_deref() != Some(prepared.expected_commit()) {
            return Err(stale_canonical_error(
                &prepared.verified.canonical_ref,
                prepared.expected_commit(),
                observed,
            ));
        }
        self.git
            .compare_and_swap_ref(
                &prepared.verified.canonical_ref,
                prepared.expected_commit(),
                prepared.target_commit(),
            )
            .map_err(PromotionSagaError::Git)
    }

    /// Alias for callers that use the phase name from the proposal.
    pub fn update_canonical_ref(&self, prepared: &PreparedPromotion) -> PromotionResult<()> {
        self.compare_and_swap_canonical_ref(prepared)
    }

    /// Mark the Git phase durable after observing a successful CAS.
    pub fn mark_ref_updated(
        &mut self,
        prepared: &PreparedPromotion,
    ) -> PromotionResult<PromotionReceipt> {
        let revision = self
            .store
            .current_revision(prepared.project_id())
            .map_err(PromotionSagaError::Store)?;
        let mutation = self
            .store
            .mark_promotion_ref_updated(&prepared.intent_id, prepared.target_commit(), revision)
            .map_err(|error| self.map_store_error(error))?;
        Ok(PromotionReceipt {
            intent_id: mutation.id,
            target_commit: prepared.target_commit().to_owned(),
            revision: mutation.revision,
        })
    }

    /// Finalize a ref-updated intent.
    pub fn finalize_promotion(
        &mut self,
        prepared: &PreparedPromotion,
    ) -> PromotionResult<PromotionReceipt> {
        let revision = self
            .store
            .current_revision(prepared.project_id())
            .map_err(PromotionSagaError::Store)?;
        let mutation = self
            .store
            .finalize_promotion(&prepared.intent_id, revision)
            .map_err(|error| self.map_store_error(error))?;
        Ok(PromotionReceipt {
            intent_id: mutation.id,
            target_commit: prepared.target_commit().to_owned(),
            revision: mutation.revision,
        })
    }

    /// Abort a prepared intent only when the canonical ref did not move to its target.
    pub fn rollback_promotion(
        &mut self,
        prepared: &PreparedPromotion,
        reason: &str,
    ) -> PromotionResult<()> {
        let observed = self.observe_canonical_ref(&prepared.verified.canonical_ref)?;
        if observed.as_deref() == Some(prepared.target_commit()) {
            return Err(PromotionSagaError::CanonicalRefAlreadyMoved {
                canonical_ref: prepared.verified.canonical_ref.clone(),
                observed: prepared.target_commit().to_owned(),
            });
        }
        let revision = self
            .store
            .current_revision(prepared.project_id())
            .map_err(PromotionSagaError::Store)?;
        self.store
            .rollback_promotion(&prepared.intent_id, reason, revision)
            .map_err(|source| PromotionSagaError::RollbackFailed {
                intent_id: prepared.intent_id.clone(),
                source,
            })?;
        Ok(())
    }

    /// Run all saga phases.  After CAS, errors are returned for recovery rather than
    /// rolling back a canonical ref that may already have moved.
    pub fn execute_promotion(
        &mut self,
        request: &PromotionRequest,
    ) -> PromotionResult<PromotionReceipt> {
        let prepared = self.prepare_promotion(request)?;
        if let Err(error) = self.compare_and_swap_canonical_ref(&prepared) {
            let observed = self.observe_canonical_ref(&prepared.verified.canonical_ref)?;
            if observed.as_deref() != Some(prepared.target_commit()) {
                self.rollback_after_failed_cas(&prepared, &error)?;
            }
            return Err(error);
        }
        self.mark_ref_updated(&prepared)?;
        self.finalize_promotion(&prepared)
    }

    /// Semantic alias for the full promotion operation.
    pub fn promote(&mut self, request: &PromotionRequest) -> PromotionResult<PromotionReceipt> {
        self.execute_promotion(request)
    }

    /// Reconcile one durable intent after restart.
    pub fn recover_promotion(
        &mut self,
        intent_id: &str,
    ) -> PromotionResult<PromotionRecoveryResult> {
        let intent = self
            .store
            .promotion_intent(intent_id)
            .map_err(PromotionSagaError::Store)?
            .ok_or_else(|| PromotionSagaError::MissingMetadata {
                entity: format!("promotion intent {intent_id}"),
                field: "record".into(),
            })?;
        let canonical_ref = metadata_string(&intent, "promotion intent", "canonicalRef")?;
        let project_id = metadata_string(&intent, "promotion intent", "projectId")?;
        let observed_commit = self.observe_canonical_ref(&canonical_ref)?;
        let observed_for_store = observed_commit.clone().unwrap_or_default();
        let expected_revision = self
            .store
            .current_revision(&project_id)
            .map_err(PromotionSagaError::Store)?;
        let recovery = self
            .store
            .recover_promotion(intent_id, &observed_for_store, expected_revision)
            .map_err(|error| self.map_store_error(error))?;

        let action = match recovery.action {
            PromotionRecoveryAction::Retry => {
                let rollback_revision = self
                    .store
                    .current_revision(&project_id)
                    .map_err(PromotionSagaError::Store)?;
                self.store
                    .rollback_promotion(
                        intent_id,
                        "canonical ref remained at the expected base during recovery",
                        rollback_revision,
                    )
                    .map_err(|source| PromotionSagaError::RollbackFailed {
                        intent_id: intent_id.to_owned(),
                        source,
                    })?;
                PromotionRecoveryAction::Rollback
            }
            PromotionRecoveryAction::Conflict => {
                let rollback_revision = self
                    .store
                    .current_revision(&project_id)
                    .map_err(PromotionSagaError::Store)?;
                self.store
                    .rollback_promotion(
                        intent_id,
                        "foreign canonical ref observed during recovery",
                        rollback_revision,
                    )
                    .map_err(|source| PromotionSagaError::RollbackFailed {
                        intent_id: intent_id.to_owned(),
                        source,
                    })?;
                PromotionRecoveryAction::Conflict
            }
            action => action,
        };
        let intent = self
            .store
            .promotion_intent(intent_id)
            .map_err(PromotionSagaError::Store)?
            .ok_or_else(|| PromotionSagaError::MissingMetadata {
                entity: format!("promotion intent {intent_id}"),
                field: "record after recovery".into(),
            })?;
        let revision = self
            .store
            .current_revision(&project_id)
            .map_err(PromotionSagaError::Store)?;
        Ok(PromotionRecoveryResult {
            intent_id: intent_id.to_owned(),
            observed_commit,
            action,
            intent,
            revision,
        })
    }

    /// Alias for restart/resume callers.
    pub fn resume_promotion(
        &mut self,
        intent_id: &str,
    ) -> PromotionResult<PromotionRecoveryResult> {
        self.recover_promotion(intent_id)
    }

    /// Reconcile all incomplete intents for one project in store order.
    pub fn recover_incomplete(
        &mut self,
        project_id: &str,
        limit: usize,
    ) -> PromotionResult<Vec<PromotionRecoveryResult>> {
        let intents = self
            .store
            .incomplete_promotions(project_id, limit)
            .map_err(PromotionSagaError::Store)?;
        intents
            .iter()
            .map(|intent| {
                let intent_id = metadata_string(intent, "promotion intent", "id")?;
                self.recover_promotion(&intent_id)
            })
            .collect()
    }

    fn rollback_after_failed_cas(
        &mut self,
        prepared: &PreparedPromotion,
        error: &PromotionSagaError,
    ) -> PromotionResult<()> {
        let reason = format!("canonical compare-and-swap failed: {error}");
        let revision = self
            .store
            .current_revision(prepared.project_id())
            .map_err(PromotionSagaError::Store)?;
        self.store
            .rollback_promotion(&prepared.intent_id, &reason, revision)
            .map_err(|source| PromotionSagaError::RollbackFailed {
                intent_id: prepared.intent_id.clone(),
                source,
            })?;
        Ok(())
    }

    fn map_store_error(&self, error: ConcurrencyStoreError) -> PromotionSagaError {
        match error {
            ConcurrencyStoreError::RevisionConflict {
                expected, actual, ..
            } => PromotionSagaError::StaleTaskerRevision { expected, actual },
            other => PromotionSagaError::Store(other),
        }
    }

    fn observe_canonical_ref(&self, ref_name: &str) -> PromotionResult<Option<String>> {
        observe_ref(self.git.repo_path(), ref_name)
    }
}

fn metadata_string(value: &Value, entity: &str, field: &str) -> PromotionResult<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| PromotionSagaError::MissingMetadata {
            entity: entity.to_owned(),
            field: field.to_owned(),
        })
}

fn stale_canonical_error(
    canonical_ref: &str,
    expected: &str,
    observed: Option<String>,
) -> PromotionSagaError {
    PromotionSagaError::StaleCanonicalBase {
        canonical_ref: canonical_ref.to_owned(),
        expected: expected.to_owned(),
        observed: observed.unwrap_or_else(|| "<missing>".into()),
    }
}

fn observe_ref(repo_path: &Path, ref_name: &str) -> PromotionResult<Option<String>> {
    if !ref_name.starts_with("refs/") {
        return Err(PromotionSagaError::CanonicalObservation {
            canonical_ref: ref_name.to_owned(),
            message: "Git refs must be fully qualified below refs/".into(),
        });
    }
    let check = Command::new("git")
        .current_dir(repo_path)
        .args(["check-ref-format", ref_name])
        .output()
        .map_err(|error| canonical_observation_error(ref_name, error))?;
    if !check.status.success() {
        return Err(PromotionSagaError::CanonicalObservation {
            canonical_ref: ref_name.to_owned(),
            message: String::from_utf8_lossy(&check.stderr).trim().to_owned(),
        });
    }

    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["show-ref", "--hash", "--verify", ref_name])
        .output()
        .map_err(|error| canonical_observation_error(ref_name, error))?;
    if output.status.success() {
        let oid = String::from_utf8(output.stdout).map_err(|error| {
            PromotionSagaError::CanonicalObservation {
                canonical_ref: ref_name.to_owned(),
                message: format!("Git returned non-UTF-8 ref output: {error}"),
            }
        })?;
        let oid = oid.trim();
        if oid.is_empty() {
            return Err(PromotionSagaError::CanonicalObservation {
                canonical_ref: ref_name.to_owned(),
                message: "Git returned an empty ref OID".into(),
            });
        }
        return Ok(Some(oid.to_owned()));
    }
    if output.status.code() == Some(1) && output.stdout.is_empty() && output.stderr.is_empty() {
        return Ok(None);
    }
    Err(PromotionSagaError::CanonicalObservation {
        canonical_ref: ref_name.to_owned(),
        message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

fn canonical_observation_error(ref_name: &str, error: std::io::Error) -> PromotionSagaError {
    PromotionSagaError::CanonicalObservation {
        canonical_ref: ref_name.to_owned(),
        message: error.to_string(),
    }
}
