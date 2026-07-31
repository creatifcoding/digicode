//! Pure, deterministic adjudication over concurrency artifacts.
//!
//! The existing [`crate::AdjudicationDecision`] predates escalation as a
//! first-class outcome. This module therefore exposes
//! [`AdjudicationPolicyDecision`] as the policy extension without changing the
//! owned concurrency domain file. An escalation is deliberately not a
//! selection: it carries the complete, deterministically ordered disagreement
//! summary for a higher-cost adjudicator or an operator.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    AdjudicationBallot, AdjudicationDecision as DomainAdjudicationDecision, BallotId, Candidate,
    CandidateAssessment, CandidateId, CandidateState, ConcurrencyPolicy,
};

/// Errors found before a deterministic adjudication decision can be made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdjudicationError {
    /// The policy cannot define a valid adjudication quorum.
    InvalidPolicy { reason: String },
    /// Candidate IDs must identify one candidate each.
    DuplicateCandidate { candidate_id: CandidateId },
    /// Ballot IDs must identify one ballot each.
    DuplicateBallot { ballot_id: BallotId },
    /// Non-abstaining ballots must carry at least one candidate assessment.
    MissingAssessmentReference { ballot_id: BallotId },
    /// An assessment or ranking points outside the candidate set.
    UnknownCandidateReference {
        ballot_id: BallotId,
        candidate_id: CandidateId,
    },
    /// A ballot cannot assess the same candidate twice.
    DuplicateAssessmentReference {
        ballot_id: BallotId,
        candidate_id: CandidateId,
    },
    /// A ballot cannot rank the same candidate twice.
    DuplicateRankingReference {
        ballot_id: BallotId,
        candidate_id: CandidateId,
    },
}

impl fmt::Display for AdjudicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicy { reason } => {
                write!(formatter, "invalid adjudication policy: {reason}")
            }
            Self::DuplicateCandidate { candidate_id } => {
                write!(
                    formatter,
                    "candidate {candidate_id} is present more than once"
                )
            }
            Self::DuplicateBallot { ballot_id } => {
                write!(formatter, "ballot {ballot_id} is present more than once")
            }
            Self::MissingAssessmentReference { ballot_id } => write!(
                formatter,
                "ballot {ballot_id} is non-abstaining but has no assessment reference"
            ),
            Self::UnknownCandidateReference {
                ballot_id,
                candidate_id,
            } => write!(
                formatter,
                "ballot {ballot_id} references unknown candidate {candidate_id}"
            ),
            Self::DuplicateAssessmentReference {
                ballot_id,
                candidate_id,
            } => write!(
                formatter,
                "ballot {ballot_id} assesses candidate {candidate_id} more than once"
            ),
            Self::DuplicateRankingReference {
                ballot_id,
                candidate_id,
            } => write!(
                formatter,
                "ballot {ballot_id} ranks candidate {candidate_id} more than once"
            ),
        }
    }
}

impl std::error::Error for AdjudicationError {}

/// The source of a failed hard gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardGateFailure {
    /// The candidate's persisted lifecycle state is not adjudicable.
    CandidateState { state: CandidateState },
    /// A validator's assessment vetoed the candidate.
    BallotAssessment {
        ballot_id: BallotId,
        validator_agent_id: String,
        notes: Vec<String>,
    },
}

/// Hard-gate result for one candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardGateEvaluation {
    pub candidate_id: CandidateId,
    pub passed: bool,
    pub failures: Vec<HardGateFailure>,
}

/// Deterministic aggregate evidence for one candidate.
///
/// Winner ordering is total and stable:
///
/// 1. hard-gate-passing candidates only;
/// 2. approval count, descending;
/// 3. `evidence_weight` (the sum of acceptance scores for approving
///    assessments), descending;
/// 4. risk score total, ascending;
/// 5. complexity score total, ascending;
/// 6. candidate creation time, earliest first;
/// 7. candidate ID, ascending as the final tie breaker.
///
/// Input slice order is never part of this ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateTally {
    pub candidate_id: CandidateId,
    pub hard_gate_passed: bool,
    pub approval_count: usize,
    pub assessment_count: usize,
    pub evidence_weight: u64,
    pub risk_score_total: u64,
    pub complexity_score_total: u64,
    pub created_at: DateTime<Utc>,
}

