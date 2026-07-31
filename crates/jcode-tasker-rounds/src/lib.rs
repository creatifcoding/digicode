//! Deterministic, evidence-bearing adjudication rounds over the Pi-compatible
//! concurrency store.
//!
//! This crate owns orchestration only. Candidate validation and tally policy stay
//! in [`jcode_tasker_types`], while candidate, ballot, round, and revision state
//! stay in [`jcode_tasker_pi`]. No validator process or model invocation is
//! performed here: a validator is the identity attached to a submitted ballot.

use chrono::{DateTime, Utc};
use jcode_tasker_pi::{ConcurrencyStore, ConcurrencyStoreError};
use jcode_tasker_types::{
    AdjudicationBallot, AdjudicationError, AdjudicationOutcome, AdjudicationPolicyDecision,
    AdjudicationPolicyExt, AdjudicationRoundId, BallotId, Candidate, CandidateAssessment,
    CandidateId, CandidateProvenance, CandidateSetId, CandidateSetState, CandidateState,
    ConcurrencyPolicy, ResourceIntent, ValidatorIdentity, adjudicate, validate_ballot,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use thiserror::Error;

const MAX_ROUND_PROJECTION_LIMIT: usize = 500;

/// The reason a round became terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundCompletion {
    /// More evidence may still change the policy outcome.
    Pending,
    /// An eligible candidate reached the configured adjudication quorum.
    QuorumReached,
    /// No eligible candidate can reach quorum with the remaining ballot slots.
    QuorumImpossible,
    /// The remaining ballot slots cannot satisfy the validity threshold.
    ValidityThresholdFailed,
    /// Every candidate is already vetoed by a hard gate.
    NoEligibleCandidate,
}

impl RoundCompletion {
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }
}

/// The durable identity and policy facts produced when a round is opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundOpened {
    pub round_id: AdjudicationRoundId,
    pub candidate_set_id: CandidateSetId,
    pub policy_version: u32,
    pub required_quorum: u16,
    pub expected_ballot_count: usize,
    pub revision: u64,
}

/// The deterministic state observed after opening or submitting a ballot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundProgress {
    pub round_id: AdjudicationRoundId,
    pub complete: bool,
    pub completion: RoundCompletion,
    pub decision: Option<AdjudicationPolicyDecision>,
    pub outcome: AdjudicationOutcome,
    pub ballot_count: usize,
    pub expected_ballot_count: usize,
    pub revision: u64,
}

impl RoundProgress {
    pub fn selected_candidate_id(&self) -> Option<CandidateId> {
        self.outcome.selected_candidate_id()
    }
}

/// A bounded read model for prompt and UI consumers.
///
/// `round` preserves the persistence projection shape, but its ballot array is
/// limited to `limit`. `outcome` is computed from the complete persisted ballot
/// set, so truncation never changes adjudication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundStatusProjection {
    pub round_id: AdjudicationRoundId,
    pub candidate_set_id: CandidateSetId,
    pub round: Value,
    pub ballots: Vec<AdjudicationBallot>,
    pub ballot_count: usize,
    pub limit: usize,
    pub truncated: bool,
    pub completion: RoundCompletion,
    pub decision: Option<AdjudicationPolicyDecision>,
    pub outcome: AdjudicationOutcome,
}

