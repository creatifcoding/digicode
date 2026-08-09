use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    PiTaskerStore, ProjectPartition, legacy_native_project_id_for_list,
    native_project_id_for_partition,
};

const MAX_PROJECTION_LIMIT: usize = 500;
const CONCURRENCY_SCHEMA_VERSION: i64 = 1;

const MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS concurrency_store_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS concurrency_project_revisions (
    project_id TEXT PRIMARY KEY NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS candidate_sets (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    base_revision INTEGER NOT NULL CHECK (base_revision >= 0),
    base_commit TEXT NOT NULL,
    acceptance_digest TEXT NOT NULL,
    policy_json TEXT NOT NULL CHECK (json_valid(policy_json)),
    policy_version INTEGER NOT NULL CHECK (policy_version >= 0),
    state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS candidates (
    id TEXT PRIMARY KEY NOT NULL,
    candidate_set_id TEXT NOT NULL,
    state TEXT NOT NULL,
    base_commit TEXT NOT NULL,
    result_commit TEXT,
    diff_digest TEXT,
    summary TEXT,
    provenance_json TEXT NOT NULL CHECK (json_valid(provenance_json)),
    supersedes_candidate_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    submitted_at TEXT
);

CREATE TABLE IF NOT EXISTS candidate_resource_intents (
    candidate_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    intent_json TEXT NOT NULL CHECK (json_valid(intent_json)),
    PRIMARY KEY (candidate_id, ordinal)
);

CREATE TABLE IF NOT EXISTS candidate_evidence (
    candidate_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    evidence_json TEXT NOT NULL CHECK (json_valid(evidence_json)),
    PRIMARY KEY (candidate_id, ordinal)
);

CREATE TABLE IF NOT EXISTS adjudication_rounds (
    id TEXT PRIMARY KEY NOT NULL,
    candidate_set_id TEXT NOT NULL,
    policy_version INTEGER NOT NULL CHECK (policy_version >= 0),
    required_quorum INTEGER NOT NULL CHECK (required_quorum > 0),
    decision_json TEXT CHECK (decision_json IS NULL OR json_valid(decision_json)),
    created_at TEXT NOT NULL,
    decided_at TEXT
);

CREATE TABLE IF NOT EXISTS adjudication_ballots (
    id TEXT PRIMARY KEY NOT NULL,
    round_id TEXT NOT NULL,
    validator_json TEXT NOT NULL CHECK (json_valid(validator_json)),
    assessments_json TEXT NOT NULL CHECK (json_valid(assessments_json)),
    ranking_json TEXT NOT NULL CHECK (json_valid(ranking_json)),
    abstained INTEGER NOT NULL CHECK (abstained IN (0, 1)),
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS promotion_intents (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    candidate_set_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    expected_revision INTEGER NOT NULL CHECK (expected_revision >= 0),
    expected_commit TEXT NOT NULL,
    target_commit TEXT NOT NULL,
    canonical_ref TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    finalized_at TEXT,
    conflict_reason TEXT
);

CREATE TABLE IF NOT EXISTS promotion_events (
    id TEXT PRIMARY KEY NOT NULL,
    intent_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    from_state TEXT,
    to_state TEXT NOT NULL,
    event_kind TEXT NOT NULL,
    details_json TEXT NOT NULL CHECK (json_valid(details_json)),
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS candidate_sets_by_project
    ON candidate_sets(project_id, created_at, id);
CREATE INDEX IF NOT EXISTS candidates_by_set
    ON candidates(candidate_set_id, created_at, id);
CREATE INDEX IF NOT EXISTS rounds_by_set
    ON adjudication_rounds(candidate_set_id, created_at, id);
CREATE INDEX IF NOT EXISTS ballots_by_round
    ON adjudication_ballots(round_id, created_at, id);
CREATE INDEX IF NOT EXISTS promotions_by_project
    ON promotion_intents(project_id, updated_at, id);
CREATE UNIQUE INDEX IF NOT EXISTS one_active_promotion_per_project
    ON promotion_intents(project_id)
    WHERE state IN ('prepared', 'ref_updated');
"#;

pub type ConcurrencyResult<T> = Result<T, ConcurrencyStoreError>;

#[derive(Debug, Error)]
pub enum ConcurrencyStoreError {
    #[error("concurrency SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("concurrency JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("concurrency schema error: {0}")]
    Schema(String),
    #[error("invalid concurrency input for {field}: {message}")]
    InvalidInput { field: String, message: String },
    #[error("concurrency record not found: {entity} {id}")]
    NotFound { entity: String, id: String },
    #[error(
        "concurrency revision conflict for project {project_id}: expected {expected}, actual {actual}"
    )]
    RevisionConflict {
        project_id: String,
        expected: u64,
        actual: u64,
    },
    #[error("candidate {candidate_id} is immutable in state {state}")]
    ImmutableCandidate { candidate_id: String, state: String },
    #[error("promotion conflict for project {project_id}: {message}")]
    PromotionConflict { project_id: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConcurrencyMutation {
    pub id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConcurrencyProjection {
    pub value: Value,
    pub limit: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotionRecoveryAction {
    Retry,
    Finalize,
    Rollback,
    Conflict,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromotionRecovery {
    pub intent: Value,
    pub action: PromotionRecoveryAction,
    pub revision: u64,
}

#[derive(Debug)]
pub struct ConcurrencyStore {
    conn: Connection,
    partition: ProjectPartition,
}

impl ConcurrencyStore {
    /// Open the additive concurrency tables in the same database as a Pi store.
    ///
    /// The migration intentionally uses its own metadata table rather than
    /// SQLite `user_version`, which belongs to the legacy Pi schema. Candidate
    /// source content is never stored here; only bounded metadata and refs are.
    pub fn open(partition: ProjectPartition) -> ConcurrencyResult<Self> {
        let mut conn = Connection::open(&partition.db_path)?;
        configure_and_migrate(&mut conn)?;
        migrate_legacy_project_identity(&mut conn, &partition)?;
        Ok(Self { conn, partition })
    }

    pub fn open_path(
        path: impl Into<PathBuf>,
        project_root: impl Into<String>,
    ) -> ConcurrencyResult<Self> {
        Self::open(ProjectPartition::with_db_path(path, project_root))
    }

    pub fn open_in_memory(project_root: impl Into<String>) -> ConcurrencyResult<Self> {
        let partition = ProjectPartition::with_db_path(":memory:", project_root);
        let mut conn = Connection::open_in_memory()?;
        configure_and_migrate(&mut conn)?;
        Ok(Self { conn, partition })
    }

    /// Create a transactionally consistent in-memory copy of the concurrency
    /// database. Plan-mode Tasker reconciliation uses this to exercise the
    /// same lifecycle code paths without writing candidate or promotion
    /// metadata to the canonical database.
    pub fn fork_in_memory(&self) -> ConcurrencyResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        {
            let backup = rusqlite::backup::Backup::new(&self.conn, &mut conn)?;
            backup.run_to_completion(128, std::time::Duration::from_millis(1), None)?;
        }
        conn.busy_timeout(std::time::Duration::from_secs(30))?;
        Ok(Self {
            conn,
            partition: ProjectPartition::with_db_path(
                ":memory:",
                self.partition.project_root.clone(),
            ),
        })
    }

    pub fn open_for(store: &PiTaskerStore) -> ConcurrencyResult<Self> {
        Self::open(store.partition().clone())
    }

    pub fn partition(&self) -> &ProjectPartition {
        &self.partition
    }

    pub fn schema_version(&self) -> ConcurrencyResult<i64> {
        let version = self.conn.query_row(
            "SELECT value FROM concurrency_store_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )?;
        version.parse::<i64>().map_err(|error| {
            ConcurrencyStoreError::Schema(format!(
                "invalid concurrency schema version {version}: {error}"
            ))
        })
    }

    pub fn current_revision(&self, project_id: &str) -> ConcurrencyResult<u64> {
        let revision = self
            .conn
            .query_row(
                "SELECT revision FROM concurrency_project_revisions WHERE project_id = ?1",
                [project_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);
        from_i64(revision, "project revision")
    }

    pub fn create_candidate_set<T: Serialize>(
        &mut self,
        candidate_set: T,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        let data = parse_candidate_set(serde_json::to_value(candidate_set)?)?;
        if data.base_revision != expected_revision {
            return Err(ConcurrencyStoreError::InvalidInput {
                field: "base_revision".into(),
                message: format!(
                    "candidate set base revision {} does not equal expected revision {expected_revision}",
                    data.base_revision
                ),
            });
        }

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let actual = ensure_revision(&tx, &data.project_id, expected_revision)?;
        tx.execute(
            "INSERT INTO candidate_sets
                (id, project_id, task_id, base_revision, base_commit, acceptance_digest,
                 policy_json, policy_version, state, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            params![
                data.id,
                data.project_id,
                data.task_id,
                to_i64_u64(data.base_revision, "base revision")?,
                data.base_commit,
                data.acceptance_digest,
                json_text(&data.policy)?,
                to_i64_u64(data.policy_version, "policy version")?,
                data.state,
                data.created_at,
            ],
        )?;
        let revision = bump_revision(&tx, &data.project_id, actual)?;
        tx.commit()?;
        Ok(ConcurrencyMutation {
            id: data.id,
            revision,
        })
    }

    pub fn register_candidate<T: Serialize>(
        &mut self,
        candidate: T,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        let data = parse_candidate(serde_json::to_value(candidate)?)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let project_id = candidate_set_project(&tx, &data.candidate_set_id)?;
        let actual = ensure_revision(&tx, &project_id, expected_revision)?;
        insert_candidate(&tx, &data)?;
        let revision = bump_revision(&tx, &project_id, actual)?;
        tx.commit()?;
        Ok(ConcurrencyMutation {
            id: data.id,
            revision,
        })
    }

    pub fn submit_candidate<T, I, E>(
        &mut self,
        candidate: T,
        evidence: I,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation>
    where
        T: Serialize,
        I: IntoIterator<Item = E>,
        E: Serialize,
    {
        let mut data = parse_candidate(serde_json::to_value(candidate)?)?;
        data.state = "submitted".into();
        if data.submitted_at.is_none() {
            data.submitted_at = Some(now());
        }
        let evidence = evidence
            .into_iter()
            .map(|item| canonical_evidence(&serde_json::to_value(item)?))
            .collect::<ConcurrencyResult<Vec<_>>>()?;

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let project_id = candidate_set_project(&tx, &data.candidate_set_id)?;
        let actual = ensure_revision(&tx, &project_id, expected_revision)?;
        let existing_state = tx
            .query_row(
                "SELECT state FROM candidates WHERE id = ?1",
                [&data.id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(state) = existing_state {
            if candidate_state_is_immutable(&state) {
                return Err(ConcurrencyStoreError::ImmutableCandidate {
                    candidate_id: data.id,
                    state,
                });
            }
            update_candidate(&tx, &data)?;
        } else {
            insert_candidate(&tx, &data)?;
        }
        replace_evidence(&tx, &data.id, &evidence)?;
        let revision = bump_revision(&tx, &project_id, actual)?;
        tx.commit()?;
        Ok(ConcurrencyMutation {
            id: data.id,
            revision,
        })
    }

    pub fn set_candidate_set_state(
        &mut self,
        candidate_set_id: &str,
        state: &str,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        validate_candidate_set_state(state)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let project_id = candidate_set_project(&tx, candidate_set_id)?;
        let actual = ensure_revision(&tx, &project_id, expected_revision)?;
        let changed = tx.execute(
            "UPDATE candidate_sets SET state = ?1, updated_at = ?2 WHERE id = ?3",
            params![state, now(), candidate_set_id],
        )?;
        if changed != 1 {
            return Err(not_found("candidate set", candidate_set_id));
        }
        let revision = bump_revision(&tx, &project_id, actual)?;
        tx.commit()?;
        Ok(ConcurrencyMutation {
            id: candidate_set_id.into(),
            revision,
        })
    }

    pub fn record_adjudication_round<T: Serialize>(
        &mut self,
        round: T,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        let data = parse_round(serde_json::to_value(round)?)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let project_id = candidate_set_project(&tx, &data.candidate_set_id)?;
        let actual = ensure_revision(&tx, &project_id, expected_revision)?;
        tx.execute(
            "INSERT INTO adjudication_rounds
                (id, candidate_set_id, policy_version, required_quorum, decision_json,
                 created_at, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                data.id,
                data.candidate_set_id,
                to_i64_u64(data.policy_version, "policy version")?,
                i64::from(data.required_quorum),
                data.decision.as_ref().map(json_text).transpose()?,
                data.created_at,
                data.decided_at,
            ],
        )?;
        let revision = bump_revision(&tx, &project_id, actual)?;
        tx.commit()?;
        Ok(ConcurrencyMutation {
            id: data.id,
            revision,
        })
    }

    pub fn record_adjudication_ballot<T: Serialize>(
        &mut self,
        ballot: T,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        let data = parse_ballot(serde_json::to_value(ballot)?)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let project_id = round_project(&tx, &data.round_id)?;
        let actual = ensure_revision(&tx, &project_id, expected_revision)?;
        tx.execute(
            "INSERT INTO adjudication_ballots
                (id, round_id, validator_json, assessments_json, ranking_json,
                 abstained, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                data.id,
                data.round_id,
                json_text(&data.validator)?,
                json_text(&data.assessments)?,
                json_text(&data.ranking)?,
                i64::from(data.abstained),
                data.created_at,
            ],
        )?;
        let revision = bump_revision(&tx, &project_id, actual)?;
        tx.commit()?;
        Ok(ConcurrencyMutation {
            id: data.id,
            revision,
        })
    }

    pub fn record_ballot<T: Serialize>(
        &mut self,
        ballot: T,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        self.record_adjudication_ballot(ballot, expected_revision)
    }

    pub fn prepare_promotion<T: Serialize>(
        &mut self,
        intent: T,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        let data = parse_promotion(serde_json::to_value(intent)?)?;
        if data.state != "prepared" {
            return Err(invalid(
                "state",
                "prepare_promotion accepts only prepared intents",
            ));
        }
        if data.expected_revision != expected_revision {
            return Err(ConcurrencyStoreError::InvalidInput {
                field: "expected_revision".into(),
                message: format!(
                    "promotion intent expected revision {} does not equal mutation revision {expected_revision}",
                    data.expected_revision
                ),
            });
        }

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let actual = ensure_revision(&tx, &data.project_id, expected_revision)?;
        let candidate_project = candidate_project(&tx, &data.candidate_id)?;
        if candidate_project != data.project_id {
            return Err(ConcurrencyStoreError::PromotionConflict {
                project_id: data.project_id,
                message: "candidate belongs to another project".into(),
            });
        }
        let candidate_state: String = tx.query_row(
            "SELECT state FROM candidates WHERE id = ?1",
            [&data.candidate_id],
            |row| row.get(0),
        )?;
        if !matches!(
            candidate_state.as_str(),
            "eligible" | "selected" | "promoted"
        ) {
            return Err(ConcurrencyStoreError::PromotionConflict {
                project_id: data.project_id,
                message: format!("candidate is not eligible for promotion: {candidate_state}"),
            });
        }
        let active: Option<String> = tx
            .query_row(
                "SELECT id FROM promotion_intents
                 WHERE project_id = ?1 AND state IN ('prepared', 'ref_updated')
                 LIMIT 1",
                [&data.project_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(active_id) = active {
            return Err(ConcurrencyStoreError::PromotionConflict {
                project_id: data.project_id,
                message: format!("promotion intent {active_id} is already active"),
            });
        }

        tx.execute(
            "INSERT INTO promotion_intents
                (id, project_id, task_id, candidate_set_id, candidate_id, expected_revision,
                 expected_commit, target_commit, canonical_ref, state, created_at, updated_at,
                 finalized_at, conflict_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'prepared', ?10, ?10, NULL, NULL)",
            params![
                data.id,
                data.project_id,
                data.task_id,
                data.candidate_set_id,
                data.candidate_id,
                to_i64_u64(data.expected_revision, "expected revision")?,
                data.expected_commit,
                data.target_commit,
                data.canonical_ref,
                data.created_at,
            ],
        )?;
        insert_promotion_event(
            &tx,
            &data.id,
            &data.project_id,
            None,
            "prepared",
            "prepare",
            json!({
                "expectedCommit": data.expected_commit,
                "targetCommit": data.target_commit,
                "canonicalRef": data.canonical_ref,
            }),
        )?;
        let revision = bump_revision(&tx, &data.project_id, actual)?;
        tx.commit()?;
        Ok(ConcurrencyMutation {
            id: data.id,
            revision,
        })
    }

    pub fn mark_promotion_ref_updated(
        &mut self,
        intent_id: &str,
        observed_commit: &str,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let intent = load_promotion_tx(&tx, intent_id)?
            .ok_or_else(|| not_found("promotion intent", intent_id))?;
        let actual = ensure_revision(&tx, &intent.project_id, expected_revision)?;
        if intent.state == "ref_updated" {
            if observed_commit != intent.target_commit {
                return Err(ConcurrencyStoreError::PromotionConflict {
                    project_id: intent.project_id,
                    message: "ref-updated intent observed a non-target commit".into(),
                });
            }
            return Ok(ConcurrencyMutation {
                id: intent.id,
                revision: actual,
            });
        }
        if intent.state != "prepared" {
            return Err(ConcurrencyStoreError::PromotionConflict {
                project_id: intent.project_id,
                message: format!("cannot mark state {} as ref_updated", intent.state),
            });
        }
        if observed_commit != intent.target_commit {
            return Err(ConcurrencyStoreError::PromotionConflict {
                project_id: intent.project_id,
                message: "compare-and-swap result does not equal target commit".into(),
            });
        }
        let timestamp = now();
        tx.execute(
            "UPDATE promotion_intents SET state = 'ref_updated', updated_at = ?1 WHERE id = ?2",
            params![timestamp, intent.id],
        )?;
        insert_promotion_event(
            &tx,
            &intent.id,
            &intent.project_id,
            Some("prepared"),
            "ref_updated",
            "git_ref_updated",
            json!({"observedCommit": observed_commit}),
        )?;
        let revision = bump_revision(&tx, &intent.project_id, actual)?;
        tx.commit()?;
        Ok(ConcurrencyMutation {
            id: intent.id,
            revision,
        })
    }

    pub fn finalize_promotion(
        &mut self,
        intent_id: &str,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let intent = load_promotion_tx(&tx, intent_id)?
            .ok_or_else(|| not_found("promotion intent", intent_id))?;
        let actual = ensure_revision(&tx, &intent.project_id, expected_revision)?;
        if intent.state == "finalized" {
            return Ok(ConcurrencyMutation {
                id: intent.id,
                revision: actual,
            });
        }
        if intent.state != "ref_updated" {
            return Err(ConcurrencyStoreError::PromotionConflict {
                project_id: intent.project_id,
                message: format!("cannot finalize promotion in state {}", intent.state),
            });
        }
        finalize_promotion_tx(&tx, &intent)?;
        let revision = bump_revision(&tx, &intent.project_id, actual)?;
        tx.commit()?;
        Ok(ConcurrencyMutation {
            id: intent.id,
            revision,
        })
    }

    pub fn abort_promotion(
        &mut self,
        intent_id: &str,
        reason: &str,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        self.rollback_promotion(intent_id, reason, expected_revision)
    }

    pub fn rollback_promotion(
        &mut self,
        intent_id: &str,
        reason: &str,
        expected_revision: u64,
    ) -> ConcurrencyResult<ConcurrencyMutation> {
        if reason.trim().is_empty() {
            return Err(ConcurrencyStoreError::InvalidInput {
                field: "reason".into(),
                message: "rollback reason must not be empty".into(),
            });
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let intent = load_promotion_tx(&tx, intent_id)?
            .ok_or_else(|| not_found("promotion intent", intent_id))?;
        let actual = ensure_revision(&tx, &intent.project_id, expected_revision)?;
        if intent.state == "aborted" {
            return Ok(ConcurrencyMutation {
                id: intent.id,
                revision: actual,
            });
        }
        if !matches!(
            intent.state.as_str(),
            "prepared" | "ref_updated" | "conflicted"
        ) {
            return Err(ConcurrencyStoreError::PromotionConflict {
                project_id: intent.project_id,
                message: format!("cannot rollback promotion in state {}", intent.state),
            });
        }
        let timestamp = now();
        tx.execute(
            "UPDATE promotion_intents
             SET state = 'aborted', updated_at = ?1, conflict_reason = ?2
             WHERE id = ?3",
            params![timestamp, reason, intent.id],
        )?;
        insert_promotion_event(
            &tx,
            &intent.id,
            &intent.project_id,
            Some(&intent.state),
            "aborted",
            "rollback",
            json!({"reason": reason}),
        )?;
        let revision = bump_revision(&tx, &intent.project_id, actual)?;
        tx.commit()?;
        Ok(ConcurrencyMutation {
            id: intent.id,
            revision,
        })
    }

    /// Reconcile one incomplete intent after observing the canonical Git ref.
    ///
    /// The persisted intent contains enough information to retry from the
    /// expected commit, finalize after observing the target commit, or mark a
    /// foreign commit as conflicted without silently overwriting it.
    pub fn recover_promotion(
        &mut self,
        intent_id: &str,
        observed_commit: &str,
        expected_revision: u64,
    ) -> ConcurrencyResult<PromotionRecovery> {
        let existing = self
            .promotion_intent(intent_id)?
            .ok_or_else(|| not_found("promotion intent", intent_id))?;
        let state = string_field(&existing, "state")?;
        let project_id = string_field(&existing, "projectId")?;
        if matches!(state.as_str(), "finalized" | "aborted") {
            return Ok(PromotionRecovery {
                intent: existing,
                action: PromotionRecoveryAction::Noop,
                revision: self.current_revision(&project_id)?,
            });
        }
        let expected_commit = string_field(&existing, "expectedCommit")?;
        let target_commit = string_field(&existing, "targetCommit")?;
        if observed_commit == expected_commit {
            return Ok(PromotionRecovery {
                intent: existing,
                action: PromotionRecoveryAction::Retry,
                revision: self.current_revision(&project_id)?,
            });
        }
        if observed_commit == target_commit {
            if state == "prepared" {
                self.mark_promotion_ref_updated(intent_id, observed_commit, expected_revision)?;
                let current = self.current_revision(&project_id)?;
                let mutation = self.finalize_promotion(intent_id, current)?;
                let intent = self
                    .promotion_intent(intent_id)?
                    .ok_or_else(|| not_found("promotion intent", intent_id))?;
                return Ok(PromotionRecovery {
                    intent,
                    action: PromotionRecoveryAction::Finalize,
                    revision: mutation.revision,
                });
            }
            let mutation = self.finalize_promotion(intent_id, expected_revision)?;
            let intent = self
                .promotion_intent(intent_id)?
                .ok_or_else(|| not_found("promotion intent", intent_id))?;
            return Ok(PromotionRecovery {
                intent,
                action: PromotionRecoveryAction::Finalize,
                revision: mutation.revision,
            });
        }

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let actual = ensure_revision(&tx, &project_id, expected_revision)?;
        let timestamp = now();
        tx.execute(
            "UPDATE promotion_intents
             SET state = 'conflicted', updated_at = ?1, conflict_reason = ?2
             WHERE id = ?3",
            params![
                timestamp,
                format!("canonical ref observed at unexpected commit {observed_commit}"),
                intent_id,
            ],
        )?;
        insert_promotion_event(
            &tx,
            intent_id,
            &project_id,
            Some(&state),
            "conflicted",
            "recovery_conflict",
            json!({"observedCommit": observed_commit}),
        )?;
        let revision = bump_revision(&tx, &project_id, actual)?;
        tx.commit()?;
        let intent = self
            .promotion_intent(intent_id)?
            .ok_or_else(|| not_found("promotion intent", intent_id))?;
        Ok(PromotionRecovery {
            intent,
            action: PromotionRecoveryAction::Conflict,
            revision,
        })
    }

    pub fn resume_promotion(
        &mut self,
        intent_id: &str,
        observed_commit: &str,
        expected_revision: u64,
    ) -> ConcurrencyResult<PromotionRecovery> {
        self.recover_promotion(intent_id, observed_commit, expected_revision)
    }

    pub fn candidate_set(&self, id: &str) -> ConcurrencyResult<Option<Value>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, project_id, task_id, base_revision, base_commit,
                        acceptance_digest, policy_json, policy_version, state,
                        created_at, updated_at
                 FROM candidate_sets WHERE id = ?1",
                [id],
                row_candidate_set,
            )
            .optional()?;
        row.map(|row| candidate_set_value(&row)).transpose()
    }

    /// Return active candidate sets and their bounded candidate metadata for
    /// mutation admission. Canonical writes need the candidate resource
    /// intents, not merely the set policy, so this projection deliberately
    /// includes the same candidate shape exposed by `candidate_set_projection`.
    pub fn active_candidate_set_projections(&self) -> ConcurrencyResult<Vec<Value>> {
        let rows = self
            .conn
            .prepare(
                "SELECT id, project_id, task_id, base_revision, base_commit,
                        acceptance_digest, policy_json, policy_version, state,
                        created_at, updated_at
                 FROM candidate_sets
                 WHERE state IN ('open', 'adjudicating', 'promoting')
                 ORDER BY created_at ASC, id ASC",
            )?
            .query_map([], row_candidate_set)?
            .collect::<Result<Vec<_>, _>>()?;

        rows.into_iter()
            .map(|row| {
                let candidate_set = candidate_set_value(&row)?;
                let candidates = self
                    .conn
                    .prepare(
                        "SELECT id, candidate_set_id, state, base_commit, result_commit,
                                diff_digest, summary, provenance_json, supersedes_candidate_id,
                                created_at, updated_at, submitted_at
                         FROM candidates
                         WHERE candidate_set_id = ?1
                         ORDER BY created_at ASC, id ASC",
                    )?
                    .query_map([&row.id], row_candidate)
                    .map_err(ConcurrencyStoreError::from)?
                    .collect::<Result<Vec<_>, _>>()?;
                let candidates = candidates
                    .iter()
                    .map(|candidate| candidate_value(&self.conn, candidate))
                    .collect::<ConcurrencyResult<Vec<_>>>()?;
                Ok(json!({
                    "candidateSet": candidate_set,
                    "candidates": candidates,
                }))
            })
            .collect()
    }

    pub fn candidate(&self, id: &str) -> ConcurrencyResult<Option<Value>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, candidate_set_id, state, base_commit, result_commit,
                        diff_digest, summary, provenance_json, supersedes_candidate_id,
                        created_at, updated_at, submitted_at
                 FROM candidates WHERE id = ?1",
                [id],
                row_candidate,
            )
            .optional()?;
        row.map(|row| candidate_value(&self.conn, &row)).transpose()
    }

    pub fn adjudication_round(&self, id: &str) -> ConcurrencyResult<Option<Value>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, candidate_set_id, policy_version, required_quorum,
                        decision_json, created_at, decided_at
                 FROM adjudication_rounds WHERE id = ?1",
                [id],
                row_round,
            )
            .optional()?;
        row.map(|row| round_value(&self.conn, &row)).transpose()
    }

    pub fn promotion_intent(&self, id: &str) -> ConcurrencyResult<Option<Value>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, project_id, task_id, candidate_set_id, candidate_id,
                        expected_revision, expected_commit, target_commit, canonical_ref,
                        state, created_at, updated_at, finalized_at, conflict_reason
                 FROM promotion_intents WHERE id = ?1",
                [id],
                row_promotion,
            )
            .optional()?;
        row.map(|row| promotion_value(&row)).transpose()
    }

    pub fn incomplete_promotions(
        &self,
        project_id: &str,
        limit: usize,
    ) -> ConcurrencyResult<Vec<Value>> {
        let limit = bounded_limit(limit);
        let rows = self
            .conn
            .prepare(
                "SELECT id, project_id, task_id, candidate_set_id, candidate_id,
                    expected_revision, expected_commit, target_commit, canonical_ref,
                    state, created_at, updated_at, finalized_at, conflict_reason
             FROM promotion_intents
             WHERE project_id = ?1 AND state IN ('prepared', 'ref_updated', 'conflicted')
             ORDER BY updated_at ASC, id ASC LIMIT ?2",
            )?
            .query_map(
                params![project_id, to_i64(limit, "projection limit")?],
                row_promotion,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        rows.iter().map(promotion_value).collect()
    }

    pub fn bounded_projection(
        &self,
        project_id: &str,
        limit: usize,
    ) -> ConcurrencyResult<ConcurrencyProjection> {
        let limit = bounded_limit(limit);
        let candidate_sets = self.list_candidate_sets(project_id, limit)?;
        let candidates = self.list_candidates(project_id, limit)?;
        let rounds = self.list_rounds(project_id, limit)?;
        let ballots = self.list_ballots(project_id, limit)?;
        let promotions = self.list_promotions(project_id, limit)?;
        let counts = self.counts(project_id)?;
        let truncated = candidate_sets.1 || candidates.1 || rounds.1 || ballots.1 || promotions.1;
        let value = json!({
            "projectId": project_id,
            "candidateSets": candidate_sets.0,
            "candidates": candidates.0,
            "adjudicationRounds": rounds.0,
            "adjudicationBallots": ballots.0,
            "promotionIntents": promotions.0,
            "counts": counts,
            "limit": limit,
            "truncated": truncated,
        });
        Ok(ConcurrencyProjection {
            value,
            limit,
            truncated,
        })
    }

    pub fn project(&self, project_id: &str, limit: usize) -> ConcurrencyResult<Value> {
        Ok(self.bounded_projection(project_id, limit)?.value)
    }

    pub fn prompt_projection(&self, project_id: &str, limit: usize) -> ConcurrencyResult<Value> {
        self.project(project_id, limit)
    }

    pub fn ui_projection(&self, project_id: &str, limit: usize) -> ConcurrencyResult<Value> {
        self.project(project_id, limit)
    }

    pub fn candidate_set_projection(
        &self,
        candidate_set_id: &str,
        limit: usize,
    ) -> ConcurrencyResult<Value> {
        let limit = bounded_limit(limit);
        let set = self
            .candidate_set(candidate_set_id)?
            .ok_or_else(|| not_found("candidate set", candidate_set_id))?;
        let project_id = string_field(&set, "projectId")?;
        let candidate_rows = self
            .conn
            .prepare(
                "SELECT id, candidate_set_id, state, base_commit, result_commit,
                    diff_digest, summary, provenance_json, supersedes_candidate_id,
                    created_at, updated_at, submitted_at
             FROM candidates WHERE candidate_set_id = ?1
             ORDER BY created_at ASC, id ASC LIMIT ?2",
            )?
            .query_map(
                params![candidate_set_id, to_i64(limit, "projection limit")?],
                row_candidate,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let candidates = candidate_rows
            .iter()
            .map(|row| candidate_value(&self.conn, row))
            .collect::<ConcurrencyResult<Vec<_>>>()?;
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM candidates WHERE candidate_set_id = ?1",
            [candidate_set_id],
            |row| row.get(0),
        )?;
        let truncated = count > i64::try_from(limit).unwrap_or(i64::MAX);
        Ok(json!({
            "projectId": project_id,
            "candidateSet": set,
            "candidates": candidates,
            "count": count,
            "limit": limit,
            "truncated": truncated,
        }))
    }

    fn list_candidate_sets(
        &self,
        project_id: &str,
        limit: usize,
    ) -> ConcurrencyResult<(Vec<Value>, bool)> {
        let rows = self
            .conn
            .prepare(
                "SELECT id, project_id, task_id, base_revision, base_commit,
                    acceptance_digest, policy_json, policy_version, state,
                    created_at, updated_at
             FROM candidate_sets WHERE project_id = ?1
             ORDER BY created_at ASC, id ASC LIMIT ?2",
            )?
            .query_map(
                params![project_id, to_i64(limit + 1, "projection limit")?],
                row_candidate_set,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        bounded_values(rows, limit, candidate_set_value)
    }

    fn list_candidates(
        &self,
        project_id: &str,
        limit: usize,
    ) -> ConcurrencyResult<(Vec<Value>, bool)> {
        let rows = self
            .conn
            .prepare(
                "SELECT c.id, c.candidate_set_id, c.state, c.base_commit, c.result_commit,
                    c.diff_digest, c.summary, c.provenance_json, c.supersedes_candidate_id,
                    c.created_at, c.updated_at, c.submitted_at
             FROM candidates c
             JOIN candidate_sets s ON s.id = c.candidate_set_id
             WHERE s.project_id = ?1
             ORDER BY c.created_at ASC, c.id ASC LIMIT ?2",
            )?
            .query_map(
                params![project_id, to_i64(limit + 1, "projection limit")?],
                row_candidate,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let truncated = rows.len() > limit;
        let values = rows
            .iter()
            .take(limit)
            .map(|row| candidate_value(&self.conn, row))
            .collect::<ConcurrencyResult<Vec<_>>>()?;
        Ok((values, truncated))
    }

    fn list_rounds(&self, project_id: &str, limit: usize) -> ConcurrencyResult<(Vec<Value>, bool)> {
        let rows = self
            .conn
            .prepare(
                "SELECT r.id, r.candidate_set_id, r.policy_version, r.required_quorum,
                    r.decision_json, r.created_at, r.decided_at
             FROM adjudication_rounds r
             JOIN candidate_sets s ON s.id = r.candidate_set_id
             WHERE s.project_id = ?1
             ORDER BY r.created_at ASC, r.id ASC LIMIT ?2",
            )?
            .query_map(
                params![project_id, to_i64(limit + 1, "projection limit")?],
                row_round,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let truncated = rows.len() > limit;
        let values = rows
            .iter()
            .take(limit)
            .map(|row| round_value(&self.conn, row))
            .collect::<ConcurrencyResult<Vec<_>>>()?;
        Ok((values, truncated))
    }

    fn list_ballots(
        &self,
        project_id: &str,
        limit: usize,
    ) -> ConcurrencyResult<(Vec<Value>, bool)> {
        let rows = self
            .conn
            .prepare(
                "SELECT b.id, b.round_id, b.validator_json, b.assessments_json,
                    b.ranking_json, b.abstained, b.created_at
             FROM adjudication_ballots b
             JOIN adjudication_rounds r ON r.id = b.round_id
             JOIN candidate_sets s ON s.id = r.candidate_set_id
             WHERE s.project_id = ?1
             ORDER BY b.created_at ASC, b.id ASC LIMIT ?2",
            )?
            .query_map(
                params![project_id, to_i64(limit + 1, "projection limit")?],
                row_ballot,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let truncated = rows.len() > limit;
        let values = rows
            .iter()
            .take(limit)
            .map(ballot_value)
            .collect::<ConcurrencyResult<Vec<_>>>()?;
        Ok((values, truncated))
    }

    fn list_promotions(
        &self,
        project_id: &str,
        limit: usize,
    ) -> ConcurrencyResult<(Vec<Value>, bool)> {
        let rows = self
            .conn
            .prepare(
                "SELECT id, project_id, task_id, candidate_set_id, candidate_id,
                    expected_revision, expected_commit, target_commit, canonical_ref,
                    state, created_at, updated_at, finalized_at, conflict_reason
             FROM promotion_intents WHERE project_id = ?1
             ORDER BY updated_at ASC, id ASC LIMIT ?2",
            )?
            .query_map(
                params![project_id, to_i64(limit + 1, "projection limit")?],
                row_promotion,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        bounded_values(rows, limit, promotion_value)
    }

    fn counts(&self, project_id: &str) -> ConcurrencyResult<Value> {
        let sets = count_query(
            &self.conn,
            "SELECT COUNT(*) FROM candidate_sets WHERE project_id = ?1",
            project_id,
        )?;
        let candidates = count_query(
            &self.conn,
            "SELECT COUNT(*) FROM candidates c
             JOIN candidate_sets s ON s.id = c.candidate_set_id
             WHERE s.project_id = ?1",
            project_id,
        )?;
        let rounds = count_query(
            &self.conn,
            "SELECT COUNT(*) FROM adjudication_rounds r
             JOIN candidate_sets s ON s.id = r.candidate_set_id
             WHERE s.project_id = ?1",
            project_id,
        )?;
        let ballots = count_query(
            &self.conn,
            "SELECT COUNT(*) FROM adjudication_ballots b
             JOIN adjudication_rounds r ON r.id = b.round_id
             JOIN candidate_sets s ON s.id = r.candidate_set_id
             WHERE s.project_id = ?1",
            project_id,
        )?;
        let promotions = count_query(
            &self.conn,
            "SELECT COUNT(*) FROM promotion_intents WHERE project_id = ?1",
            project_id,
        )?;
        Ok(json!({
            "candidateSets": sets,
            "candidates": candidates,
            "adjudicationRounds": rounds,
            "adjudicationBallots": ballots,
            "promotionIntents": promotions,
        }))
    }
}

impl PiTaskerStore {
    pub fn open_concurrency_store(&self) -> ConcurrencyResult<ConcurrencyStore> {
        ConcurrencyStore::open_for(self)
    }
}

#[derive(Debug)]
struct CandidateSetData {
    id: String,
    project_id: String,
    task_id: String,
    base_revision: u64,
    base_commit: String,
    acceptance_digest: String,
    policy: Value,
    policy_version: u64,
    state: String,
    created_at: String,
}

#[derive(Debug)]
struct CandidateData {
    id: String,
    candidate_set_id: String,
    state: String,
    base_commit: String,
    result_commit: Option<String>,
    diff_digest: Option<String>,
    summary: Option<String>,
    provenance: Value,
    resource_intents: Vec<Value>,
    supersedes_candidate_id: Option<String>,
    created_at: String,
    updated_at: String,
    submitted_at: Option<String>,
}

#[derive(Debug)]
struct RoundData {
    id: String,
    candidate_set_id: String,
    policy_version: u64,
    required_quorum: u16,
    decision: Option<Value>,
    created_at: String,
    decided_at: Option<String>,
}

#[derive(Debug)]
struct BallotData {
    id: String,
    round_id: String,
    validator: Value,
    assessments: Value,
    ranking: Value,
    abstained: bool,
    created_at: String,
}

#[derive(Debug)]
struct PromotionData {
    id: String,
    project_id: String,
    task_id: String,
    candidate_set_id: String,
    candidate_id: String,
    expected_revision: u64,
    expected_commit: String,
    target_commit: String,
    canonical_ref: String,
    state: String,
    created_at: String,
}

#[derive(Debug)]
struct CandidateSetRow {
    id: String,
    project_id: String,
    task_id: String,
    base_revision: i64,
    base_commit: String,
    acceptance_digest: String,
    policy_json: String,
    policy_version: i64,
    state: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug)]
struct CandidateRow {
    id: String,
    candidate_set_id: String,
    state: String,
    base_commit: String,
    result_commit: Option<String>,
    diff_digest: Option<String>,
    summary: Option<String>,
    provenance_json: String,
    supersedes_candidate_id: Option<String>,
    created_at: String,
    updated_at: String,
    submitted_at: Option<String>,
}

#[derive(Debug)]
struct RoundRow {
    id: String,
    candidate_set_id: String,
    policy_version: i64,
    required_quorum: i64,
    decision_json: Option<String>,
    created_at: String,
    decided_at: Option<String>,
}

#[derive(Debug)]
struct BallotRow {
    id: String,
    round_id: String,
    validator_json: String,
    assessments_json: String,
    ranking_json: String,
    abstained: i64,
    created_at: String,
}

#[derive(Debug)]
struct PromotionRow {
    id: String,
    project_id: String,
    task_id: String,
    candidate_set_id: String,
    candidate_id: String,
    expected_revision: i64,
    expected_commit: String,
    target_commit: String,
    canonical_ref: String,
    state: String,
    created_at: String,
    updated_at: String,
    finalized_at: Option<String>,
    conflict_reason: Option<String>,
}

fn configure_and_migrate(conn: &mut Connection) -> ConcurrencyResult<()> {
    // Match PiTaskerStore: install the busy handler before WAL, then use
    // WAL-appropriate NORMAL durability for contending writers.
    conn.busy_timeout(std::time::Duration::from_secs(30))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(MIGRATION)?;
    tx.execute(
        "INSERT OR IGNORE INTO concurrency_store_meta (key, value)
         VALUES ('schema_version', ?1)",
        [CONCURRENCY_SCHEMA_VERSION.to_string()],
    )?;
    tx.commit()?;
    Ok(())
}

fn migrate_legacy_project_identity(
    conn: &mut Connection,
    partition: &ProjectPartition,
) -> ConcurrencyResult<()> {
    let legacy_id = legacy_native_project_id_for_list(&partition.list_id).to_string();
    let current_id = native_project_id_for_partition(partition).to_string();
    if legacy_id == current_id {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let legacy_revision = tx
        .query_row(
            "SELECT revision FROM concurrency_project_revisions WHERE project_id = ?1",
            [&legacy_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let legacy_records = legacy_revision.is_some()
        || tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM candidate_sets WHERE project_id = ?1)",
            [&legacy_id],
            |row| row.get::<_, bool>(0),
        )?
        || tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM promotion_intents WHERE project_id = ?1)",
            [&legacy_id],
            |row| row.get::<_, bool>(0),
        )?
        || tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM promotion_events WHERE project_id = ?1)",
            [&legacy_id],
            |row| row.get::<_, bool>(0),
        )?;
    if !legacy_records {
        tx.commit()?;
        return Ok(());
    }

    if let Some(legacy_revision) = legacy_revision {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO concurrency_project_revisions(project_id, revision, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(project_id) DO UPDATE SET
                 revision = MAX(revision, excluded.revision),
                 updated_at = excluded.updated_at",
            params![current_id, legacy_revision, now],
        )?;
        tx.execute(
            "DELETE FROM concurrency_project_revisions WHERE project_id = ?1",
            [&legacy_id],
        )?;
    }
    for table in ["candidate_sets", "promotion_intents", "promotion_events"] {
        tx.execute(
            &format!("UPDATE {table} SET project_id = ?1 WHERE project_id = ?2"),
            params![current_id, legacy_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn parse_candidate_set(value: Value) -> ConcurrencyResult<CandidateSetData> {
    let object = as_object(value, "candidate_set")?;
    let policy = required_value(&object, "policy")?;
    validate_policy(&policy)?;
    let state = optional_string(&object, "state")?.unwrap_or_else(|| "open".into());
    validate_candidate_set_state(&state)?;
    Ok(CandidateSetData {
        id: required_string(&object, "id")?,
        project_id: required_string_any(&object, "project_id", "projectId")?,
        task_id: required_string_any(&object, "task_id", "taskId")?,
        base_revision: required_u64_any(&object, "base_revision", "baseRevision")?,
        base_commit: required_string_any(&object, "base_commit", "baseCommit")?,
        acceptance_digest: required_string_any(&object, "acceptance_digest", "acceptanceDigest")?,
        policy,
        policy_version: required_u64_any(&object, "policy_version", "policyVersion")?,
        state,
        created_at: optional_timestamp(&object, "created_at", "createdAt")?.unwrap_or_else(now),
    })
}

fn parse_candidate(value: Value) -> ConcurrencyResult<CandidateData> {
    let object = as_object(value, "candidate")?;
    let state = optional_string(&object, "state")?.unwrap_or_else(|| "registered".into());
    validate_candidate_state(&state)?;
    let resource_intents = match optional_value_any(&object, "resource_intents", "resourceIntents")?
    {
        Some(Value::Array(values)) => values
            .iter()
            .map(canonical_resource_intent)
            .collect::<ConcurrencyResult<Vec<_>>>()?,
        Some(_) => return Err(invalid("resource_intents", "must be a JSON array")),
        None => Vec::new(),
    };
    let provenance =
        canonical_provenance(required_value_any(&object, "provenance", "provenance")?)?;
    Ok(CandidateData {
        id: required_string(&object, "id")?,
        candidate_set_id: required_string_any(&object, "candidate_set_id", "candidateSetId")?,
        state,
        base_commit: required_string_any(&object, "base_commit", "baseCommit")?,
        result_commit: optional_string_any(&object, "result_commit", "resultCommit")?,
        diff_digest: optional_string_any(&object, "diff_digest", "diffDigest")?,
        summary: optional_string(&object, "summary")?.map(|value| truncate(&value, 4_000)),
        provenance,
        resource_intents,
        supersedes_candidate_id: optional_string_any(
            &object,
            "supersedes_candidate_id",
            "supersedesCandidateId",
        )?,
        created_at: optional_timestamp(&object, "created_at", "createdAt")?.unwrap_or_else(now),
        updated_at: optional_timestamp(&object, "updated_at", "updatedAt")?.unwrap_or_else(now),
        submitted_at: optional_timestamp(&object, "submitted_at", "submittedAt")?,
    })
}

fn parse_round(value: Value) -> ConcurrencyResult<RoundData> {
    let object = as_object(value, "adjudication_round")?;
    let required_quorum = required_u64_any(&object, "required_quorum", "requiredQuorum")?;
    if required_quorum == 0 || required_quorum > u64::from(u16::MAX) {
        return Err(invalid("required_quorum", "must fit a non-zero u16"));
    }
    let decision = optional_value(&object, "decision")?;
    if let Some(decision) = &decision {
        validate_decision(decision)?;
    }
    Ok(RoundData {
        id: required_string(&object, "id")?,
        candidate_set_id: required_string_any(&object, "candidate_set_id", "candidateSetId")?,
        policy_version: required_u64_any(&object, "policy_version", "policyVersion")?,
        required_quorum: required_quorum as u16,
        decision,
        created_at: optional_timestamp(&object, "created_at", "createdAt")?.unwrap_or_else(now),
        decided_at: optional_timestamp(&object, "decided_at", "decidedAt")?,
    })
}

fn parse_ballot(value: Value) -> ConcurrencyResult<BallotData> {
    let object = as_object(value, "adjudication_ballot")?;
    let validator = canonical_validator(required_value_any(&object, "validator", "validator")?)?;
    let assessments = match required_value(&object, "assessments")? {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(canonical_assessment)
                .collect::<ConcurrencyResult<Vec<_>>>()?,
        ),
        _ => return Err(invalid("assessments", "must be a JSON array")),
    };
    let ranking = match required_value(&object, "ranking")? {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(|candidate_id| Value::String(candidate_id.to_owned()))
                        .ok_or_else(|| invalid("ranking", "candidate IDs must be strings"))
                })
                .collect::<ConcurrencyResult<Vec<_>>>()?,
        ),
        _ => return Err(invalid("ranking", "must be a JSON array")),
    };
    Ok(BallotData {
        id: required_string(&object, "id")?,
        round_id: required_string_any(&object, "round_id", "roundId")?,
        validator,
        assessments,
        ranking,
        abstained: optional_bool(&object, "abstained")?.unwrap_or(false),
        created_at: optional_timestamp(&object, "created_at", "createdAt")?.unwrap_or_else(now),
    })
}

fn parse_promotion(value: Value) -> ConcurrencyResult<PromotionData> {
    let object = as_object(value, "promotion_intent")?;
    let state = optional_string(&object, "state")?.unwrap_or_else(|| "prepared".into());
    validate_promotion_state(&state)?;
    let canonical_ref = required_string_any(&object, "canonical_ref", "canonicalRef")?;
    if canonical_ref.trim().is_empty() {
        return Err(invalid("canonical_ref", "must not be empty"));
    }
    Ok(PromotionData {
        id: required_string(&object, "id")?,
        project_id: required_string_any(&object, "project_id", "projectId")?,
        task_id: required_string_any(&object, "task_id", "taskId")?,
        candidate_set_id: required_string_any(&object, "candidate_set_id", "candidateSetId")?,
        candidate_id: required_string_any(&object, "candidate_id", "candidateId")?,
        expected_revision: required_u64_any(&object, "expected_revision", "expectedRevision")?,
        expected_commit: required_string_any(&object, "expected_commit", "expectedCommit")?,
        target_commit: required_string_any(&object, "target_commit", "targetCommit")?,
        canonical_ref,
        state,
        created_at: optional_timestamp(&object, "created_at", "createdAt")?.unwrap_or_else(now),
    })
}

fn insert_candidate(tx: &Transaction<'_>, data: &CandidateData) -> ConcurrencyResult<()> {
    tx.execute(
        "INSERT INTO candidates
            (id, candidate_set_id, state, base_commit, result_commit, diff_digest, summary,
             provenance_json, supersedes_candidate_id, created_at, updated_at, submitted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            data.id,
            data.candidate_set_id,
            data.state,
            data.base_commit,
            data.result_commit,
            data.diff_digest,
            data.summary,
            json_text(&data.provenance)?,
            data.supersedes_candidate_id,
            data.created_at,
            data.updated_at,
            data.submitted_at,
        ],
    )?;
    replace_resource_intents(tx, &data.id, &data.resource_intents)?;
    Ok(())
}

fn update_candidate(tx: &Transaction<'_>, data: &CandidateData) -> ConcurrencyResult<()> {
    tx.execute(
        "UPDATE candidates SET
            candidate_set_id = ?1, state = ?2, base_commit = ?3, result_commit = ?4,
            diff_digest = ?5, summary = ?6, provenance_json = ?7,
            supersedes_candidate_id = ?8, updated_at = ?9, submitted_at = ?10
         WHERE id = ?11",
        params![
            data.candidate_set_id,
            data.state,
            data.base_commit,
            data.result_commit,
            data.diff_digest,
            data.summary,
            json_text(&data.provenance)?,
            data.supersedes_candidate_id,
            data.updated_at,
            data.submitted_at,
            data.id,
        ],
    )?;
    replace_resource_intents(tx, &data.id, &data.resource_intents)?;
    Ok(())
}

fn replace_resource_intents(
    tx: &Transaction<'_>,
    candidate_id: &str,
    intents: &[Value],
) -> ConcurrencyResult<()> {
    tx.execute(
        "DELETE FROM candidate_resource_intents WHERE candidate_id = ?1",
        [candidate_id],
    )?;
    for (ordinal, intent) in intents.iter().enumerate() {
        tx.execute(
            "INSERT INTO candidate_resource_intents (candidate_id, ordinal, intent_json)
             VALUES (?1, ?2, ?3)",
            params![
                candidate_id,
                to_i64(ordinal, "resource intent ordinal")?,
                json_text(intent)?
            ],
        )?;
    }
    Ok(())
}

fn replace_evidence(
    tx: &Transaction<'_>,
    candidate_id: &str,
    evidence: &[Value],
) -> ConcurrencyResult<()> {
    tx.execute(
        "DELETE FROM candidate_evidence WHERE candidate_id = ?1",
        [candidate_id],
    )?;
    for (ordinal, item) in evidence.iter().enumerate() {
        tx.execute(
            "INSERT INTO candidate_evidence (candidate_id, ordinal, evidence_json)
             VALUES (?1, ?2, ?3)",
            params![
                candidate_id,
                to_i64(ordinal, "evidence ordinal")?,
                json_text(item)?
            ],
        )?;
    }
    Ok(())
}

fn candidate_set_project(tx: &Transaction<'_>, id: &str) -> ConcurrencyResult<String> {
    tx.query_row(
        "SELECT project_id FROM candidate_sets WHERE id = ?1",
        [id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| not_found("candidate set", id))
}

fn candidate_project(tx: &Transaction<'_>, id: &str) -> ConcurrencyResult<String> {
    tx.query_row(
        "SELECT s.project_id FROM candidates c
         JOIN candidate_sets s ON s.id = c.candidate_set_id
         WHERE c.id = ?1",
        [id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| not_found("candidate", id))
}

fn round_project(tx: &Transaction<'_>, id: &str) -> ConcurrencyResult<String> {
    tx.query_row(
        "SELECT s.project_id FROM adjudication_rounds r
         JOIN candidate_sets s ON s.id = r.candidate_set_id
         WHERE r.id = ?1",
        [id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| not_found("adjudication round", id))
}

fn ensure_revision(
    tx: &Transaction<'_>,
    project_id: &str,
    expected: u64,
) -> ConcurrencyResult<u64> {
    tx.execute(
        "INSERT OR IGNORE INTO concurrency_project_revisions
            (project_id, revision, updated_at) VALUES (?1, 0, ?2)",
        params![project_id, now()],
    )?;
    let actual = from_i64(
        tx.query_row(
            "SELECT revision FROM concurrency_project_revisions WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )?,
        "project revision",
    )?;
    if actual != expected {
        return Err(ConcurrencyStoreError::RevisionConflict {
            project_id: project_id.into(),
            expected,
            actual,
        });
    }
    Ok(actual)
}

fn bump_revision(tx: &Transaction<'_>, project_id: &str, current: u64) -> ConcurrencyResult<u64> {
    let next = current
        .checked_add(1)
        .ok_or_else(|| ConcurrencyStoreError::Schema("project revision overflow".into()))?;
    tx.execute(
        "UPDATE concurrency_project_revisions
         SET revision = ?1, updated_at = ?2 WHERE project_id = ?3",
        params![to_i64_u64(next, "project revision")?, now(), project_id],
    )?;
    Ok(next)
}

fn insert_promotion_event(
    tx: &Transaction<'_>,
    intent_id: &str,
    project_id: &str,
    from_state: Option<&str>,
    to_state: &str,
    event_kind: &str,
    details: Value,
) -> ConcurrencyResult<()> {
    tx.execute(
        "INSERT INTO promotion_events
            (id, intent_id, project_id, from_state, to_state, event_kind, details_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            format!("pevent_{}", Uuid::new_v4().simple()),
            intent_id,
            project_id,
            from_state,
            to_state,
            event_kind,
            json_text(&details)?,
            now(),
        ],
    )?;
    Ok(())
}

fn finalize_promotion_tx(tx: &Transaction<'_>, intent: &PromotionRow) -> ConcurrencyResult<()> {
    let timestamp = now();
    tx.execute(
        "UPDATE promotion_intents
         SET state = 'finalized', updated_at = ?1, finalized_at = ?1, conflict_reason = NULL
         WHERE id = ?2",
        params![timestamp, intent.id],
    )?;
    tx.execute(
        "UPDATE candidates SET state = 'promoted', updated_at = ?1
         WHERE id = ?2",
        params![timestamp, intent.candidate_id],
    )?;
    tx.execute(
        "UPDATE candidate_sets SET state = 'completed', updated_at = ?1
         WHERE id = ?2",
        params![timestamp, intent.candidate_set_id],
    )?;
    insert_promotion_event(
        tx,
        &intent.id,
        &intent.project_id,
        Some(&intent.state),
        "finalized",
        "finalize",
        json!({"targetCommit": intent.target_commit}),
    )?;
    Ok(())
}

fn load_promotion_tx(tx: &Transaction<'_>, id: &str) -> ConcurrencyResult<Option<PromotionRow>> {
    tx.query_row(
        "SELECT id, project_id, task_id, candidate_set_id, candidate_id,
                expected_revision, expected_commit, target_commit, canonical_ref,
                state, created_at, updated_at, finalized_at, conflict_reason
         FROM promotion_intents WHERE id = ?1",
        [id],
        row_promotion,
    )
    .optional()
    .map_err(Into::into)
}

fn row_candidate_set(row: &rusqlite::Row<'_>) -> rusqlite::Result<CandidateSetRow> {
    Ok(CandidateSetRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        task_id: row.get(2)?,
        base_revision: row.get(3)?,
        base_commit: row.get(4)?,
        acceptance_digest: row.get(5)?,
        policy_json: row.get(6)?,
        policy_version: row.get(7)?,
        state: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn row_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<CandidateRow> {
    Ok(CandidateRow {
        id: row.get(0)?,
        candidate_set_id: row.get(1)?,
        state: row.get(2)?,
        base_commit: row.get(3)?,
        result_commit: row.get(4)?,
        diff_digest: row.get(5)?,
        summary: row.get(6)?,
        provenance_json: row.get(7)?,
        supersedes_candidate_id: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        submitted_at: row.get(11)?,
    })
}

fn row_round(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoundRow> {
    Ok(RoundRow {
        id: row.get(0)?,
        candidate_set_id: row.get(1)?,
        policy_version: row.get(2)?,
        required_quorum: row.get(3)?,
        decision_json: row.get(4)?,
        created_at: row.get(5)?,
        decided_at: row.get(6)?,
    })
}

fn row_ballot(row: &rusqlite::Row<'_>) -> rusqlite::Result<BallotRow> {
    Ok(BallotRow {
        id: row.get(0)?,
        round_id: row.get(1)?,
        validator_json: row.get(2)?,
        assessments_json: row.get(3)?,
        ranking_json: row.get(4)?,
        abstained: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn row_promotion(row: &rusqlite::Row<'_>) -> rusqlite::Result<PromotionRow> {
    Ok(PromotionRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        task_id: row.get(2)?,
        candidate_set_id: row.get(3)?,
        candidate_id: row.get(4)?,
        expected_revision: row.get(5)?,
        expected_commit: row.get(6)?,
        target_commit: row.get(7)?,
        canonical_ref: row.get(8)?,
        state: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        finalized_at: row.get(12)?,
        conflict_reason: row.get(13)?,
    })
}

fn candidate_set_value(row: &CandidateSetRow) -> ConcurrencyResult<Value> {
    Ok(json!({
        "id": row.id,
        "projectId": row.project_id,
        "taskId": row.task_id,
        "baseRevision": from_i64(row.base_revision, "base revision")?,
        "baseCommit": row.base_commit,
        "acceptanceDigest": row.acceptance_digest,
        "policy": serde_json::from_str::<Value>(&row.policy_json)?,
        "policyVersion": from_i64(row.policy_version, "policy version")?,
        "state": row.state,
        "createdAt": row.created_at,
        "updatedAt": row.updated_at,
    }))
}

fn candidate_value(conn: &Connection, row: &CandidateRow) -> ConcurrencyResult<Value> {
    let resource_rows = conn
        .prepare(
            "SELECT intent_json FROM candidate_resource_intents
             WHERE candidate_id = ?1 ORDER BY ordinal ASC",
        )?
        .query_map([&row.id], |result| result.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let evidence_rows = conn
        .prepare(
            "SELECT evidence_json FROM candidate_evidence
             WHERE candidate_id = ?1 ORDER BY ordinal ASC",
        )?
        .query_map([&row.id], |result| result.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "id": row.id,
        "candidateSetId": row.candidate_set_id,
        "state": row.state,
        "baseCommit": row.base_commit,
        "resultCommit": row.result_commit,
        "diffDigest": row.diff_digest,
        "summary": row.summary,
        "provenance": serde_json::from_str::<Value>(&row.provenance_json)?,
        "resourceIntents": resource_rows.iter().map(|value| serde_json::from_str::<Value>(value)).collect::<Result<Vec<_>, _>>()?,
        "evidence": evidence_rows.iter().map(|value| serde_json::from_str::<Value>(value)).collect::<Result<Vec<_>, _>>()?,
        "supersedesCandidateId": row.supersedes_candidate_id,
        "createdAt": row.created_at,
        "updatedAt": row.updated_at,
        "submittedAt": row.submitted_at,
    }))
}

fn round_value(conn: &Connection, row: &RoundRow) -> ConcurrencyResult<Value> {
    let ballot_rows = conn
        .prepare(
            "SELECT id, round_id, validator_json, assessments_json, ranking_json,
                    abstained, created_at
             FROM adjudication_ballots WHERE round_id = ?1
             ORDER BY created_at ASC, id ASC",
        )?
        .query_map([&row.id], row_ballot)?
        .collect::<Result<Vec<_>, _>>()?;
    let ballots = ballot_rows
        .iter()
        .map(ballot_value)
        .collect::<ConcurrencyResult<Vec<_>>>()?;
    Ok(json!({
        "id": row.id,
        "candidateSetId": row.candidate_set_id,
        "policyVersion": from_i64(row.policy_version, "policy version")?,
        "requiredQuorum": from_i64(row.required_quorum, "required quorum")?,
        "decision": row.decision_json.as_ref().map(|value| serde_json::from_str::<Value>(value)).transpose()?,
        "ballots": ballots,
        "createdAt": row.created_at,
        "decidedAt": row.decided_at,
    }))
}

fn ballot_value(row: &BallotRow) -> ConcurrencyResult<Value> {
    Ok(json!({
        "id": row.id,
        "roundId": row.round_id,
        "validator": serde_json::from_str::<Value>(&row.validator_json)?,
        "assessments": serde_json::from_str::<Value>(&row.assessments_json)?,
        "ranking": serde_json::from_str::<Value>(&row.ranking_json)?,
        "abstained": row.abstained != 0,
        "createdAt": row.created_at,
    }))
}

fn promotion_value(row: &PromotionRow) -> ConcurrencyResult<Value> {
    Ok(json!({
        "id": row.id,
        "projectId": row.project_id,
        "taskId": row.task_id,
        "candidateSetId": row.candidate_set_id,
        "candidateId": row.candidate_id,
        "expectedRevision": from_i64(row.expected_revision, "expected revision")?,
        "expectedCommit": row.expected_commit,
        "targetCommit": row.target_commit,
        "canonicalRef": row.canonical_ref,
        "state": row.state,
        "createdAt": row.created_at,
        "updatedAt": row.updated_at,
        "finalizedAt": row.finalized_at,
        "conflictReason": row.conflict_reason,
    }))
}

fn bounded_values<T, F>(
    rows: Vec<T>,
    limit: usize,
    convert: F,
) -> ConcurrencyResult<(Vec<Value>, bool)>
where
    F: Fn(&T) -> ConcurrencyResult<Value>,
{
    let truncated = rows.len() > limit;
    let values = rows
        .iter()
        .take(limit)
        .map(convert)
        .collect::<ConcurrencyResult<Vec<_>>>()?;
    Ok((values, truncated))
}

fn count_query(conn: &Connection, sql: &str, project_id: &str) -> ConcurrencyResult<i64> {
    Ok(conn.query_row(sql, [project_id], |row| row.get(0))?)
}

fn as_object(value: Value, field: &str) -> ConcurrencyResult<Map<String, Value>> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| invalid(field, "must be a JSON object"))
}

fn required_value(object: &Map<String, Value>, key: &str) -> ConcurrencyResult<Value> {
    object
        .get(key)
        .cloned()
        .ok_or_else(|| invalid(key, "is required"))
}

fn required_value_any(
    object: &Map<String, Value>,
    snake: &str,
    camel: &str,
) -> ConcurrencyResult<Value> {
    object
        .get(snake)
        .or_else(|| object.get(camel))
        .cloned()
        .ok_or_else(|| invalid(camel, "is required"))
}

fn optional_value(object: &Map<String, Value>, key: &str) -> ConcurrencyResult<Option<Value>> {
    Ok(object.get(key).cloned())
}

fn optional_value_any(
    object: &Map<String, Value>,
    snake: &str,
    camel: &str,
) -> ConcurrencyResult<Option<Value>> {
    Ok(object.get(snake).or_else(|| object.get(camel)).cloned())
}

fn required_string(object: &Map<String, Value>, key: &str) -> ConcurrencyResult<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| invalid(key, "must be a non-empty string"))
}

fn required_string_any(
    object: &Map<String, Value>,
    snake: &str,
    camel: &str,
) -> ConcurrencyResult<String> {
    object
        .get(snake)
        .or_else(|| object.get(camel))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| invalid(camel, "must be a non-empty string"))
}

fn optional_string(object: &Map<String, Value>, key: &str) -> ConcurrencyResult<Option<String>> {
    object
        .get(key)
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| invalid(key, "must be a string when present"))
        })
        .transpose()
}

fn optional_string_any(
    object: &Map<String, Value>,
    snake: &str,
    camel: &str,
) -> ConcurrencyResult<Option<String>> {
    object
        .get(snake)
        .or_else(|| object.get(camel))
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| invalid(camel, "must be a string when present"))
        })
        .transpose()
}

fn required_u64_any(
    object: &Map<String, Value>,
    snake: &str,
    camel: &str,
) -> ConcurrencyResult<u64> {
    let value = object
        .get(snake)
        .or_else(|| object.get(camel))
        .ok_or_else(|| invalid(camel, "is required"))?;
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        .ok_or_else(|| invalid(camel, "must be a non-negative integer"))
}

fn optional_bool(object: &Map<String, Value>, key: &str) -> ConcurrencyResult<Option<bool>> {
    object
        .get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| invalid(key, "must be a boolean when present"))
        })
        .transpose()
}

fn optional_timestamp(
    object: &Map<String, Value>,
    snake: &str,
    camel: &str,
) -> ConcurrencyResult<Option<String>> {
    let Some(value) = object.get(snake).or_else(|| object.get(camel)) else {
        return Ok(None);
    };
    if let Some(value) = value.as_str() {
        return Ok(Some(value.to_owned()));
    }
    if let Some(value) = value.as_i64() {
        return Ok(Some(value.to_string()));
    }
    Err(invalid(
        camel,
        "must be an RFC3339 string or integer timestamp",
    ))
}

fn canonical_provenance(value: Value) -> ConcurrencyResult<Value> {
    let object = as_object(value, "provenance")?;
    let mut result = Map::new();
    for (snake, camel) in [
        ("session_id", "sessionId"),
        ("agent_id", "agentId"),
        ("model_id", "modelId"),
        ("work_unit_id", "workUnitId"),
        ("lineage_digest", "lineageDigest"),
    ] {
        if let Some(value) = object.get(snake).or_else(|| object.get(camel)) {
            result.insert(camel.into(), value.clone());
        }
    }
    if result.is_empty() {
        return Err(invalid("provenance", "must contain provenance fields"));
    }
    Ok(Value::Object(result))
}

fn canonical_resource_intent(value: &Value) -> ConcurrencyResult<Value> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("resource_intents", "each intent must be an object"))?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("resource_intent.kind", "must be a string"))?;
    if !matches!(kind, "file" | "directory" | "task" | "schema" | "external") {
        return Err(invalid(
            "resource_intent.kind",
            "is not a known resource kind",
        ));
    }
    let selector = object
        .get("selector")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid("resource_intent.selector", "must be non-empty"))?;
    let access = object
        .get("access")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("resource_intent.access", "must be a string"))?;
    if !matches!(access, "read" | "propose_write") {
        return Err(invalid(
            "resource_intent.access",
            "is not a known resource access",
        ));
    }
    let mut result = Map::new();
    result.insert("kind".into(), Value::String(kind.into()));
    result.insert("selector".into(), Value::String(truncate(selector, 2_000)));
    result.insert("access".into(), Value::String(access.into()));
    if let Some(rationale) = object.get("rationale")
        && let Some(rationale) = rationale.as_str()
    {
        result.insert(
            "rationale".into(),
            Value::String(truncate(rationale, 2_000)),
        );
    }
    Ok(Value::Object(result))
}