/// Why the deterministic adjudicator escalated instead of selecting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscalationReason {
    /// Every candidate failed a hard gate or was not in an adjudicable state.
    NoEligibleCandidate,
    /// Too few non-abstaining ballots made the round invalid.
    AbstentionStarvation {
        usable_ballots: usize,
        abstentions: usize,
        required_valid_ballots: usize,
    },
    /// Too few usable ballots made the round invalid without abstentions being
    /// the cause.
    InsufficientValidBallots {
        usable_ballots: usize,
        required_valid_ballots: usize,
    },
    /// The round was valid, but the strongest candidate did not reach quorum.
    QuorumUnreached {
        leading_approval_count: usize,
        required_quorum: u16,
    },
}

impl fmt::Display for EscalationReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEligibleCandidate => formatter.write_str("no eligible candidate"),
            Self::AbstentionStarvation {
                usable_ballots,
                abstentions,
                required_valid_ballots,
            } => write!(
                formatter,
                "abstention starvation: {usable_ballots} usable ballots, {abstentions} abstentions, {required_valid_ballots} required"
            ),
            Self::InsufficientValidBallots {
                usable_ballots,
                required_valid_ballots,
            } => write!(
                formatter,
                "insufficient valid ballots: {usable_ballots} usable ballots, {required_valid_ballots} required"
            ),
            Self::QuorumUnreached {
                leading_approval_count,
                required_quorum,
            } => write!(
                formatter,
                "quorum unreached: leading candidate has {leading_approval_count} approvals, {required_quorum} required"
            ),
        }
    }
}

/// Deterministic disagreement evidence returned when no candidate reaches
/// adjudication quorum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisagreementSummary {
    pub reason: EscalationReason,
    pub required_quorum: u16,
    pub round_validity_threshold: usize,
    pub expected_candidate_count: usize,
    pub actual_candidate_count: usize,
    pub usable_ballot_count: usize,
    pub abstention_count: usize,
    pub candidate_tallies: Vec<CandidateTally>,
    /// At most the two strongest eligible candidates are suggested as
    /// synthesis sources. This is a hint for the next adjudication stage, not
    /// a synthesized candidate and not a winner.
    pub suggested_source_candidate_ids: Vec<CandidateId>,
}

/// The policy-level decision. `Escalation` is intentionally distinct from the
/// older domain `Blocked` variant so callers cannot mistake disagreement for
/// a completed selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AdjudicationPolicyDecision {
    Select { candidate_id: CandidateId },
    Escalation { summary: DisagreementSummary },
}

/// Compatibility alias for callers that prefer a result-oriented name.
pub type AdjudicationDecisionResult = AdjudicationPolicyDecision;

/// Complete deterministic adjudication output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjudicationOutcome {
    pub decision: AdjudicationPolicyDecision,
    pub candidate_tallies: Vec<CandidateTally>,
    pub required_quorum: u16,
    pub round_validity_threshold: usize,
    pub usable_ballot_count: usize,
    pub abstention_count: usize,
}

/// Compatibility alias for callers that want a report-shaped name.
pub type AdjudicationResult = AdjudicationOutcome;

/// Extension methods for the concurrency policy without modifying its owned
/// domain module.
pub trait AdjudicationPolicyExt {
    fn adjudication_quorum(&self) -> Result<u16, AdjudicationError>;
    fn round_validity_threshold(&self) -> Result<usize, AdjudicationError>;
}

impl AdjudicationPolicyExt for ConcurrencyPolicy {
    fn adjudication_quorum(&self) -> Result<u16, AdjudicationError> {
        policy_parameters(self).map(|(_, quorum)| quorum)
    }

    fn round_validity_threshold(&self) -> Result<usize, AdjudicationError> {
        policy_parameters(self).map(|(_, quorum)| usize::from(quorum))
    }
}