/// Errors raised while admitting or replaying a round.
#[derive(Debug, Error)]
pub enum RoundError {
    #[error("round persistence error: {0}")]
    Persistence(#[from] ConcurrencyStoreError),
    #[error("adjudication policy error: {0}")]
    Policy(#[from] AdjudicationError),
    #[error("round JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("round SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("round {round_id} was not found")]
    RoundNotFound { round_id: String },
    #[error("candidate set {candidate_set_id} was not found")]
    CandidateSetNotFound { candidate_set_id: String },
    #[error("candidate set {candidate_set_id} has no persisted candidates")]
    NoCandidates { candidate_set_id: String },
    #[error("candidate set {candidate_set_id} projection is truncated at {limit} candidates")]
    CandidateProjectionTruncated {
        candidate_set_id: String,
        limit: usize,
    },
    #[error("candidate set {candidate_set_id} is not open for adjudication: {state:?}")]
    CandidateSetClosed {
        candidate_set_id: String,
        state: CandidateSetState,
    },
    #[error("ballot {ballot_id} belongs to round {actual_round_id}, not {expected_round_id}")]
    WrongRound {
        ballot_id: String,
        actual_round_id: String,
        expected_round_id: String,
    },
    #[error(
        "ballot {ballot_id} is evidence-free: a non-abstaining ballot needs a CandidateAssessment"
    )]
    EvidenceFreeBallot { ballot_id: String },
    #[error("validator {validator} already submitted a ballot for round {round_id}")]
    DuplicateValidator { round_id: String, validator: String },
    #[error("ballot {ballot_id} was already submitted for round {round_id}")]
    DuplicateBallot { round_id: String, ballot_id: String },
    #[error("round {round_id} is already decided")]
    RoundAlreadyDecided { round_id: String },
    #[error(
        "round {round_id} has an inconsistent persisted policy quorum: stored {stored}, policy {policy}"
    )]
    PolicyQuorumMismatch {
        round_id: String,
        stored: u16,
        policy: u16,
    },
    #[error(
        "round {round_id} has persisted policy version {stored}, candidate set policy version {policy}"
    )]
    PolicyVersionMismatch {
        round_id: String,
        stored: u32,
        policy: u32,
    },
    #[error("round {round_id} persisted decision does not match deterministic replay")]
    DecisionDrift {
        round_id: String,
        persisted: Value,
        recomputed: Value,
    },
    #[error("project {project_id} revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict {
        project_id: String,
        expected: u64,
        actual: u64,
    },
    #[error("round {round_id} decision persistence raced with another writer")]
    DecisionPersistenceRace { round_id: String },
    #[error("round {round_id} has an invalid persisted decision: {reason}")]
    InvalidPersistedDecision { round_id: String, reason: String },
    #[error("round projection omitted required field {field}")]
    InvalidProjection { field: String },
}

/// The stateful round orchestrator.
///
/// `ConcurrencyStore` remains the source of persisted candidates, ballots, and
/// rounds. The small in-memory decision map exists only for `open_in_memory`
/// stores, whose SQLite connection cannot be reopened from another connection.
#[derive(Debug)]
pub struct RoundOrchestrator {
    store: ConcurrencyStore,
    in_memory_decisions: BTreeMap<AdjudicationRoundId, AdjudicationPolicyDecision>,
}

/// Conventional noun-first alias for callers that refer to the component as a
/// runner rather than an orchestrator.
pub type RoundRunner = RoundOrchestrator;

/// Explicit alias for callers that keep the adjudication boundary in the type
/// name.
pub type AdjudicationRoundRunner = RoundOrchestrator;

impl RoundOrchestrator {
    pub fn new(store: ConcurrencyStore) -> Self {
        Self {
            store,
            in_memory_decisions: BTreeMap::new(),
        }
    }

    pub fn store(&self) -> &ConcurrencyStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut ConcurrencyStore {
        &mut self.store
    }

    pub fn into_store(self) -> ConcurrencyStore {
        self.store
    }

