//! Composition layer for bounded Tasker candidate lanes.
//!
//! This crate owns the launch contract and lane lifecycle. It does not spawn
//! agents or create worktrees. Instead, it emits typed lane descriptors for a
//! downstream executor and persists the lane state beside the landed
//! `ConcurrencyStore` records.

use chrono::Utc;
use jcode_tasker_git::{CandidateRef, CleanupReport, GitCandidateAdapter, IsolationProof};
use jcode_tasker_pi::{ConcurrencyStore, ConcurrencyStoreError, ProjectPartition};
use jcode_tasker_rounds::{RoundError, RoundOpened, RoundOrchestrator};
use jcode_tasker_types::{
    Candidate, CandidateId, CandidateProvenance, CandidateSet, CandidateSetId, CandidateSetState,
    CandidateState, ConcurrencyPolicy, ProjectId, ResourceIntent, TaskId, TaskerError,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use thiserror::Error;

const LANE_PROJECTION_LIMIT: usize = 500;
const LANE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS orchestration_lanes (
    candidate_set_id TEXT NOT NULL,
    candidate_id TEXT PRIMARY KEY NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    candidate_ref TEXT NOT NULL,
    state TEXT NOT NULL,
    reason TEXT,
    acceptance_json TEXT NOT NULL CHECK (json_valid(acceptance_json)),
    worktree_json TEXT NOT NULL CHECK (json_valid(worktree_json)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS orchestration_lanes_by_set
    ON orchestration_lanes(candidate_set_id, ordinal, candidate_id);
"#;

/// A validation command that a downstream candidate executor must run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationCommand {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

impl ValidationCommand {
    pub fn new(
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    fn validate(&self) -> Result<(), OrchestrationError> {
        if self.program.trim().is_empty() {
            return Err(OrchestrationError::InvalidContract(
                "validation command program must not be empty".into(),
            ));
        }
        Ok(())
    }
}

/// The acceptance contract shared by every candidate lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceContract {
    pub validation_commands: Vec<ValidationCommand>,
    pub acceptance_criteria: String,
    #[serde(default)]
    pub resource_intents: Vec<ResourceIntent>,
}

impl AcceptanceContract {
    pub fn new(
        validation_commands: Vec<ValidationCommand>,
        acceptance_criteria: impl Into<String>,
        resource_intents: Vec<ResourceIntent>,
    ) -> Self {
        Self {
            validation_commands,
            acceptance_criteria: acceptance_criteria.into(),
            resource_intents,
        }
    }

    pub fn validate(&self) -> Result<(), OrchestrationError> {
        if self.validation_commands.is_empty() {
            return Err(OrchestrationError::InvalidContract(
                "at least one validation command is required".into(),
            ));
        }
        if self.acceptance_criteria.trim().is_empty() {
            return Err(OrchestrationError::InvalidContract(
                "acceptance criteria must not be empty".into(),
            ));
        }
        for command in &self.validation_commands {
            command.validate()?;
        }
        Ok(())
    }

    /// Return a stable digest suitable for `CandidateSet.acceptance_digest`.
    pub fn digest(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("acceptance contract is serializable");
        let digest = Sha1::digest(bytes);
        format!("sha1:{digest:x}")
    }
}

/// Shared provenance fields used to derive each lane's candidate provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceTemplate {
    pub session_id: String,
    pub agent_id: String,
    pub model_id: Option<String>,
    pub lineage_digest: Option<String>,
    pub work_unit_prefix: Option<String>,
}

impl ProvenanceTemplate {
    pub fn new(session_id: impl Into<String>, agent_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            agent_id: agent_id.into(),
            model_id: None,
            lineage_digest: None,
            work_unit_prefix: None,
        }
    }

    fn candidate(&self, candidate_set_id: CandidateSetId, ordinal: usize) -> CandidateProvenance {
        let prefix = self.work_unit_prefix.as_deref().unwrap_or("candidate-lane");
        CandidateProvenance {
            session_id: self.session_id.clone(),
            agent_id: self.agent_id.clone(),
            model_id: self.model_id.clone(),
            work_unit_id: Some(format!("{prefix}-{candidate_set_id}-{ordinal}")),
            lineage_digest: self.lineage_digest.clone(),
        }
    }
}

/// The downstream executor's typed, non-spawning worktree description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeDescriptor {
    pub path: PathBuf,
    pub candidate_ref: String,
    pub base_commit: String,
    pub candidate_id: CandidateId,
}

/// The bounded lifecycle owned by this composition layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneState {
    Pending,
    InProgress,
    Submitted,
    Failed,
    Abandoned,
    TimedOut,
}

impl LaneState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Submitted => "submitted",
            Self::Failed => "failed",
            Self::Abandoned => "abandoned",
            Self::TimedOut => "timed_out",
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Submitted | Self::Failed | Self::Abandoned | Self::TimedOut
        )
    }

    fn can_transition_to(self, next: Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (
                    Self::Pending,
                    Self::InProgress | Self::Failed | Self::Abandoned | Self::TimedOut
                ) | (
                    Self::InProgress,
                    Self::Submitted | Self::Failed | Self::Abandoned | Self::TimedOut
                )
            )
    }
}