fn canonical_evidence(value: &Value) -> ConcurrencyResult<Value> {
    if let Some(reference) = value.as_str() {
        return Ok(json!({"ref": truncate(reference, 2_000)}));
    }
    let object = value
        .as_object()
        .ok_or_else(|| invalid("evidence", "must be a reference string or object"))?;
    let mut result = Map::new();
    for key in ["id", "kind", "ref", "uri", "path", "digest", "label"] {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            result.insert(key.into(), Value::String(truncate(value, 2_000)));
        }
    }
    if let Some(metadata) = object.get("metadata") {
        result.insert("metadata".into(), metadata.clone());
    }
    if result.is_empty() {
        return Err(invalid("evidence", "must contain a reference field"));
    }
    Ok(Value::Object(result))
}

fn canonical_validator(value: Value) -> ConcurrencyResult<Value> {
    let object = as_object(value, "validator")?;
    let mut result = Map::new();
    for (snake, camel) in [
        ("session_id", "sessionId"),
        ("agent_id", "agentId"),
        ("model_id", "modelId"),
        ("lineage_digest", "lineageDigest"),
    ] {
        if let Some(value) = object.get(snake).or_else(|| object.get(camel)) {
            result.insert(camel.into(), value.clone());
        }
    }
    if result.get("lineageDigest").is_none() {
        return Err(invalid("validator.lineageDigest", "is required"));
    }
    Ok(Value::Object(result))
}