    /// Open and persist a pending round for one candidate set.
    pub fn open_round(
        &mut self,
        candidate_set_id: CandidateSetId,
        expected_revision: u64,
    ) -> Result<RoundOpened, RoundError> {
        let candidate_context = self.load_candidate_set_context(candidate_set_id)?;
        if candidate_context.candidates.is_empty() {
            return Err(RoundError::NoCandidates {
                candidate_set_id: candidate_set_id.to_string(),
            });
        }

        let mut revision = expected_revision;
        match candidate_context.candidate_set.state {
            CandidateSetState::Open => {
                revision = self
                    .store
                    .set_candidate_set_state(
                        &candidate_set_id.to_string(),
                        "adjudicating",
                        revision,
                    )?
                    .revision;
            }
            CandidateSetState::Adjudicating => {}
            state => {
                return Err(RoundError::CandidateSetClosed {
                    candidate_set_id: candidate_set_id.to_string(),
                    state,
                });
            }
        }

        let round_id = AdjudicationRoundId::new();
        let mutation = self.store.record_adjudication_round(
            json!({
                "id": round_id,
                "candidateSetId": candidate_set_id,
                "policyVersion": candidate_context.candidate_set.policy_version,
                "requiredQuorum": candidate_context.required_quorum,
                "createdAt": Utc::now().to_rfc3339(),
            }),
            revision,
        )?;

        Ok(RoundOpened {
            round_id,
            candidate_set_id,
            policy_version: candidate_context.candidate_set.policy_version,
            required_quorum: candidate_context.required_quorum,
            expected_ballot_count: candidate_context.expected_ballot_count,
            revision: mutation.revision,
        })
    }

    /// Admit one validator ballot and evaluate the round deterministically.
    pub fn submit_ballot(
        &mut self,
        ballot: AdjudicationBallot,
        expected_revision: u64,
    ) -> Result<RoundProgress, RoundError> {
        let round_id = ballot.round_id;
        let context = self.load_round_context(round_id)?;
        if context.decision.is_some() {
            return Err(RoundError::RoundAlreadyDecided {
                round_id: round_id.to_string(),
            });
        }
        self.validate_new_ballot(&context, &ballot)?;

        let mutation = self
            .store
            .record_adjudication_ballot(ballot, expected_revision)?;
        let context = self.load_round_context(round_id)?;
        let (outcome, completion) = self.evaluate_context(&context)?;
        let (decision, revision) = if completion.is_terminal() {
            let revision = self.persist_decision(
                round_id,
                &context.project_id,
                &outcome.decision,
                mutation.revision,
            )?;
            (Some(outcome.decision.clone()), revision)
        } else {
            (None, mutation.revision)
        };

        Ok(progress(
            round_id,
            context.expected_ballot_count,
            context.ballots.len(),
            completion,
            decision,
            outcome,
            revision,
        ))
    }

    /// Replay every persisted ballot and, if terminal, verify or persist the decision.
    pub fn replay_round(
        &mut self,
        round_id: AdjudicationRoundId,
    ) -> Result<RoundProgress, RoundError> {
        let context = self.load_round_context(round_id)?;
        let (outcome, completion) = self.evaluate_context(&context)?;
        let mut decision = context.decision.clone();
        let revision = if completion.is_terminal() {
            let revision = self.persist_decision(
                round_id,
                &context.project_id,
                &outcome.decision,
                self.store.current_revision(&context.project_id)?,
            )?;
            decision = Some(outcome.decision.clone());
            revision
        } else {
            self.store.current_revision(&context.project_id)?
        };

        Ok(progress(
            round_id,
            context.expected_ballot_count,
            context.ballots.len(),
            completion,
            decision,
            outcome,
            revision,
        ))
    }

    /// Alias for callers that name the operation by its deterministic property.
    pub fn replay(&mut self, round_id: AdjudicationRoundId) -> Result<RoundProgress, RoundError> {
        self.replay_round(round_id)
    }