impl AdjudicationPolicyDecision {
    /// Convert a policy decision to the pre-existing domain decision.
    ///
    /// The conversion is lossy for escalation because the older domain enum
    /// has no escalation variant. Preserve [`AdjudicationOutcome`] when the
    /// disagreement summary is needed by a downstream operator or service.
    pub fn as_domain_decision(&self) -> DomainAdjudicationDecision {
        match self {
            Self::Select { candidate_id } => DomainAdjudicationDecision::Select {
                candidate_id: *candidate_id,
            },
            Self::Escalation { summary } => DomainAdjudicationDecision::Blocked {
                reason: format!("adjudication escalation: {}", summary.reason),
            },
        }
    }
}

impl AdjudicationOutcome {
    pub fn selected_candidate_id(&self) -> Option<CandidateId> {
        match self.decision {
            AdjudicationPolicyDecision::Select { candidate_id } => Some(candidate_id),
            AdjudicationPolicyDecision::Escalation { .. } => None,
        }
    }
}

/// Validate one ballot against the candidate set.
///
/// An explicit abstention is a valid ballot with no candidate claim. Every
/// non-abstaining ballot must carry at least one `CandidateAssessment`; the
/// assessment's `candidate_id` is the evidence reference.
pub fn validate_ballot(
    ballot: &AdjudicationBallot,
    candidate_ids: &BTreeSet<CandidateId>,
) -> Result<(), AdjudicationError> {
    if !ballot.abstained && ballot.assessments.is_empty() {
        return Err(AdjudicationError::MissingAssessmentReference {
            ballot_id: ballot.id,
        });
    }

    let mut assessment_ids = BTreeSet::new();
    for assessment in assessments_in_deterministic_order(ballot) {
        if !candidate_ids.contains(&assessment.candidate_id) {
            return Err(AdjudicationError::UnknownCandidateReference {
                ballot_id: ballot.id,
                candidate_id: assessment.candidate_id,
            });
        }
        if !assessment_ids.insert(assessment.candidate_id) {
            return Err(AdjudicationError::DuplicateAssessmentReference {
                ballot_id: ballot.id,
                candidate_id: assessment.candidate_id,
            });
        }
    }

    let mut ranking_ids = BTreeSet::new();
    let mut ranking = ballot.ranking.to_vec();
    ranking.sort_unstable();
    for candidate_id in ranking {
        if !candidate_ids.contains(&candidate_id) {
            return Err(AdjudicationError::UnknownCandidateReference {
                ballot_id: ballot.id,
                candidate_id,
            });
        }
        if !ranking_ids.insert(candidate_id) {
            return Err(AdjudicationError::DuplicateRankingReference {
                ballot_id: ballot.id,
                candidate_id,
            });
        }
    }

    Ok(())
}

/// Evaluate hard gates before any score or quorum tally is considered.
///
/// A candidate lifecycle state outside `Eligible` or `Selected`, or one
/// assessment with `eligible == false`, is a veto. Vetoes are monotonic: no
/// number of approving ballots can make that candidate selectable.
pub fn evaluate_hard_gates(
    candidates: &[Candidate],
    ballots: &[AdjudicationBallot],
) -> Result<Vec<HardGateEvaluation>, AdjudicationError> {
    let candidate_ids = candidate_ids(candidates)?;
    validate_ballot_ids(ballots)?;
    for ballot in ballots_in_deterministic_order(ballots) {
        validate_ballot(ballot, &candidate_ids)?;
    }

    let mut evaluations = BTreeMap::new();
    for candidate in candidates_in_deterministic_order(candidates) {
        let failures = if candidate.state.is_eligible_for_adjudication() {
            Vec::new()
        } else {
            vec![HardGateFailure::CandidateState {
                state: candidate.state,
            }]
        };
        evaluations.insert(
            candidate.id,
            HardGateEvaluation {
                candidate_id: candidate.id,
                passed: failures.is_empty(),
                failures,
            },
        );
    }

    for ballot in ballots_in_deterministic_order(ballots) {
        for assessment in assessments_in_deterministic_order(ballot) {
            if assessment.eligible {
                continue;
            }
            if let Some(evaluation) = evaluations.get_mut(&assessment.candidate_id) {
                evaluation.failures.push(HardGateFailure::BallotAssessment {
                    ballot_id: ballot.id,
                    validator_agent_id: ballot.validator.agent_id.clone(),
                    notes: assessment.notes.clone(),
                });
                evaluation.passed = false;
            }
        }
    }

    for evaluation in evaluations.values_mut() {
        evaluation.failures.sort_by(compare_hard_gate_failures);
    }

    Ok(evaluations.into_values().collect())
}