fn canonical_assessment(value: &Value) -> ConcurrencyResult<Value> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("assessments", "each assessment must be an object"))?;
    let candidate_id = object
        .get("candidate_id")
        .or_else(|| object.get("candidateId"))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("assessment.candidateId", "is required"))?;
    let mut result = Map::new();
    result.insert("candidateId".into(), Value::String(candidate_id.into()));
    for key in ["eligible", "approve"] {
        let value = object
            .get(key)
            .and_then(Value::as_bool)
            .ok_or_else(|| invalid(key, "must be a boolean"))?;
        result.insert(key.into(), Value::Bool(value));
    }
    for (snake, camel) in [
        ("acceptance_score", "acceptanceScore"),
        ("risk_score", "riskScore"),
        ("complexity_score", "complexityScore"),
    ] {
        let value = object
            .get(snake)
            .or_else(|| object.get(camel))
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid(camel, "must be a non-negative integer"))?;
        result.insert(camel.into(), json!(value));
    }
    if let Some(notes) = object.get("notes") {
        let notes = notes
            .as_array()
            .ok_or_else(|| invalid("assessment.notes", "must be an array"))?;
        result.insert(
            "notes".into(),
            Value::Array(
                notes
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|note| Value::String(truncate(note, 2_000)))
                    .collect(),
            ),
        );
    } else {
        result.insert("notes".into(), Value::Array(Vec::new()));
    }
    Ok(Value::Object(result))
}