    /// Return a bounded round projection without mutating persisted state.
    pub fn round_status(
        &self,
        round_id: AdjudicationRoundId,
        limit: usize,
    ) -> Result<RoundStatusProjection, RoundError> {
        let context = self.load_round_context(round_id)?;
        let (outcome, completion) = self.evaluate_context(&context)?;
        let limit = limit.clamp(1, MAX_ROUND_PROJECTION_LIMIT);
        let truncated = context.ballots.len() > limit;
        let ballots = context
            .ballots
            .iter()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let decision = context.decision.clone();
        let mut round = context.round_value.clone();
        if let Some(object) = round.as_object_mut() {
            object.insert("ballots".into(), serde_json::to_value(&ballots)?);
            object.insert("limit".into(), json!(limit));
            object.insert("truncated".into(), json!(truncated));
            if let Some(decision) = &decision {
                object.insert("decision".into(), serde_json::to_value(decision)?);
            }
        }

        Ok(RoundStatusProjection {
            round_id,
            candidate_set_id: context.candidate_set.id,
            round,
            ballots,
            ballot_count: context.ballots.len(),
            limit,
            truncated,
            completion,
            decision,
            outcome,
        })
    }

    /// Alias for consumers that use `status` as the projection operation name.
    pub fn status(
        &self,
        round_id: AdjudicationRoundId,
        limit: usize,
    ) -> Result<RoundStatusProjection, RoundError> {
        self.round_status(round_id, limit)
    }