/// Adjudicate candidates using a pure, deterministic quorum policy.
///
/// For `ensemble(candidate_count, quorum)`, `quorum` is the number of
/// non-abstaining approval assessments required for selection.
/// The round validity threshold is also `quorum`: abstentions never satisfy
/// it. If no hard-gate-passing candidate reaches that threshold, the result is
/// an [`AdjudicationPolicyDecision::Escalation`] with ordered evidence rather
/// than an arbitrary winner.
pub fn adjudicate(
    candidates: &[Candidate],
    ballots: &[AdjudicationBallot],
    policy: &ConcurrencyPolicy,
) -> Result<AdjudicationOutcome, AdjudicationError> {
    let (expected_candidate_count, required_quorum) = policy_parameters(policy)?;
    let candidate_ids = candidate_ids(candidates)?;
    validate_ballot_ids(ballots)?;
    for ballot in ballots_in_deterministic_order(ballots) {
        validate_ballot(ballot, &candidate_ids)?;
    }

    let hard_gates = evaluate_hard_gates(candidates, ballots)?;
    let hard_gate_by_candidate: BTreeMap<_, _> = hard_gates
        .into_iter()
        .map(|evaluation| (evaluation.candidate_id, evaluation))
        .collect();

    let mut tallies: BTreeMap<CandidateId, MutableCandidateTally> = candidates
        .iter()
        .map(|candidate| {
            let hard_gate_passed = hard_gate_by_candidate
                .get(&candidate.id)
                .is_some_and(|evaluation| evaluation.passed);
            (
                candidate.id,
                MutableCandidateTally {
                    candidate_id: candidate.id,
                    hard_gate_passed,
                    created_at: candidate.created_at,
                    approval_count: 0,
                    assessment_count: 0,
                    evidence_weight: 0,
                    risk_score_total: 0,
                    complexity_score_total: 0,
                },
            )
        })
        .collect();

    let mut usable_ballot_count = 0;
    let mut abstention_count = 0;
    for ballot in ballots_in_deterministic_order(ballots) {
        if ballot.abstained {
            abstention_count += 1;
            continue;
        }

        usable_ballot_count += 1;
        for assessment in assessments_in_deterministic_order(ballot) {
            let Some(tally) = tallies.get_mut(&assessment.candidate_id) else {
                continue;
            };
            tally.assessment_count += 1;
            if assessment.eligible && assessment.approve {
                tally.approval_count += 1;
                tally.evidence_weight = tally
                    .evidence_weight
                    .saturating_add(u64::from(assessment.acceptance_score));
                tally.risk_score_total = tally
                    .risk_score_total
                    .saturating_add(u64::from(assessment.risk_score));
                tally.complexity_score_total = tally
                    .complexity_score_total
                    .saturating_add(u64::from(assessment.complexity_score));
            }
        }
    }

    let mut candidate_tallies: Vec<_> = tallies
        .into_values()
        .map(MutableCandidateTally::finish)
        .collect();
    candidate_tallies.sort_by(compare_candidate_tallies);

    let round_validity_threshold = usize::from(required_quorum);
    let decision = if let Some(tally) = candidate_tallies.iter().find(|tally| {
        tally.hard_gate_passed && tally.approval_count >= usize::from(required_quorum)
    }) {
        AdjudicationPolicyDecision::Select {
            candidate_id: tally.candidate_id,
        }
    } else {
        let eligible_tallies: Vec<_> = candidate_tallies
            .iter()
            .filter(|tally| tally.hard_gate_passed)
            .collect();
        let reason = if eligible_tallies.is_empty() {
            EscalationReason::NoEligibleCandidate
        } else if usable_ballot_count < round_validity_threshold && abstention_count > 0 {
            EscalationReason::AbstentionStarvation {
                usable_ballots: usable_ballot_count,
                abstentions: abstention_count,
                required_valid_ballots: round_validity_threshold,
            }
        } else if usable_ballot_count < round_validity_threshold {
            EscalationReason::InsufficientValidBallots {
                usable_ballots: usable_ballot_count,
                required_valid_ballots: round_validity_threshold,
            }
        } else {
            EscalationReason::QuorumUnreached {
                leading_approval_count: eligible_tallies
                    .first()
                    .map_or(0, |tally| tally.approval_count),
                required_quorum,
            }
        };

        let suggested_source_candidate_ids = eligible_tallies
            .iter()
            .take(2)
            .map(|tally| tally.candidate_id)
            .collect();

        AdjudicationPolicyDecision::Escalation {
            summary: DisagreementSummary {
                reason,
                required_quorum,
                round_validity_threshold,
                expected_candidate_count,
                actual_candidate_count: candidates.len(),
                usable_ballot_count,
                abstention_count,
                candidate_tallies: candidate_tallies.clone(),
                suggested_source_candidate_ids,
            },
        }
    };

    Ok(AdjudicationOutcome {
        decision,
        candidate_tallies,
        required_quorum,
        round_validity_threshold,
        usable_ballot_count,
        abstention_count,
    })
}