impl std::str::FromStr for LaneState {
    type Err = OrchestrationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "submitted" => Ok(Self::Submitted),
            "failed" => Ok(Self::Failed),
            "abandoned" => Ok(Self::Abandoned),
            "timed_out" => Ok(Self::TimedOut),
            other => Err(OrchestrationError::InvalidLaneState(other.into())),
        }
    }
}

/// A lane specification emitted for another system to execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateLaneSpec {
    pub candidate_id: CandidateId,
    pub candidate_ref: String,
    pub worktree: WorktreeDescriptor,
    pub acceptance: AcceptanceContract,
    pub state: LaneState,
}

/// A submission produced by a downstream candidate executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateSubmission {
    pub result_commit: String,
    pub diff_digest: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub evidence: Vec<Value>,
}

impl CandidateSubmission {
    pub fn new(result_commit: impl Into<String>) -> Self {
        Self {
            result_commit: result_commit.into(),
            diff_digest: None,
            summary: None,
            evidence: Vec::new(),
        }
    }
}

/// Inputs for opening and provisioning a candidate set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCandidateSetRequest {
    pub project_id: ProjectId,
    pub task_id: TaskId,
    pub base_commit: String,
    pub policy: ConcurrencyPolicy,
    pub lane_count: u16,
    pub acceptance: AcceptanceContract,
    pub provenance: ProvenanceTemplate,
    pub worktree_root: PathBuf,
    pub expected_revision: Option<u64>,
    pub policy_version: u32,
}

impl OpenCandidateSetRequest {
    // Each argument maps directly to one launch-contract field. Keep this
    // constructor explicit so callers cannot accidentally omit a lane input.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_id: ProjectId,
        task_id: TaskId,
        base_commit: impl Into<String>,
        policy: ConcurrencyPolicy,
        lane_count: u16,
        acceptance: AcceptanceContract,
        provenance: ProvenanceTemplate,
        worktree_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            project_id,
            task_id,
            base_commit: base_commit.into(),
            policy,
            lane_count,
            acceptance,
            provenance,
            worktree_root: worktree_root.into(),
            expected_revision: None,
            policy_version: 1,
        }
    }
}

/// The durable result of opening a candidate set and provisioning its lanes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSetOpened {
    pub candidate_set: CandidateSet,
    pub lanes: Vec<CandidateLaneSpec>,
    pub isolation: IsolationProof,
    pub revision: u64,
}

/// A CAS-safe lifecycle transition receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneTransitionReceipt {
    pub candidate_id: CandidateId,
    pub from: LaneState,
    pub to: LaneState,
    pub revision: u64,
}

/// An abandonment/timeout receipt including the observed Git cleanup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneAbandoned {
    pub transition: LaneTransitionReceipt,
    pub cleanup: CleanupReport,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneCounts {
    pub pending: usize,
    pub in_progress: usize,
    pub submitted: usize,
    pub failed: usize,
    pub abandoned: usize,
    pub timed_out: usize,
}

/// A bounded status read model for coordinators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneStatusProjection {
    pub candidate_id: CandidateId,
    pub candidate_ref: String,
    pub worktree: WorktreeDescriptor,
    pub acceptance: AcceptanceContract,
    pub state: LaneState,
    pub reason: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationStatusProjection {
    pub candidate_set_id: CandidateSetId,
    pub project_id: ProjectId,
    pub revision: u64,
    pub lanes: Vec<LaneStatusProjection>,
    pub counts: LaneCounts,
    pub store_projection: Value,
    pub limit: usize,
    pub truncated: bool,
}

/// The round handoff keeps a live `RoundOrchestrator` over the same durable DB.
#[derive(Debug)]
pub struct RoundHandoff {
    pub opened: RoundOpened,
    pub round: RoundOrchestrator,
}