    fn load_candidate_set_context(
        &self,
        candidate_set_id: CandidateSetId,
    ) -> Result<CandidateSetContext, RoundError> {
        let id = candidate_set_id.to_string();
        let set_value =
            self.store
                .candidate_set(&id)?
                .ok_or_else(|| RoundError::CandidateSetNotFound {
                    candidate_set_id: id.clone(),
                })?;
        let candidate_set: StoredCandidateSet = serde_json::from_value(set_value)?;
        let required_quorum = candidate_set.policy.adjudication_quorum()?;
        let expected_ballot_count = usize::from(candidate_set.policy.candidate_limit());
        let projection = self
            .store
            .candidate_set_projection(&id, MAX_ROUND_PROJECTION_LIMIT)?;
        if projection
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(RoundError::CandidateProjectionTruncated {
                candidate_set_id: id,
                limit: MAX_ROUND_PROJECTION_LIMIT,
            });
        }
        let candidate_values = projection
            .get("candidates")
            .and_then(Value::as_array)
            .ok_or_else(|| RoundError::InvalidProjection {
                field: "candidates".into(),
            })?;
        let candidates = candidate_values
            .iter()
            .cloned()
            .map(serde_json::from_value::<StoredCandidate>)
            .map(|candidate| candidate.map(StoredCandidate::into_domain))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CandidateSetContext {
            candidate_set,
            candidates,
            required_quorum,
            expected_ballot_count,
        })
    }

    fn load_round_context(
        &self,
        round_id: AdjudicationRoundId,
    ) -> Result<RoundContext, RoundError> {
        let id = round_id.to_string();
        let round_value =
            self.store
                .adjudication_round(&id)?
                .ok_or_else(|| RoundError::RoundNotFound {
                    round_id: id.clone(),
                })?;
        let stored_round: StoredRound = serde_json::from_value(round_value.clone())?;
        let candidate_context = self.load_candidate_set_context(stored_round.candidate_set_id)?;
        if stored_round.required_quorum != candidate_context.required_quorum {
            return Err(RoundError::PolicyQuorumMismatch {
                round_id: id,
                stored: stored_round.required_quorum,
                policy: candidate_context.required_quorum,
            });
        }
        if stored_round.policy_version != candidate_context.candidate_set.policy_version {
            return Err(RoundError::PolicyVersionMismatch {
                round_id: id,
                stored: stored_round.policy_version,
                policy: candidate_context.candidate_set.policy_version,
            });
        }
        let CandidateSetContext {
            candidate_set,
            candidates,
            expected_ballot_count,
            ..
        } = candidate_context;
        let policy = candidate_set.policy.clone();
        let ballots = stored_round
            .ballots
            .into_iter()
            .map(StoredBallot::into_domain)
            .collect::<Vec<_>>();
        ensure_unique_ballots_and_validators(round_id, &ballots)?;
        let persisted_decision = stored_round
            .decision
            .map(|value| parse_decision(round_id, value))
            .transpose()?;
        let decision =
            persisted_decision.or_else(|| self.in_memory_decisions.get(&round_id).cloned());
        Ok(RoundContext {
            round_value,
            round_id,
            project_id: candidate_set.project_id.to_string(),
            candidate_set,
            candidates,
            policy,
            expected_ballot_count,
            ballots,
            decision,
        })
    }

    fn validate_new_ballot(
        &self,
        context: &RoundContext,
        ballot: &AdjudicationBallot,
    ) -> Result<(), RoundError> {
        if ballot.round_id != context.round_id {
            return Err(RoundError::WrongRound {
                ballot_id: ballot.id.to_string(),
                actual_round_id: ballot.round_id.to_string(),
                expected_round_id: context.round_id.to_string(),
            });
        }
        if context
            .ballots
            .iter()
            .any(|existing| existing.id == ballot.id)
        {
            return Err(RoundError::DuplicateBallot {
                round_id: context.round_id.to_string(),
                ballot_id: ballot.id.to_string(),
            });
        }
        let key = validator_key(&ballot.validator);
        if context
            .ballots
            .iter()
            .map(|existing| validator_key(&existing.validator))
            .any(|existing| existing == key)
        {
            return Err(RoundError::DuplicateValidator {
                round_id: context.round_id.to_string(),
                validator: key,
            });
        }
        if !ballot.abstained && ballot.assessments.is_empty() {
            return Err(RoundError::EvidenceFreeBallot {
                ballot_id: ballot.id.to_string(),
            });
        }
        let candidate_ids = context
            .candidates
            .iter()
            .map(|candidate| candidate.id)
            .collect::<BTreeSet<_>>();
        validate_ballot(ballot, &candidate_ids)?;
        Ok(())
    }

    fn evaluate_context(
        &self,
        context: &RoundContext,
    ) -> Result<(AdjudicationOutcome, RoundCompletion), RoundError> {
        let outcome = adjudicate(&context.candidates, &context.ballots, &context.policy)?;
        if let Some(persisted) = &context.decision
            && persisted != &outcome.decision
        {
            return Err(RoundError::DecisionDrift {
                round_id: context.round_id.to_string(),
                persisted: serde_json::to_value(persisted)?,
                recomputed: serde_json::to_value(&outcome.decision)?,
            });
        }
        let completion = completion_for(&outcome, &context.ballots, context.expected_ballot_count);
        Ok((outcome, completion))
    }

    fn persist_decision(
        &mut self,
        round_id: AdjudicationRoundId,
        project_id: &str,
        decision: &AdjudicationPolicyDecision,
        expected_revision: u64,
    ) -> Result<u64, RoundError> {
        // The Pi store exposes round insertion and ballot insertion, but no
        // decision-update method. Keep this completion write in the rounds
        // boundary, using the store's schema and the same optimistic revision
        // check, rather than modifying the separately owned Pi crate.
        let decision_value = serde_json::to_value(decision)?;
        if self.store.partition().db_path.as_os_str() == ":memory:" {
            self.in_memory_decisions.insert(round_id, decision.clone());
            return Ok(expected_revision);
        }

        let mut connection = Connection::open(&self.store.partition().db_path)?;
        connection.busy_timeout(Duration::from_secs(30))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = transaction
            .query_row(
                "SELECT s.project_id, r.decision_json
                 FROM adjudication_rounds r
                 JOIN candidate_sets s ON s.id = r.candidate_set_id
                 WHERE r.id = ?1",
                [round_id.to_string()],
                |row| {
                    Ok::<(String, Option<String>), rusqlite::Error>((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()?;
        let Some((stored_project_id, existing_json)) = stored else {
            return Err(RoundError::RoundNotFound {
                round_id: round_id.to_string(),
            });
        };
        if stored_project_id != project_id {
            return Err(RoundError::Persistence(ConcurrencyStoreError::NotFound {
                entity: "round project".into(),
                id: round_id.to_string(),
            }));
        }
        let actual_revision = transaction
            .query_row(
                "SELECT revision FROM concurrency_project_revisions WHERE project_id = ?1",
                [project_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);
        let actual_revision = u64::try_from(actual_revision).map_err(|_| {
            ConcurrencyStoreError::Schema(format!("negative project revision for {project_id}"))
        })?;

        if let Some(existing_json) = existing_json {
            let existing = serde_json::from_str::<Value>(&existing_json)?;
            if existing == decision_value {
                transaction.commit()?;
                return Ok(actual_revision);
            }
            return Err(RoundError::DecisionDrift {
                round_id: round_id.to_string(),
                persisted: existing,
                recomputed: decision_value,
            });
        }
        if actual_revision != expected_revision {
            return Err(RoundError::RevisionConflict {
                project_id: project_id.into(),
                expected: expected_revision,
                actual: actual_revision,
            });
        }

        let decision_json = serde_json::to_string(&decision_value)?;
        let changed = transaction.execute(
            "UPDATE adjudication_rounds
             SET decision_json = ?1, decided_at = ?2
             WHERE id = ?3 AND decision_json IS NULL",
            params![decision_json, Utc::now().to_rfc3339(), round_id.to_string()],
        )?;
        if changed != 1 {
            return Err(RoundError::DecisionPersistenceRace {
                round_id: round_id.to_string(),
            });
        }
        let expected_revision_i64 = i64::try_from(expected_revision).map_err(|_| {
            ConcurrencyStoreError::Schema(format!(
                "project revision is too large for SQLite: {expected_revision}"
            ))
        })?;
        let changed_revision = transaction.execute(
            "UPDATE concurrency_project_revisions
             SET revision = revision + 1, updated_at = ?1
             WHERE project_id = ?2 AND revision = ?3",
            params![Utc::now().to_rfc3339(), project_id, expected_revision_i64],
        )?;
        if changed_revision != 1 {
            let actual = transaction
                .query_row(
                    "SELECT revision FROM concurrency_project_revisions WHERE project_id = ?1",
                    [project_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0);
            let actual = u64::try_from(actual).map_err(|_| {
                ConcurrencyStoreError::Schema(format!("negative project revision for {project_id}"))
            })?;
            return Err(RoundError::RevisionConflict {
                project_id: project_id.into(),
                expected: expected_revision,
                actual,
            });
        }
        transaction.commit()?;
        Ok(expected_revision + 1)
    }
}

fn progress(
    round_id: AdjudicationRoundId,
    expected_ballot_count: usize,
    ballot_count: usize,
    completion: RoundCompletion,
    decision: Option<AdjudicationPolicyDecision>,
    outcome: AdjudicationOutcome,
    revision: u64,
) -> RoundProgress {
    RoundProgress {
        round_id,
        complete: completion.is_terminal(),
        completion,
        decision,
        outcome,
        ballot_count,
        expected_ballot_count,
        revision,
    }
}

fn completion_for(
    outcome: &AdjudicationOutcome,
    ballots: &[AdjudicationBallot],
    expected_ballot_count: usize,
) -> RoundCompletion {
    if matches!(outcome.decision, AdjudicationPolicyDecision::Select { .. }) {
        return RoundCompletion::QuorumReached;
    }
    let eligible_count = outcome
        .candidate_tallies
        .iter()
        .filter(|tally| tally.hard_gate_passed)
        .count();
    if eligible_count == 0 {
        return RoundCompletion::NoEligibleCandidate;
    }
    let remaining = expected_ballot_count.saturating_sub(ballots.len());
    if outcome.usable_ballot_count + remaining < outcome.round_validity_threshold {
        return RoundCompletion::ValidityThresholdFailed;
    }
    let can_still_reach = outcome.candidate_tallies.iter().any(|tally| {
        tally.hard_gate_passed
            && tally.approval_count.saturating_add(remaining)
                >= usize::from(outcome.required_quorum)
    });
    if !can_still_reach {
        return RoundCompletion::QuorumImpossible;
    }
    RoundCompletion::Pending
}

fn ensure_unique_ballots_and_validators(
    round_id: AdjudicationRoundId,
    ballots: &[AdjudicationBallot],
) -> Result<(), RoundError> {
    let mut ballot_ids = BTreeSet::new();
    let mut validators = BTreeSet::new();
    for ballot in ballots {
        if !ballot_ids.insert(ballot.id) {
            return Err(RoundError::DuplicateBallot {
                round_id: round_id.to_string(),
                ballot_id: ballot.id.to_string(),
            });
        }
        let key = validator_key(&ballot.validator);
        if !validators.insert(key.clone()) {
            return Err(RoundError::DuplicateValidator {
                round_id: round_id.to_string(),
                validator: key,
            });
        }
    }
    Ok(())
}

fn validator_key(identity: &ValidatorIdentity) -> String {
    format!(
        "agent={};session={};model={};lineage={}",
        identity.agent_id,
        identity.session_id,
        identity.model_id.as_deref().unwrap_or_default(),
        identity.lineage_digest,
    )
}

fn parse_decision(
    round_id: AdjudicationRoundId,
    value: Value,
) -> Result<AdjudicationPolicyDecision, RoundError> {
    serde_json::from_value(value).map_err(|error| RoundError::InvalidPersistedDecision {
        round_id: round_id.to_string(),
        reason: error.to_string(),
    })
}

#[derive(Debug)]
struct CandidateSetContext {
    candidate_set: StoredCandidateSet,
    candidates: Vec<Candidate>,
    required_quorum: u16,
    expected_ballot_count: usize,
}

#[derive(Debug)]
struct RoundContext {
    round_value: Value,
    round_id: AdjudicationRoundId,
    project_id: String,
    candidate_set: StoredCandidateSet,
    candidates: Vec<Candidate>,
    policy: ConcurrencyPolicy,
    expected_ballot_count: usize,
    ballots: Vec<AdjudicationBallot>,
    decision: Option<AdjudicationPolicyDecision>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCandidateSet {
    id: CandidateSetId,
    project_id: jcode_tasker_types::ProjectId,
    policy: ConcurrencyPolicy,
    policy_version: u32,
    state: CandidateSetState,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCandidate {
    id: CandidateId,
    candidate_set_id: CandidateSetId,
    state: CandidateState,
    base_commit: String,
    result_commit: Option<String>,
    diff_digest: Option<String>,
    summary: Option<String>,
    provenance: StoredCandidateProvenance,
    resource_intents: Vec<ResourceIntent>,
    supersedes_candidate_id: Option<CandidateId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    submitted_at: Option<DateTime<Utc>>,
}

impl StoredCandidate {
    fn into_domain(self) -> Candidate {
        Candidate {
            id: self.id,
            candidate_set_id: self.candidate_set_id,
            state: self.state,
            base_commit: self.base_commit,
            result_commit: self.result_commit,
            diff_digest: self.diff_digest,
            summary: self.summary,
            provenance: self.provenance.into_domain(),
            resource_intents: self.resource_intents,
            supersedes_candidate_id: self.supersedes_candidate_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
            submitted_at: self.submitted_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCandidateProvenance {
    session_id: String,
    agent_id: String,
    model_id: Option<String>,
    work_unit_id: Option<String>,
    lineage_digest: Option<String>,
}

impl StoredCandidateProvenance {
    fn into_domain(self) -> CandidateProvenance {
        CandidateProvenance {
            session_id: self.session_id,
            agent_id: self.agent_id,
            model_id: self.model_id,
            work_unit_id: self.work_unit_id,
            lineage_digest: self.lineage_digest,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRound {
    candidate_set_id: CandidateSetId,
    policy_version: u32,
    required_quorum: u16,
    decision: Option<Value>,
    ballots: Vec<StoredBallot>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredBallot {
    id: BallotId,
    round_id: AdjudicationRoundId,
    validator: StoredValidatorIdentity,
    assessments: Vec<StoredAssessment>,
    ranking: Vec<CandidateId>,
    abstained: bool,
    created_at: DateTime<Utc>,
}

impl StoredBallot {
    fn into_domain(self) -> AdjudicationBallot {
        AdjudicationBallot {
            id: self.id,
            round_id: self.round_id,
            validator: self.validator.into_domain(),
            assessments: self
                .assessments
                .into_iter()
                .map(StoredAssessment::into_domain)
                .collect(),
            ranking: self.ranking,
            abstained: self.abstained,
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredValidatorIdentity {
    session_id: Option<String>,
    agent_id: Option<String>,
    model_id: Option<String>,
    lineage_digest: String,
}

impl StoredValidatorIdentity {
    fn into_domain(self) -> ValidatorIdentity {
        ValidatorIdentity {
            session_id: self.session_id.unwrap_or_default(),
            agent_id: self.agent_id.unwrap_or_default(),
            model_id: self.model_id,
            lineage_digest: self.lineage_digest,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAssessment {
    candidate_id: CandidateId,
    eligible: bool,
    approve: bool,
    acceptance_score: u16,
    risk_score: u16,
    complexity_score: u16,
    notes: Vec<String>,
}

impl StoredAssessment {
    fn into_domain(self) -> CandidateAssessment {
        CandidateAssessment {
            candidate_id: self.candidate_id,
            eligible: self.eligible,
            approve: self.approve,
            acceptance_score: self.acceptance_score,
            risk_score: self.risk_score,
            complexity_score: self.complexity_score,
            notes: self.notes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_is_terminal_only_after_quorum_or_an_impossibility_proof() {
        let outcome = AdjudicationOutcome {
            decision: AdjudicationPolicyDecision::Escalation {
                summary: jcode_tasker_types::DisagreementSummary {
                    reason: jcode_tasker_types::EscalationReason::QuorumUnreached {
                        leading_approval_count: 1,
                        required_quorum: 2,
                    },
                    required_quorum: 2,
                    round_validity_threshold: 2,
                    expected_candidate_count: 3,
                    actual_candidate_count: 3,
                    usable_ballot_count: 1,
                    abstention_count: 0,
                    candidate_tallies: vec![jcode_tasker_types::CandidateTally {
                        candidate_id: CandidateId::new(),
                        hard_gate_passed: true,
                        approval_count: 1,
                        assessment_count: 1,
                        evidence_weight: 1,
                        risk_score_total: 1,
                        complexity_score_total: 1,
                        created_at: Utc::now(),
                    }],
                    suggested_source_candidate_ids: vec![],
                },
            },
            candidate_tallies: vec![jcode_tasker_types::CandidateTally {
                candidate_id: CandidateId::new(),
                hard_gate_passed: true,
                approval_count: 1,
                assessment_count: 1,
                evidence_weight: 1,
                risk_score_total: 1,
                complexity_score_total: 1,
                created_at: Utc::now(),
            }],
            required_quorum: 2,
            round_validity_threshold: 2,
            usable_ballot_count: 1,
            abstention_count: 0,
        };
        let ballot = AdjudicationBallot {
            id: BallotId::new(),
            round_id: AdjudicationRoundId::new(),
            validator: ValidatorIdentity {
                session_id: "session".into(),
                agent_id: "agent".into(),
                model_id: None,
                lineage_digest: "lineage".into(),
            },
            assessments: vec![],
            ranking: vec![],
            abstained: true,
            created_at: Utc::now(),
        };
        assert_eq!(
            completion_for(&outcome, std::slice::from_ref(&ballot), 3),
            RoundCompletion::Pending
        );
        assert_eq!(
            completion_for(&outcome, std::slice::from_ref(&ballot), 1),
            RoundCompletion::ValidityThresholdFailed
        );
    }
}