/// Return only the policy decision while retaining the same validation and
/// deterministic behavior as [`adjudicate`].
pub fn adjudicate_decision(
    candidates: &[Candidate],
    ballots: &[AdjudicationBallot],
    policy: &ConcurrencyPolicy,
) -> Result<AdjudicationPolicyDecision, AdjudicationError> {
    adjudicate(candidates, ballots, policy).map(|outcome| outcome.decision)
}

#[derive(Debug, Clone)]
struct MutableCandidateTally {
    candidate_id: CandidateId,
    hard_gate_passed: bool,
    approval_count: usize,
    assessment_count: usize,
    evidence_weight: u64,
    risk_score_total: u64,
    complexity_score_total: u64,
    created_at: DateTime<Utc>,
}

impl MutableCandidateTally {
    fn finish(self) -> CandidateTally {
        CandidateTally {
            candidate_id: self.candidate_id,
            hard_gate_passed: self.hard_gate_passed,
            approval_count: self.approval_count,
            assessment_count: self.assessment_count,
            evidence_weight: self.evidence_weight,
            risk_score_total: self.risk_score_total,
            complexity_score_total: self.complexity_score_total,
            created_at: self.created_at,
        }
    }
}

fn policy_parameters(policy: &ConcurrencyPolicy) -> Result<(usize, u16), AdjudicationError> {
    policy
        .validate()
        .map_err(|error| AdjudicationError::InvalidPolicy {
            reason: error.to_string(),
        })?;

    Ok(match policy {
        ConcurrencyPolicy::Exclusive => (1, 1),
        ConcurrencyPolicy::Speculative { max_candidates } => (usize::from(*max_candidates), 1),
        ConcurrencyPolicy::Ensemble {
            candidate_count,
            quorum,
        } => (usize::from(*candidate_count), *quorum),
    })
}

fn candidate_ids(candidates: &[Candidate]) -> Result<BTreeSet<CandidateId>, AdjudicationError> {
    let mut ids = BTreeSet::new();
    for candidate in candidates_in_deterministic_order(candidates) {
        if !ids.insert(candidate.id) {
            return Err(AdjudicationError::DuplicateCandidate {
                candidate_id: candidate.id,
            });
        }
    }
    Ok(ids)
}

fn validate_ballot_ids(ballots: &[AdjudicationBallot]) -> Result<(), AdjudicationError> {
    let mut ids = BTreeSet::new();
    for ballot in ballots_in_deterministic_order(ballots) {
        if !ids.insert(ballot.id) {
            return Err(AdjudicationError::DuplicateBallot {
                ballot_id: ballot.id,
            });
        }
    }
    Ok(())
}