fn validate_policy(value: &Value) -> ConcurrencyResult<()> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("policy", "must be an object"))?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("policy.kind", "is required"))?;
    match kind {
        "exclusive" => Ok(()),
        "speculative" => {
            let max = object
                .get("max_candidates")
                .or_else(|| object.get("maxCandidates"))
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("policy.maxCandidates", "is required"))?;
            if max == 0 {
                Err(invalid("policy.maxCandidates", "must be greater than zero"))
            } else {
                Ok(())
            }
        }
        "ensemble" => {
            let count = object
                .get("candidate_count")
                .or_else(|| object.get("candidateCount"))
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("policy.candidateCount", "is required"))?;
            let quorum = object
                .get("quorum")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("policy.quorum", "is required"))?;
            if count <= 1 || quorum == 0 || quorum > count {
                Err(invalid(
                    "policy.ensemble",
                    "candidateCount must exceed one and quorum must be within it",
                ))
            } else {
                Ok(())
            }
        }
        _ => Err(invalid("policy.kind", "is not a known concurrency policy")),
    }
}

fn validate_decision(value: &Value) -> ConcurrencyResult<()> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("decision", "must be an object"))?;
    let outcome = object
        .get("outcome")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("decision.outcome", "is required"))?;
    if matches!(outcome, "select" | "synthesize" | "blocked") {
        Ok(())
    } else {
        Err(invalid(
            "decision.outcome",
            "is not a known adjudication outcome",
        ))
    }
}