#[derive(Debug, Error)]
pub enum OrchestrationError {
    #[error("concurrency policy validation failed: {0}")]
    Policy(#[source] TaskerError),
    #[error("exclusive policy cannot provision parallel candidate lanes")]
    ExclusivePolicy,
    #[error("requested {requested} candidate lanes but policy limit is {limit}")]
    CandidateLimit { requested: u16, limit: u16 },
    #[error("invalid acceptance contract: {0}")]
    InvalidContract(String),
    #[error("invalid lane state: {0}")]
    InvalidLaneState(String),
    #[error("invalid lane transition from {from:?} to {to:?} for candidate {candidate_id}")]
    InvalidTransition {
        candidate_id: String,
        from: LaneState,
        to: LaneState,
    },
    #[error("lane {candidate_id} was not found")]
    LaneNotFound { candidate_id: String },
    #[error("candidate set {candidate_set_id} was not found")]
    CandidateSetNotFound { candidate_set_id: String },
    #[error("candidate set {candidate_set_id} has unfinished lanes: {states}")]
    IncompleteLanes {
        candidate_set_id: String,
        states: String,
    },
    #[error("candidate set {candidate_set_id} has no submitted lanes")]
    NoSubmittedLanes { candidate_set_id: String },
    #[error("revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("in-memory ConcurrencyStore is not supported by durable orchestration")]
    InMemoryUnsupported,
    #[error("Git candidate adapter error: {message}")]
    Git { message: String },
    #[error("concurrency store error: {0}")]
    Store(#[from] ConcurrencyStoreError),
    #[error("round orchestration error: {0}")]
    Round(#[from] RoundError),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

fn git_error(error: impl std::fmt::Display) -> OrchestrationError {
    OrchestrationError::Git {
        message: error.to_string(),
    }
}

#[derive(Debug, Clone)]
struct LaneRow {
    candidate_set_id: CandidateSetId,
    candidate_id: CandidateId,
    candidate_ref: String,
    state: LaneState,
    reason: Option<String>,
    acceptance: AcceptanceContract,
    worktree: WorktreeDescriptor,
    updated_at: String,
}

impl LaneRow {
    fn projection(self) -> LaneStatusProjection {
        LaneStatusProjection {
            candidate_id: self.candidate_id,
            candidate_ref: self.candidate_ref,
            worktree: self.worktree,
            acceptance: self.acceptance,
            state: self.state,
            reason: self.reason,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug)]
struct LaneStore {
    connection: Connection,
}

impl LaneStore {
    fn open(partition: &ProjectPartition) -> Result<Self, OrchestrationError> {
        if partition.db_path == Path::new(":memory:") {
            return Err(OrchestrationError::InMemoryUnsupported);
        }
        let connection = Connection::open(&partition.db_path)?;
        connection.busy_timeout(std::time::Duration::from_secs(30))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.execute_batch(LANE_SCHEMA)?;
        Ok(Self { connection })
    }

    fn create_lanes(
        &mut self,
        project_id: &str,
        candidate_set_id: CandidateSetId,
        lanes: &[CandidateLaneSpec],
        expected_revision: u64,
    ) -> Result<u64, OrchestrationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_revision(&transaction, project_id, expected_revision)?;
        let timestamp = Utc::now().to_rfc3339();
        for (ordinal, lane) in lanes.iter().enumerate() {
            transaction.execute(
                "INSERT INTO orchestration_lanes
                    (candidate_set_id, candidate_id, ordinal, candidate_ref, state, reason,
                     acceptance_json, worktree_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8, ?8)",
                params![
                    candidate_set_id.to_string(),
                    lane.candidate_id.to_string(),
                    i64::try_from(ordinal).map_err(|_| {
                        OrchestrationError::InvalidContract("lane ordinal overflow".into())
                    })?,
                    lane.candidate_ref,
                    lane.state.as_str(),
                    serde_json::to_string(&lane.acceptance)?,
                    serde_json::to_string(&lane.worktree)?,
                    timestamp,
                ],
            )?;
        }
        let revision = bump_revision(&transaction, project_id, expected_revision)?;
        transaction.commit()?;
        Ok(revision)
    }

    fn get(&self, candidate_id: CandidateId) -> Result<LaneRow, OrchestrationError> {
        let row = self
            .connection
            .query_row(
                "SELECT candidate_set_id, candidate_id, candidate_ref, state, reason,
                        acceptance_json, worktree_json, updated_at
                 FROM orchestration_lanes WHERE candidate_id = ?1",
                [candidate_id.to_string()],
                lane_row,
            )
            .optional()?;
        row.ok_or_else(|| OrchestrationError::LaneNotFound {
            candidate_id: candidate_id.to_string(),
        })
    }

    fn list(
        &self,
        candidate_set_id: CandidateSetId,
        limit: usize,
    ) -> Result<(Vec<LaneRow>, bool), OrchestrationError> {
        let limit = limit.clamp(1, LANE_PROJECTION_LIMIT);
        let total: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM orchestration_lanes WHERE candidate_set_id = ?1",
            [candidate_set_id.to_string()],
            |row| row.get(0),
        )?;
        let rows = self
            .connection
            .prepare(
                "SELECT candidate_set_id, candidate_id, candidate_ref, state, reason,
                        acceptance_json, worktree_json, updated_at
                 FROM orchestration_lanes
                 WHERE candidate_set_id = ?1 ORDER BY ordinal ASC, candidate_id ASC LIMIT ?2",
            )?
            .query_map(
                params![
                    candidate_set_id.to_string(),
                    i64::try_from(limit).unwrap_or(i64::MAX)
                ],
                lane_row,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((rows, total > i64::try_from(limit).unwrap_or(i64::MAX)))
    }

    fn counts(&self, candidate_set_id: CandidateSetId) -> Result<LaneCounts, OrchestrationError> {
        let mut statement = self.connection.prepare(
            "SELECT state, COUNT(*) FROM orchestration_lanes
             WHERE candidate_set_id = ?1 GROUP BY state",
        )?;
        let rows = statement
            .query_map([candidate_set_id.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut counts = LaneCounts::default();
        for (state, count) in rows {
            let count = usize::try_from(count)
                .map_err(|_| OrchestrationError::InvalidContract("negative lane count".into()))?;
            match state.parse::<LaneState>()? {
                LaneState::Pending => counts.pending = count,
                LaneState::InProgress => counts.in_progress = count,
                LaneState::Submitted => counts.submitted = count,
                LaneState::Failed => counts.failed = count,
                LaneState::Abandoned => counts.abandoned = count,
                LaneState::TimedOut => counts.timed_out = count,
            }
        }
        Ok(counts)
    }

    fn transition(
        &mut self,
        project_id: &str,
        candidate_id: CandidateId,
        expected_revision: u64,
        target: LaneState,
        reason: Option<&str>,
    ) -> Result<LaneTransitionReceipt, OrchestrationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_revision(&transaction, project_id, expected_revision)?;
        let existing = transaction
            .query_row(
                "SELECT candidate_set_id, state FROM orchestration_lanes WHERE candidate_id = ?1",
                [candidate_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((candidate_set_id, current_value)) = existing else {
            return Err(OrchestrationError::LaneNotFound {
                candidate_id: candidate_id.to_string(),
            });
        };
        let current = current_value.parse::<LaneState>()?;
        if !current.can_transition_to(target) {
            return Err(OrchestrationError::InvalidTransition {
                candidate_id: candidate_id.to_string(),
                from: current,
                to: target,
            });
        }

        let canonical_state = canonical_candidate_state(target);
        let changed_candidate = transaction.execute(
            "UPDATE candidates SET state = ?1, updated_at = ?2
             WHERE id = ?3 AND candidate_set_id = ?4",
            params![
                canonical_state,
                Utc::now().to_rfc3339(),
                candidate_id.to_string(),
                candidate_set_id
            ],
        )?;
        if changed_candidate != 1 {
            return Err(OrchestrationError::LaneNotFound {
                candidate_id: candidate_id.to_string(),
            });
        }
        let timestamp = Utc::now().to_rfc3339();
        transaction.execute(
            "UPDATE orchestration_lanes SET state = ?1, reason = ?2, updated_at = ?3
             WHERE candidate_id = ?4",
            params![target.as_str(), reason, timestamp, candidate_id.to_string()],
        )?;
        let revision = bump_revision(&transaction, project_id, expected_revision)?;
        transaction.commit()?;
        Ok(LaneTransitionReceipt {
            candidate_id,
            from: current,
            to: target,
            revision,
        })
    }
}

fn lane_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LaneRow> {
    let candidate_set_id = row
        .get::<_, String>(0)?
        .parse()
        .map_err(|error: TaskerError| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let candidate_id = row
        .get::<_, String>(1)?
        .parse()
        .map_err(|error: TaskerError| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let state = row
        .get::<_, String>(3)?
        .parse()
        .map_err(|error: OrchestrationError| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let acceptance = serde_json::from_str::<AcceptanceContract>(&row.get::<_, String>(5)?)
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let worktree =
        serde_json::from_str::<WorktreeDescriptor>(&row.get::<_, String>(6)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(LaneRow {
        candidate_set_id,
        candidate_id,
        candidate_ref: row.get(2)?,
        state,
        reason: row.get(4)?,
        acceptance,
        worktree,
        updated_at: row.get(7)?,
    })
}

fn canonical_candidate_state(state: LaneState) -> &'static str {
    match state {
        LaneState::Pending => "registered",
        LaneState::InProgress => "authoring",
        LaneState::Submitted => "submitted",
        LaneState::Failed | LaneState::TimedOut => "failed",
        // The landed ConcurrencyStore has no `abandoned` candidate state. A
        // rejected candidate is the durable terminal equivalent; the exact
        // orchestration state remains in `orchestration_lanes.state`.
        LaneState::Abandoned => "rejected",
    }
}

fn ensure_revision(
    transaction: &Transaction<'_>,
    project_id: &str,
    expected: u64,
) -> Result<(), OrchestrationError> {
    transaction.execute(
        "INSERT OR IGNORE INTO concurrency_project_revisions
            (project_id, revision, updated_at) VALUES (?1, 0, ?2)",
        params![project_id, Utc::now().to_rfc3339()],
    )?;
    let actual: i64 = transaction.query_row(
        "SELECT revision FROM concurrency_project_revisions WHERE project_id = ?1",
        [project_id],
        |row| row.get(0),
    )?;
    let actual = u64::try_from(actual)
        .map_err(|_| OrchestrationError::InvalidContract("negative concurrency revision".into()))?;
    if actual != expected {
        return Err(OrchestrationError::RevisionConflict { expected, actual });
    }
    Ok(())
}

fn bump_revision(
    transaction: &Transaction<'_>,
    project_id: &str,
    expected: u64,
) -> Result<u64, OrchestrationError> {
    let changed = transaction.execute(
        "UPDATE concurrency_project_revisions
         SET revision = revision + 1, updated_at = ?1
         WHERE project_id = ?2 AND revision = ?3",
        params![
            Utc::now().to_rfc3339(),
            project_id,
            i64::try_from(expected).unwrap_or(i64::MAX)
        ],
    )?;
    if changed != 1 {
        let actual: i64 = transaction.query_row(
            "SELECT revision FROM concurrency_project_revisions WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )?;
        return Err(OrchestrationError::RevisionConflict {
            expected,
            actual: u64::try_from(actual).unwrap_or(u64::MAX),
        });
    }
    expected
        .checked_add(1)
        .ok_or_else(|| OrchestrationError::InvalidContract("concurrency revision overflow".into()))
}

fn candidate_set_project(
    store: &ConcurrencyStore,
    id: CandidateSetId,
) -> Result<String, OrchestrationError> {
    let value = store.candidate_set(&id.to_string())?.ok_or_else(|| {
        OrchestrationError::CandidateSetNotFound {
            candidate_set_id: id.to_string(),
        }
    })?;
    value
        .get("projectId")
        .or_else(|| value.get("project_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            OrchestrationError::InvalidContract("candidate set is missing projectId".into())
        })
}

fn candidate_value(candidate: &Candidate) -> Result<Value, OrchestrationError> {
    let mut value = serde_json::to_value(candidate)?;
    if let Some(object) = value.as_object_mut() {
        object.retain(|_, value| !value.is_null());
    }
    Ok(value)
}

/// The bounded candidate-lane orchestrator.
#[derive(Debug)]
pub struct CandidateOrchestrator {
    store: ConcurrencyStore,
    git: GitCandidateAdapter,
    lanes: LaneStore,
}

impl CandidateOrchestrator {
    pub fn new(
        store: ConcurrencyStore,
        git: GitCandidateAdapter,
    ) -> Result<Self, OrchestrationError> {
        let lanes = LaneStore::open(store.partition())?;
        Ok(Self { store, git, lanes })
    }

    pub fn store(&self) -> &ConcurrencyStore {
        &self.store
    }

    pub fn git(&self) -> &GitCandidateAdapter {
        &self.git
    }

    /// Open a set and emit N isolated lane specs without spawning workers.
    pub fn open_candidate_set(
        &mut self,
        request: OpenCandidateSetRequest,
    ) -> Result<CandidateSetOpened, OrchestrationError> {
        request
            .policy
            .validate()
            .map_err(OrchestrationError::Policy)?;
        if matches!(&request.policy, ConcurrencyPolicy::Exclusive) {
            return Err(OrchestrationError::ExclusivePolicy);
        }
        request.acceptance.validate()?;
        let candidate_limit = request.policy.candidate_limit();
        if request.lane_count == 0 || request.lane_count > candidate_limit {
            return Err(OrchestrationError::CandidateLimit {
                requested: request.lane_count,
                limit: candidate_limit,
            });
        }
        if let ConcurrencyPolicy::Ensemble {
            candidate_count, ..
        } = &request.policy
            && request.lane_count != *candidate_count
        {
            return Err(OrchestrationError::CandidateLimit {
                requested: request.lane_count,
                limit: *candidate_count,
            });
        }

        let base_revision = self
            .store
            .current_revision(&request.project_id.to_string())?;
        if let Some(expected_revision) = request.expected_revision
            && expected_revision != base_revision
        {
            return Err(OrchestrationError::RevisionConflict {
                expected: expected_revision,
                actual: base_revision,
            });
        }
        let candidate_set_id = CandidateSetId::new();
        let created_at = Utc::now();
        let candidate_set = CandidateSet {
            id: candidate_set_id,
            project_id: request.project_id,
            task_id: request.task_id,
            base_revision: jcode_tasker_types::ProjectRevision(base_revision),
            base_commit: request.base_commit.clone(),
            acceptance_digest: request.acceptance.digest(),
            policy: request.policy.clone(),
            policy_version: request.policy_version,
            state: CandidateSetState::Open,
            created_at,
            updated_at: created_at,
        };

        let mut candidate_refs = Vec::with_capacity(usize::from(request.lane_count));
        let mut lanes = Vec::with_capacity(usize::from(request.lane_count));
        for _ in 0..usize::from(request.lane_count) {
            let candidate_id = CandidateId::new();
            let candidate_ref = match self.git.create_candidate_ref(
                candidate_set_id,
                candidate_id,
                &request.base_commit,
            ) {
                Ok(candidate_ref) => candidate_ref,
                Err(error) => {
                    for existing in &candidate_refs {
                        let _ = self.git.cleanup_abandoned_candidate(existing);
                    }
                    return Err(git_error(error));
                }
            };
            candidate_refs.push(candidate_ref.clone());
            let worktree = WorktreeDescriptor {
                path: request.worktree_root.join(format!("lane-{candidate_id}")),
                candidate_ref: candidate_ref.to_string(),
                base_commit: request.base_commit.clone(),
                candidate_id,
            };
            lanes.push(CandidateLaneSpec {
                candidate_id,
                candidate_ref: candidate_ref.to_string(),
                worktree,
                acceptance: request.acceptance.clone(),
                state: LaneState::Pending,
            });
        }

        let isolation = match self.git.assert_isolated() {
            Ok(proof) => proof,
            Err(error) => {
                for candidate_ref in &candidate_refs {
                    let _ = self.git.cleanup_abandoned_candidate(candidate_ref);
                }
                return Err(git_error(error));
            }
        };

        let project_id = request.project_id.to_string();
        let mut revision = match self
            .store
            .create_candidate_set(&candidate_set, base_revision)
        {
            Ok(mutation) => mutation.revision,
            Err(error) => {
                for candidate_ref in &candidate_refs {
                    let _ = self.git.cleanup_abandoned_candidate(candidate_ref);
                }
                return Err(error.into());
            }
        };

        for (ordinal, lane) in lanes.iter().enumerate() {
            let candidate = Candidate {
                id: lane.candidate_id,
                candidate_set_id,
                state: CandidateState::Registered,
                base_commit: request.base_commit.clone(),
                result_commit: None,
                diff_digest: None,
                summary: None,
                provenance: request.provenance.candidate(candidate_set_id, ordinal),
                resource_intents: request.acceptance.resource_intents.clone(),
                supersedes_candidate_id: None,
                created_at,
                updated_at: created_at,
                submitted_at: None,
            };
            match self
                .store
                .register_candidate(candidate_value(&candidate)?, revision)
            {
                Ok(mutation) => revision = mutation.revision,
                Err(error) => {
                    for candidate_ref in &candidate_refs {
                        let _ = self.git.cleanup_abandoned_candidate(candidate_ref);
                    }
                    let _ = self.store.set_candidate_set_state(
                        &candidate_set_id.to_string(),
                        "cancelled",
                        self.store.current_revision(&project_id).unwrap_or(revision),
                    );
                    return Err(error.into());
                }
            }
        }

        revision = self
            .lanes
            .create_lanes(&project_id, candidate_set_id, &lanes, revision)?;
        Ok(CandidateSetOpened {
            candidate_set,
            lanes,
            isolation,
            revision,
        })
    }

    pub fn start_lane(
        &mut self,
        candidate_id: CandidateId,
        expected_revision: u64,
    ) -> Result<LaneTransitionReceipt, OrchestrationError> {
        self.transition(candidate_id, expected_revision, LaneState::InProgress, None)
    }

    pub fn fail_lane(
        &mut self,
        candidate_id: CandidateId,
        expected_revision: u64,
        reason: impl Into<String>,
    ) -> Result<LaneTransitionReceipt, OrchestrationError> {
        let reason = reason.into();
        self.transition(
            candidate_id,
            expected_revision,
            LaneState::Failed,
            Some(reason),
        )
    }

    pub fn abandon_lane(
        &mut self,
        candidate_id: CandidateId,
        expected_revision: u64,
        reason: impl Into<String>,
    ) -> Result<LaneAbandoned, OrchestrationError> {
        self.abandon_or_timeout(
            candidate_id,
            expected_revision,
            LaneState::Abandoned,
            reason,
        )
    }

    pub fn timeout_lane(
        &mut self,
        candidate_id: CandidateId,
        expected_revision: u64,
        reason: impl Into<String>,
    ) -> Result<LaneAbandoned, OrchestrationError> {
        self.abandon_or_timeout(candidate_id, expected_revision, LaneState::TimedOut, reason)
    }

    pub fn submit_lane(
        &mut self,
        candidate_id: CandidateId,
        expected_revision: u64,
        submission: CandidateSubmission,
    ) -> Result<LaneTransitionReceipt, OrchestrationError> {
        let lane = self.lanes.get(candidate_id)?;
        if lane.state != LaneState::InProgress {
            return Err(OrchestrationError::InvalidTransition {
                candidate_id: candidate_id.to_string(),
                from: lane.state,
                to: LaneState::Submitted,
            });
        }
        let mut persisted_candidate = self
            .store
            .candidate(&candidate_id.to_string())?
            .ok_or_else(|| OrchestrationError::LaneNotFound {
                candidate_id: candidate_id.to_string(),
            })?;
        let object = persisted_candidate.as_object_mut().ok_or_else(|| {
            OrchestrationError::InvalidLaneState("candidate is not an object".into())
        })?;
        let submitted_at = Utc::now().to_rfc3339();
        object.insert("state".into(), json!("submitted"));
        object.insert("resultCommit".into(), json!(submission.result_commit));
        if let Some(diff_digest) = submission.diff_digest {
            object.insert("diffDigest".into(), json!(diff_digest));
        } else {
            object.remove("diffDigest");
        }
        if let Some(summary) = submission.summary {
            object.insert("summary".into(), json!(summary));
        } else {
            object.remove("summary");
        }
        object.insert("updatedAt".into(), json!(submitted_at));
        object.insert("submittedAt".into(), json!(submitted_at));
        object.retain(|_, value| !value.is_null());
        let mutation = self.store.submit_candidate(
            persisted_candidate,
            submission.evidence,
            expected_revision,
        )?;
        self.lanes.transition(
            &candidate_set_project(&self.store, lane.candidate_set_id)?,
            candidate_id,
            mutation.revision,
            LaneState::Submitted,
            None,
        )
    }

    pub fn cleanup_lane(
        &self,
        candidate_id: CandidateId,
    ) -> Result<CleanupReport, OrchestrationError> {
        let lane = self.lanes.get(candidate_id)?;
        if !lane.state.is_terminal() || lane.state == LaneState::Submitted {
            return Err(OrchestrationError::InvalidTransition {
                candidate_id: candidate_id.to_string(),
                from: lane.state,
                to: lane.state,
            });
        }
        let candidate_ref = CandidateRef::parse(&lane.candidate_ref).map_err(git_error)?;
        self.git
            .cleanup_abandoned_candidate(&candidate_ref)
            .map_err(git_error)
    }

    /// Mark terminal failure/abandonment and then remove only candidate refs.
    pub fn abandon_or_timeout(
        &mut self,
        candidate_id: CandidateId,
        expected_revision: u64,
        state: LaneState,
        reason: impl Into<String>,
    ) -> Result<LaneAbandoned, OrchestrationError> {
        debug_assert!(matches!(state, LaneState::Abandoned | LaneState::TimedOut));
        let transition =
            self.transition(candidate_id, expected_revision, state, Some(reason.into()))?;
        let cleanup = self.cleanup_lane(candidate_id)?;
        Ok(LaneAbandoned {
            transition,
            cleanup,
        })
    }

    pub fn handoff_to_round(
        &mut self,
        candidate_set_id: CandidateSetId,
        expected_revision: u64,
    ) -> Result<RoundHandoff, OrchestrationError> {
        let (rows, _) = self.lanes.list(candidate_set_id, LANE_PROJECTION_LIMIT)?;
        if rows.is_empty() {
            return Err(OrchestrationError::CandidateSetNotFound {
                candidate_set_id: candidate_set_id.to_string(),
            });
        }
        let unfinished = rows
            .iter()
            .filter(|row| matches!(row.state, LaneState::Pending | LaneState::InProgress))
            .map(|row| row.state.as_str())
            .collect::<Vec<_>>();
        if !unfinished.is_empty() {
            return Err(OrchestrationError::IncompleteLanes {
                candidate_set_id: candidate_set_id.to_string(),
                states: unfinished.join(", "),
            });
        }
        if !rows.iter().any(|row| row.state == LaneState::Submitted) {
            return Err(OrchestrationError::NoSubmittedLanes {
                candidate_set_id: candidate_set_id.to_string(),
            });
        }
        let store = ConcurrencyStore::open(self.store.partition().clone())?;
        let mut round = RoundOrchestrator::new(store);
        let opened = round.open_round(candidate_set_id, expected_revision)?;
        Ok(RoundHandoff { opened, round })
    }

    pub fn status(
        &self,
        candidate_set_id: CandidateSetId,
        limit: usize,
    ) -> Result<OrchestrationStatusProjection, OrchestrationError> {
        let limit = limit.clamp(1, LANE_PROJECTION_LIMIT);
        let project_id = candidate_set_project(&self.store, candidate_set_id)?;
        let project_id_typed: ProjectId = project_id
            .parse()
            .map_err(|error: TaskerError| OrchestrationError::InvalidContract(error.to_string()))?;
        let (rows, lane_truncated) = self.lanes.list(candidate_set_id, limit)?;
        let counts = self.lanes.counts(candidate_set_id)?;
        let store_projection = self
            .store
            .candidate_set_projection(&candidate_set_id.to_string(), limit)?;
        let store_truncated = store_projection
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(OrchestrationStatusProjection {
            candidate_set_id,
            project_id: project_id_typed,
            revision: self.store.current_revision(&project_id)?,
            lanes: rows.into_iter().map(LaneRow::projection).collect(),
            counts,
            store_projection,
            limit,
            truncated: lane_truncated || store_truncated,
        })
    }

    fn transition(
        &mut self,
        candidate_id: CandidateId,
        expected_revision: u64,
        state: LaneState,
        reason: Option<String>,
    ) -> Result<LaneTransitionReceipt, OrchestrationError> {
        let lane = self.lanes.get(candidate_id)?;
        let project_id = candidate_set_project(&self.store, lane.candidate_set_id)?;
        self.lanes.transition(
            &project_id,
            candidate_id,
            expected_revision,
            state,
            reason.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};
    use tempfile::TempDir;

    fn run_git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .expect("git should start");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("git output should be UTF-8")
            .trim()
            .to_owned()
    }

    fn request_for(
        policy: ConcurrencyPolicy,
        lane_count: u16,
    ) -> (TempDir, CandidateOrchestrator, OpenCandidateSetRequest) {
        let temp = TempDir::new().expect("tempdir");
        run_git(temp.path(), &["init", "--quiet"]);
        run_git(temp.path(), &["config", "user.name", "Tasker Test"]);
        run_git(
            temp.path(),
            &["config", "user.email", "tasker-test@example.invalid"],
        );
        fs::write(temp.path().join("base.txt"), "base\n").expect("write base");
        run_git(temp.path(), &["add", "base.txt"]);
        run_git(temp.path(), &["commit", "--quiet", "-m", "base"]);
        let base_commit = run_git(temp.path(), &["rev-parse", "HEAD"]);
        let db_path = temp.path().join("tasker.sqlite");
        let project_id = ProjectId::new();
        let task_id = TaskId::new();
        let store = ConcurrencyStore::open_path(&db_path, temp.path().to_string_lossy())
            .expect("open concurrency store");
        let git = GitCandidateAdapter::try_new(temp.path()).expect("open git adapter");
        let orchestrator = CandidateOrchestrator::new(store, git).expect("open orchestrator");
        let acceptance = AcceptanceContract::new(
            vec![ValidationCommand::new("cargo", ["test"])],
            "The candidate satisfies the acceptance criteria.",
            Vec::new(),
        );
        let request = OpenCandidateSetRequest::new(
            project_id,
            task_id,
            base_commit,
            policy,
            lane_count,
            acceptance,
            ProvenanceTemplate::new("session-1", "agent-1"),
            temp.path().join("worktrees"),
        );
        (temp, orchestrator, request)
    }

    #[test]
    fn policy_bounds_respect_candidate_limit() {
        let (temp, mut orchestrator, request) =
            request_for(ConcurrencyPolicy::Speculative { max_candidates: 3 }, 3);
        let opened = orchestrator
            .open_candidate_set(request)
            .expect("three speculative lanes are allowed");
        assert_eq!(opened.lanes.len(), 3);
        drop(temp);

        let (temp, mut orchestrator, request) =
            request_for(ConcurrencyPolicy::Speculative { max_candidates: 3 }, 4);
        let error = orchestrator
            .open_candidate_set(request)
            .expect_err("the policy limit must be enforced");
        assert!(matches!(error, OrchestrationError::CandidateLimit { .. }));
        drop(temp);
    }

    #[test]
    fn lanes_have_unique_isolated_refs() {
        let (temp, mut orchestrator, request) = request_for(
            ConcurrencyPolicy::Ensemble {
                candidate_count: 3,
                quorum: 2,
            },
            3,
        );
        let opened = orchestrator
            .open_candidate_set(request)
            .expect("open candidate set");
        let refs = orchestrator
            .git()
            .list_candidate_ref_names()
            .expect("list candidate refs");
        assert_eq!(refs.len(), 3);
        assert_eq!(
            refs.iter().collect::<std::collections::BTreeSet<_>>().len(),
            3
        );
        assert!(
            orchestrator
                .git()
                .assert_isolated()
                .expect("isolation")
                .is_isolated()
        );
        assert!(
            opened
                .lanes
                .iter()
                .all(|lane| refs.contains(&lane.candidate_ref))
        );
        drop(orchestrator);
        drop(temp);
    }

    #[test]
    fn acceptance_contract_is_propagated_to_every_lane() {
        let (temp, mut orchestrator, request) =
            request_for(ConcurrencyPolicy::Speculative { max_candidates: 3 }, 2);
        let expected = request.acceptance.clone();
        let opened = orchestrator
            .open_candidate_set(request)
            .expect("open candidate set");
        assert!(opened.lanes.iter().all(|lane| lane.acceptance == expected));
        let status = orchestrator
            .status(opened.candidate_set.id, 10)
            .expect("status projection");
        assert!(status.lanes.iter().all(|lane| lane.acceptance == expected));
        drop(temp);
    }

    #[test]
    fn lifecycle_handles_failure_abandonment_and_timeout() {
        let (temp, mut orchestrator, request) =
            request_for(ConcurrencyPolicy::Speculative { max_candidates: 3 }, 3);
        let opened = orchestrator
            .open_candidate_set(request)
            .expect("open candidate set");
        let mut revision = opened.revision;
        let first = opened.lanes[0].candidate_id;
        let second = opened.lanes[1].candidate_id;
        let third = opened.lanes[2].candidate_id;
        revision = orchestrator
            .start_lane(first, revision)
            .expect("start first")
            .revision;
        let stale_revision = revision.saturating_sub(1);
        assert!(matches!(
            orchestrator.fail_lane(first, stale_revision, "stale writer"),
            Err(OrchestrationError::RevisionConflict { expected, actual })
                if expected == stale_revision && actual == revision
        ));
        revision = orchestrator
            .fail_lane(first, revision, "executor failed")
            .expect("fail first")
            .revision;
        let abandoned = orchestrator
            .abandon_lane(second, revision, "operator abandoned")
            .expect("abandon second");
        revision = abandoned.transition.revision;
        assert!(!abandoned.cleanup.candidate_ref_remaining);
        let timed_out = orchestrator
            .timeout_lane(third, revision, "lane deadline elapsed")
            .expect("timeout third");
        assert!(!timed_out.cleanup.base_ref_remaining);
        let status = orchestrator
            .status(opened.candidate_set.id, 10)
            .expect("status projection");
        assert_eq!(status.counts.failed, 1);
        assert_eq!(status.counts.abandoned, 1);
        assert_eq!(status.counts.timed_out, 1);
        drop(temp);
    }

    #[test]
    fn submitted_lanes_handoff_to_round_orchestrator() {
        let (temp, mut orchestrator, request) = request_for(
            ConcurrencyPolicy::Ensemble {
                candidate_count: 2,
                quorum: 1,
            },
            2,
        );
        let opened = orchestrator
            .open_candidate_set(request)
            .expect("open candidate set");
        let mut revision = opened.revision;
        for lane in &opened.lanes {
            revision = orchestrator
                .start_lane(lane.candidate_id, revision)
                .expect("start lane")
                .revision;
            revision = orchestrator
                .submit_lane(
                    lane.candidate_id,
                    revision,
                    CandidateSubmission::new(opened.candidate_set.base_commit.clone()),
                )
                .expect("submit lane")
                .revision;
        }
        let handoff = orchestrator
            .handoff_to_round(opened.candidate_set.id, revision)
            .expect("handoff to round");
        assert_eq!(handoff.opened.candidate_set_id, opened.candidate_set.id);
        assert_eq!(handoff.opened.expected_ballot_count, 2);
        assert!(handoff.round.status(handoff.opened.round_id, 10).is_ok());
        drop(temp);
    }

    #[test]
    fn status_projection_is_bounded() {
        let (temp, mut orchestrator, request) =
            request_for(ConcurrencyPolicy::Speculative { max_candidates: 3 }, 3);
        let opened = orchestrator
            .open_candidate_set(request)
            .expect("open candidate set");
        let status = orchestrator
            .status(opened.candidate_set.id, 2)
            .expect("bounded status");
        assert_eq!(status.limit, 2);
        assert_eq!(status.lanes.len(), 2);
        assert!(status.truncated);
        drop(temp);
    }
}