fn candidates_in_deterministic_order(candidates: &[Candidate]) -> Vec<&Candidate> {
    let mut ordered: Vec<_> = candidates.iter().collect();
    ordered.sort_by_key(|candidate| candidate.id);
    ordered
}

fn ballots_in_deterministic_order(ballots: &[AdjudicationBallot]) -> Vec<&AdjudicationBallot> {
    let mut ordered: Vec<_> = ballots.iter().collect();
    ordered.sort_by_key(|ballot| ballot.id);
    ordered
}

fn assessments_in_deterministic_order(ballot: &AdjudicationBallot) -> Vec<&CandidateAssessment> {
    let mut ordered: Vec<_> = ballot.assessments.iter().collect();
    ordered.sort_by_key(|assessment| assessment.candidate_id);
    ordered
}

fn compare_hard_gate_failures(left: &HardGateFailure, right: &HardGateFailure) -> Ordering {
    match (left, right) {
        (HardGateFailure::CandidateState { .. }, HardGateFailure::BallotAssessment { .. }) => {
            Ordering::Less
        }
        (HardGateFailure::BallotAssessment { .. }, HardGateFailure::CandidateState { .. }) => {
            Ordering::Greater
        }
        (
            HardGateFailure::CandidateState { state: left_state },
            HardGateFailure::CandidateState { state: right_state },
        ) => format!("{left_state:?}").cmp(&format!("{right_state:?}")),
        (
            HardGateFailure::BallotAssessment {
                ballot_id: left_ballot,
                validator_agent_id: left_validator,
                notes: left_notes,
            },
            HardGateFailure::BallotAssessment {
                ballot_id: right_ballot,
                validator_agent_id: right_validator,
                notes: right_notes,
            },
        ) => left_ballot
            .cmp(right_ballot)
            .then_with(|| left_validator.cmp(right_validator))
            .then_with(|| left_notes.cmp(right_notes)),
    }
}