fn validate_candidate_set_state(state: &str) -> ConcurrencyResult<()> {
    if matches!(
        state,
        "open" | "adjudicating" | "decided" | "promoting" | "completed" | "cancelled"
    ) {
        Ok(())
    } else {
        Err(invalid("state", "is not a known candidate set state"))
    }
}

fn validate_candidate_state(state: &str) -> ConcurrencyResult<()> {
    if matches!(
        state,
        "registered"
            | "authoring"
            | "submitted"
            | "validating"
            | "eligible"
            | "rejected"
            | "failed"
            | "selected"
            | "superseded"
            | "promoted"
    ) {
        Ok(())
    } else {
        Err(invalid("state", "is not a known candidate state"))
    }
}

fn validate_promotion_state(state: &str) -> ConcurrencyResult<()> {
    if matches!(
        state,
        "prepared" | "ref_updated" | "finalized" | "aborted" | "conflicted"
    ) {
        Ok(())
    } else {
        Err(invalid("state", "is not a known promotion state"))
    }
}

fn candidate_state_is_immutable(state: &str) -> bool {
    matches!(
        state,
        "submitted"
            | "validating"
            | "eligible"
            | "rejected"
            | "failed"
            | "selected"
            | "superseded"
            | "promoted"
    )
}

fn required_string_field(value: &Value, key: &str) -> ConcurrencyResult<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| invalid(key, "must be a string"))
}

fn string_field(value: &Value, key: &str) -> ConcurrencyResult<String> {
    required_string_field(value, key)
}

fn json_text(value: &Value) -> ConcurrencyResult<String> {
    Ok(serde_json::to_string(value)?)
}

fn invalid(field: &str, message: &str) -> ConcurrencyStoreError {
    ConcurrencyStoreError::InvalidInput {
        field: field.into(),
        message: message.into(),
    }
}

fn not_found(entity: &str, id: &str) -> ConcurrencyStoreError {
    ConcurrencyStoreError::NotFound {
        entity: entity.into(),
        id: id.into(),
    }
}

fn to_i64(value: usize, field: &str) -> ConcurrencyResult<i64> {
    i64::try_from(value).map_err(|_| invalid(field, "is too large for SQLite"))
}

fn to_i64_u64(value: u64, field: &str) -> ConcurrencyResult<i64> {
    i64::try_from(value).map_err(|_| invalid(field, "is too large for SQLite"))
}

fn from_i64(value: i64, field: &str) -> ConcurrencyResult<u64> {
    u64::try_from(value).map_err(|_| ConcurrencyStoreError::Schema(format!("negative {field}")))
}

fn bounded_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_PROJECTION_LIMIT)
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::json;
    use tempfile::NamedTempFile;

    fn temp_path() -> PathBuf {
        let file = NamedTempFile::new().expect("temp database");
        let path = file.path().to_path_buf();
        std::mem::forget(file);
        path
    }

    fn store() -> (ConcurrencyStore, PathBuf) {
        let path = temp_path();
        let store = ConcurrencyStore::open_path(&path, "/repo/concurrency").unwrap();
        (store, path)
    }

    fn candidate_set() -> Value {
        json!({
            "id": "cset_1",
            "projectId": "proj_1",
            "taskId": "task_1",
            "baseRevision": 0,
            "baseCommit": "base-commit",
            "acceptanceDigest": "acceptance-digest",
            "policy": {"kind": "ensemble", "candidateCount": 2, "quorum": 1},
            "policyVersion": 1,
            "state": "open",
            "createdAt": "2026-07-31T00:00:00Z",
            "updatedAt": "2026-07-31T00:00:00Z"
        })
    }

    fn candidate(id: &str, state: &str) -> Value {
        json!({
            "id": id,
            "candidateSetId": "cset_1",
            "state": state,
            "baseCommit": "base-commit",
            "resultCommit": format!("result-{id}"),
            "diffDigest": format!("digest-{id}"),
            "summary": "bounded candidate summary",
            "provenance": {
                "sessionId": format!("session-{id}"),
                "agentId": format!("agent-{id}"),
                "modelId": "model",
                "workUnitId": format!("wu-{id}"),
                "lineageDigest": format!("lineage-{id}")
            },
            "resourceIntents": [{
                "kind": "file",
                "selector": "crates/jcode-tasker-pi/src/concurrency_store.rs",
                "access": "propose_write",
                "rationale": "implement persistence"
            }],
            "createdAt": "2026-07-31T00:00:00Z",
            "updatedAt": "2026-07-31T00:00:00Z"
        })
    }

    fn promotion() -> Value {
        json!({
            "id": "promote_1",
            "projectId": "proj_1",
            "taskId": "task_1",
            "candidateSetId": "cset_1",
            "candidateId": "cand_1",
            "expectedRevision": 2,
            "expectedCommit": "base-commit",
            "targetCommit": "result-cand_1",
            "canonicalRef": "refs/heads/main",
            "state": "prepared",
            "createdAt": "2026-07-31T00:00:00Z"
        })
    }

    #[test]
    fn migration_is_additive_and_idempotent() {
        let path = temp_path();
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "CREATE TABLE tasks (id TEXT PRIMARY KEY, title TEXT NOT NULL)",
            [],
        )
        .unwrap();
        let before: Vec<String> = conn
            .prepare("PRAGMA table_info(tasks)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        drop(conn);

        let store = ConcurrencyStore::open_path(&path, "/repo/concurrency").unwrap();
        assert_eq!(store.schema_version().unwrap(), 1);
        drop(store);
        let store = ConcurrencyStore::open_path(&path, "/repo/concurrency").unwrap();
        assert_eq!(store.schema_version().unwrap(), 1);
        let conn = Connection::open(&path).unwrap();
        let after: Vec<String> = conn
            .prepare("PRAGMA table_info(tasks)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn legacy_list_scoped_project_identity_migrates_to_database_partition_identity() {
        let path = temp_path();
        let partition = ProjectPartition::with_db_path_and_list_id(
            &path,
            "/repo/concurrency",
            "list_migration",
        );
        let legacy_id = legacy_native_project_id_for_list(&partition.list_id).to_string();
        let current_id = native_project_id_for_partition(&partition).to_string();
        assert_ne!(legacy_id, current_id);

        let mut conn = Connection::open(&path).unwrap();
        configure_and_migrate(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO concurrency_project_revisions(project_id, revision, updated_at)
             VALUES (?1, 7, '2026-08-09T00:00:00Z')",
            [&legacy_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO candidate_sets(
                 id, project_id, task_id, base_revision, base_commit, acceptance_digest,
                 policy_json, policy_version, state, created_at, updated_at
             ) VALUES (
                 'cset_legacy', ?1, 'task_legacy', 7, 'base', 'acceptance',
                 '{\"kind\":\"speculative\",\"maxCandidates\":2}', 1, 'open',
                 '2026-08-09T00:00:00Z', '2026-08-09T00:00:00Z'
             )",
            [&legacy_id],
        )
        .unwrap();
        drop(conn);

        let store = ConcurrencyStore::open(partition).unwrap();
        assert_eq!(store.current_revision(&current_id).unwrap(), 7);
        drop(store);

        let conn = Connection::open(&path).unwrap();
        let migrated_project: String = conn
            .query_row(
                "SELECT project_id FROM candidate_sets WHERE id = 'cset_legacy'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(migrated_project, current_id);
        let legacy_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM concurrency_project_revisions WHERE project_id = ?1",
                [&legacy_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(legacy_rows, 0);
    }

    #[test]
    fn mutations_check_project_revision_and_persist_metadata_only() {
        let (mut store, _path) = store();
        let created = store.create_candidate_set(candidate_set(), 0).unwrap();
        assert_eq!(created.revision, 1);
        let stale = store.register_candidate(candidate("cand_1", "selected"), 0);
        assert!(matches!(
            stale,
            Err(ConcurrencyStoreError::RevisionConflict { .. })
        ));
        let registered = store
            .register_candidate(candidate("cand_1", "selected"), 1)
            .unwrap();
        assert_eq!(registered.revision, 2);
        let persisted = store.candidate("cand_1").unwrap().unwrap();
        assert_eq!(persisted["resourceIntents"][0]["kind"], "file");
        assert!(persisted.get("source").is_none());
    }

    #[test]
    fn bounded_projection_is_safe_for_prompt_and_ui_consumers() {
        let (mut store, _path) = store();
        store.create_candidate_set(candidate_set(), 0).unwrap();
        store
            .register_candidate(candidate("cand_1", "selected"), 1)
            .unwrap();
        store
            .register_candidate(candidate("cand_2", "eligible"), 2)
            .unwrap();
        let projection = store.bounded_projection("proj_1", 1).unwrap();
        assert_eq!(projection.limit, 1);
        assert!(projection.truncated);
        assert_eq!(projection.value["candidates"].as_array().unwrap().len(), 1);
        assert_eq!(projection.value["counts"]["candidates"], 2);
        assert_eq!(store.prompt_projection("proj_1", 1).unwrap()["limit"], 1);
    }

    #[test]
    fn promotion_is_single_writer_and_revision_checked() {
        let (mut store, _path) = store();
        store.create_candidate_set(candidate_set(), 0).unwrap();
        store
            .register_candidate(candidate("cand_1", "selected"), 1)
            .unwrap();
        let prepared = store.prepare_promotion(promotion(), 2).unwrap();
        assert_eq!(prepared.revision, 3);
        let other = json!({
            "id": "promote_2",
            "projectId": "proj_1",
            "taskId": "task_1",
            "candidateSetId": "cset_1",
            "candidateId": "cand_1",
            "expectedRevision": 3,
            "expectedCommit": "base-commit",
            "targetCommit": "result-cand_1",
            "canonicalRef": "refs/heads/main"
        });
        assert!(matches!(
            store.prepare_promotion(other, 3),
            Err(ConcurrencyStoreError::PromotionConflict { .. })
        ));
        assert!(
            store
                .mark_promotion_ref_updated("promote_1", "wrong", 3)
                .is_err()
        );
    }

    #[test]
    fn kill_mid_promotion_recovers_from_persisted_target_ref() {
        let (mut store, path) = store();
        store.create_candidate_set(candidate_set(), 0).unwrap();
        store
            .register_candidate(candidate("cand_1", "selected"), 1)
            .unwrap();
        store.prepare_promotion(promotion(), 2).unwrap();
        store
            .mark_promotion_ref_updated("promote_1", "result-cand_1", 3)
            .unwrap();
        drop(store);

        let mut restarted = ConcurrencyStore::open_path(&path, "/repo/concurrency").unwrap();
        let persisted = restarted.promotion_intent("promote_1").unwrap().unwrap();
        assert_eq!(persisted["state"], "ref_updated");
        let recovery = restarted
            .recover_promotion("promote_1", "result-cand_1", 4)
            .unwrap();
        assert_eq!(recovery.action, PromotionRecoveryAction::Finalize);
        assert_eq!(recovery.intent["state"], "finalized");
        assert_eq!(
            restarted.candidate("cand_1").unwrap().unwrap()["state"],
            "promoted"
        );
    }

    #[test]
    fn recovery_at_expected_commit_is_non_mutating_for_prepared_and_ref_updated_intents() {
        let (mut store, path) = store();
        store.create_candidate_set(candidate_set(), 0).unwrap();
        store
            .register_candidate(candidate("cand_1", "selected"), 1)
            .unwrap();
        let prepared = store.prepare_promotion(promotion(), 2).unwrap();

        let retry = store
            .recover_promotion("promote_1", "base-commit", prepared.revision)
            .unwrap();
        assert_eq!(retry.action, PromotionRecoveryAction::Retry);
        assert_eq!(retry.intent["state"], "prepared");
        assert_eq!(retry.revision, prepared.revision);
        assert_eq!(store.current_revision("proj_1").unwrap(), prepared.revision);
        assert!(
            store
                .incomplete_promotions("proj_1", 10)
                .unwrap()
                .iter()
                .any(|intent| intent["id"] == "promote_1")
        );

        let ref_updated = store
            .mark_promotion_ref_updated("promote_1", "result-cand_1", prepared.revision)
            .unwrap();
        drop(store);

        let mut restarted = ConcurrencyStore::open_path(&path, "/repo/concurrency").unwrap();
        let retry = restarted
            .recover_promotion("promote_1", "base-commit", ref_updated.revision)
            .unwrap();
        assert_eq!(retry.action, PromotionRecoveryAction::Retry);
        assert_eq!(retry.intent["state"], "ref_updated");
        assert_eq!(retry.revision, ref_updated.revision);
        assert_eq!(
            restarted.current_revision("proj_1").unwrap(),
            ref_updated.revision
        );
        assert!(
            restarted
                .incomplete_promotions("proj_1", 10)
                .unwrap()
                .iter()
                .any(|intent| intent["id"] == "promote_1")
        );
    }

    #[test]
    fn unexpected_canonical_commit_is_persisted_as_conflict_and_rollbackable() {
        let (mut store, _path) = store();
        store.create_candidate_set(candidate_set(), 0).unwrap();
        store
            .register_candidate(candidate("cand_1", "selected"), 1)
            .unwrap();
        store.prepare_promotion(promotion(), 2).unwrap();
        let recovery = store
            .recover_promotion("promote_1", "foreign-commit", 3)
            .unwrap();
        assert_eq!(recovery.action, PromotionRecoveryAction::Conflict);
        assert_eq!(recovery.intent["state"], "conflicted");
        store
            .rollback_promotion("promote_1", "operator chose rollback", 4)
            .unwrap();
        assert_eq!(
            store.promotion_intent("promote_1").unwrap().unwrap()["state"],
            "aborted"
        );
    }
}