fn compare_candidate_tallies(left: &CandidateTally, right: &CandidateTally) -> Ordering {
    right
        .hard_gate_passed
        .cmp(&left.hard_gate_passed)
        .then_with(|| right.approval_count.cmp(&left.approval_count))
        .then_with(|| right.evidence_weight.cmp(&left.evidence_weight))
        .then_with(|| left.risk_score_total.cmp(&right.risk_score_total))
        .then_with(|| {
            left.complexity_score_total
                .cmp(&right.complexity_score_total)
        })
        .then_with(|| left.created_at.cmp(&right.created_at))
        .then_with(|| left.candidate_id.cmp(&right.candidate_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    use crate::ValidatorIdentity;

    fn candidate_id(value: u128) -> CandidateId {
        CandidateId::from_uuid(Uuid::from_u128(value))
    }

    fn ballot_id(value: u128) -> BallotId {
        BallotId::from_uuid(Uuid::from_u128(value))
    }

    fn round_id() -> crate::AdjudicationRoundId {
        crate::AdjudicationRoundId::from_uuid(Uuid::from_u128(900))
    }

    fn timestamp(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).expect("test timestamp is valid")
    }

    fn candidate(value: u128, created_at: i64, state: CandidateState) -> Candidate {
        Candidate {
            id: candidate_id(value),
            candidate_set_id: crate::CandidateSetId::from_uuid(Uuid::from_u128(700)),
            state,
            base_commit: "base".to_string(),
            result_commit: Some(format!("result-{value}")),
            diff_digest: Some(format!("digest-{value}")),
            summary: Some(format!("candidate-{value}")),
            provenance: crate::CandidateProvenance {
                session_id: format!("session-{value}"),
                agent_id: format!("agent-{value}"),
                model_id: Some("model".to_string()),
                work_unit_id: None,
                lineage_digest: Some(format!("lineage-{value}")),
            },
            resource_intents: Vec::new(),
            supersedes_candidate_id: None,
            created_at: timestamp(created_at),
            updated_at: timestamp(created_at),
            submitted_at: Some(timestamp(created_at)),
        }
    }

    fn assessment(
        candidate_id: CandidateId,
        approve: bool,
        eligible: bool,
        acceptance_score: u16,
    ) -> CandidateAssessment {
        CandidateAssessment {
            candidate_id,
            eligible,
            approve,
            acceptance_score,
            risk_score: 10,
            complexity_score: 10,
            notes: vec!["acceptance evidence".to_string()],
        }
    }

    fn ballot(
        value: u128,
        assessments: Vec<CandidateAssessment>,
        abstained: bool,
    ) -> AdjudicationBallot {
        let ranking = assessments
            .iter()
            .map(|assessment| assessment.candidate_id)
            .collect();
        AdjudicationBallot {
            id: ballot_id(value),
            round_id: round_id(),
            validator: ValidatorIdentity {
                session_id: format!("validator-session-{value}"),
                agent_id: format!("validator-agent-{value}"),
                model_id: Some("validator-model".to_string()),
                lineage_digest: format!("validator-lineage-{value}"),
            },
            assessments,
            ranking,
            abstained,
            created_at: timestamp(value as i64),
        }
    }

    fn ensemble() -> ConcurrencyPolicy {
        ConcurrencyPolicy::Ensemble {
            candidate_count: 3,
            quorum: 2,
        }
    }

    #[test]
    fn quorum_reached_selects_the_deterministically_ranked_candidate() {
        let first = candidate(1, 1, CandidateState::Eligible);
        let second = candidate(2, 2, CandidateState::Eligible);
        let third = candidate(3, 3, CandidateState::Eligible);
        let ballots = vec![
            ballot(11, vec![assessment(first.id, true, true, 80)], false),
            ballot(12, vec![assessment(first.id, true, true, 70)], false),
            ballot(13, vec![assessment(second.id, true, true, 100)], false),
        ];
        let first_id = first.id;

        let outcome = adjudicate(&[first, second, third], &ballots, &ensemble()).unwrap();

        assert_eq!(outcome.selected_candidate_id(), Some(first_id));
        assert_eq!(outcome.required_quorum, 2);
        assert_eq!(outcome.usable_ballot_count, 3);
        assert_eq!(outcome.abstention_count, 0);
    }

    #[test]
    fn quorum_impossible_escalates_without_picking_a_winner() {
        let first = candidate(1, 1, CandidateState::Eligible);
        let second = candidate(2, 2, CandidateState::Eligible);
        let third = candidate(3, 3, CandidateState::Eligible);
        let ballots = vec![
            ballot(11, vec![assessment(first.id, true, true, 80)], false),
            ballot(12, vec![assessment(second.id, true, true, 90)], false),
        ];

        let outcome = adjudicate(&[first, second, third], &ballots, &ensemble()).unwrap();

        match outcome.decision {
            AdjudicationPolicyDecision::Escalation { summary } => {
                assert_eq!(
                    summary.reason,
                    EscalationReason::QuorumUnreached {
                        leading_approval_count: 1,
                        required_quorum: 2,
                    }
                );
                assert_eq!(summary.suggested_source_candidate_ids.len(), 2);
            }
            AdjudicationPolicyDecision::Select { candidate_id } => {
                panic!("unexpected selection of {candidate_id}")
            }
        }
    }

    #[test]
    fn tie_breaking_is_total_and_independent_of_shuffled_input_order() {
        let first = candidate(1, 10, CandidateState::Eligible);
        let second = candidate(2, 10, CandidateState::Eligible);
        let third = candidate(3, 20, CandidateState::Eligible);
        let ballots = vec![
            ballot(11, vec![assessment(first.id, true, true, 50)], false),
            ballot(12, vec![assessment(second.id, true, true, 50)], false),
        ];
        let policy = ConcurrencyPolicy::Ensemble {
            candidate_count: 3,
            quorum: 1,
        };

        let original = adjudicate(
            &[first.clone(), second.clone(), third.clone()],
            &ballots,
            &policy,
        )
        .unwrap();
        let shuffled = adjudicate(
            &[third, first.clone(), second.clone()],
            &[ballots[1].clone(), ballots[0].clone()],
            &policy,
        )
        .unwrap();

        assert_eq!(original, shuffled);
        assert_eq!(original.selected_candidate_id(), Some(first.id));
    }

    #[test]
    fn abstentions_do_not_satisfy_round_validity_or_quorum() {
        let first = candidate(1, 1, CandidateState::Eligible);
        let ballots = vec![
            ballot(11, vec![assessment(first.id, true, true, 80)], false),
            ballot(12, Vec::new(), true),
            ballot(13, Vec::new(), true),
        ];

        let outcome = adjudicate(&[first], &ballots, &ensemble()).unwrap();

        assert_eq!(outcome.selected_candidate_id(), None);
        match outcome.decision {
            AdjudicationPolicyDecision::Escalation { summary } => {
                assert_eq!(
                    summary.reason,
                    EscalationReason::AbstentionStarvation {
                        usable_ballots: 1,
                        abstentions: 2,
                        required_valid_ballots: 2,
                    }
                );
            }
            AdjudicationPolicyDecision::Select { .. } => {
                panic!("abstention starvation selected a candidate")
            }
        }
    }

    #[test]
    fn a_failed_hard_gate_vetoes_a_candidate_despite_approvals() {
        let first = candidate(1, 1, CandidateState::Eligible);
        let ballots = vec![
            ballot(11, vec![assessment(first.id, true, false, 100)], false),
            ballot(12, vec![assessment(first.id, true, true, 100)], false),
            ballot(13, vec![assessment(first.id, true, true, 100)], false),
        ];

        let evaluations = evaluate_hard_gates(std::slice::from_ref(&first), &ballots).unwrap();
        assert!(!evaluations[0].passed);
        assert_eq!(evaluations[0].failures.len(), 1);

        let outcome = adjudicate(&[first], &ballots, &ensemble()).unwrap();
        match outcome.decision {
            AdjudicationPolicyDecision::Escalation { summary } => {
                assert_eq!(summary.reason, EscalationReason::NoEligibleCandidate);
                assert_eq!(summary.candidate_tallies[0].approval_count, 2);
                assert!(!summary.candidate_tallies[0].hard_gate_passed);
            }
            AdjudicationPolicyDecision::Select { .. } => panic!("hard-gate veto was ignored"),
        }
    }

    #[test]
    fn escalation_payload_carries_ordered_disagreement_evidence() {
        let first = candidate(1, 1, CandidateState::Eligible);
        let second = candidate(2, 2, CandidateState::Eligible);
        let third = candidate(3, 3, CandidateState::Eligible);
        let ballots = vec![
            ballot(11, vec![assessment(first.id, true, true, 90)], false),
            ballot(12, vec![assessment(second.id, true, true, 90)], false),
            ballot(13, vec![assessment(third.id, true, true, 90)], false),
        ];

        let outcome = adjudicate(
            &[third, first.clone(), second.clone()],
            &ballots,
            &ensemble(),
        )
        .unwrap();

        let AdjudicationPolicyDecision::Escalation { summary } = outcome.decision else {
            panic!("split evidence unexpectedly selected a candidate")
        };
        assert_eq!(summary.required_quorum, 2);
        assert_eq!(summary.round_validity_threshold, 2);
        assert_eq!(summary.usable_ballot_count, 3);
        assert_eq!(summary.abstention_count, 0);
        assert_eq!(summary.candidate_tallies.len(), 3);
        assert_eq!(
            summary.suggested_source_candidate_ids,
            vec![first.id, second.id]
        );
    }

    #[test]
    fn non_abstaining_ballot_without_assessment_reference_is_invalid() {
        let first = candidate(1, 1, CandidateState::Eligible);
        let error =
            adjudicate(&[first], &[ballot(11, Vec::new(), false)], &ensemble()).unwrap_err();

        assert!(matches!(
            error,
            AdjudicationError::MissingAssessmentReference { .. }
        ));
    }
}
