use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    AdjudicationRoundId, BallotId, CandidateId, CandidateSetId, ProjectId, ProjectRevision,
    PromotionIntentId, TaskId, TaskerError,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConcurrencyPolicy {
    #[default]
    Exclusive,
    Speculative {
        max_candidates: u16,
    },
    Ensemble {
        candidate_count: u16,
        quorum: u16,
    },
}

impl ConcurrencyPolicy {
    pub fn validate(&self) -> Result<(), TaskerError> {
        match self {
            Self::Exclusive => Ok(()),
            Self::Speculative { max_candidates } if *max_candidates > 0 => Ok(()),
            Self::Ensemble {
                candidate_count,
                quorum,
            } if *candidate_count > 1 && *quorum > 0 && quorum <= candidate_count => Ok(()),
            Self::Speculative { .. } => Err(TaskerError::InvalidInput {
                field: "max_candidates".into(),
                message: "must be greater than zero".into(),
            }),
            Self::Ensemble { .. } => Err(TaskerError::InvalidInput {
                field: "ensemble".into(),
                message:
                    "candidate_count must exceed one and quorum must be within the candidate count"
                        .into(),
            }),
        }
    }

    pub const fn candidate_limit(&self) -> u16 {
        match self {
            Self::Exclusive => 1,
            Self::Speculative { max_candidates } => *max_candidates,
            Self::Ensemble {
                candidate_count, ..
            } => *candidate_count,
        }
    }

    pub const fn permits_parallel_authorship(&self) -> bool {
        !matches!(self, Self::Exclusive)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSetState {
    #[default]
    Open,
    Adjudicating,
    Decided,
    Promoting,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateState {
    #[default]
    Registered,
    Authoring,
    Submitted,
    Validating,
    Eligible,
    Rejected,
    Failed,
    Selected,
    Superseded,
    Promoted,
}

impl CandidateState {
    pub const fn is_immutable(self) -> bool {
        matches!(
            self,
            Self::Submitted
                | Self::Validating
                | Self::Eligible
                | Self::Rejected
                | Self::Failed
                | Self::Selected
                | Self::Superseded
                | Self::Promoted
        )
    }

    pub const fn is_eligible_for_adjudication(self) -> bool {
        matches!(self, Self::Eligible | Self::Selected)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    File,
    Directory,
    Task,
    Schema,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAccess {
    Read,
    ProposeWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceIntent {
    pub kind: ResourceKind,
    pub selector: String,
    pub access: ResourceAccess,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateSet {
    pub id: CandidateSetId,
    pub project_id: ProjectId,
    pub task_id: TaskId,
    pub base_revision: ProjectRevision,
    pub base_commit: String,
    pub acceptance_digest: String,
    pub policy: ConcurrencyPolicy,
    pub policy_version: u32,
    pub state: CandidateSetState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateProvenance {
    pub session_id: String,
    pub agent_id: String,
    pub model_id: Option<String>,
    pub work_unit_id: Option<String>,
    pub lineage_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: CandidateId,
    pub candidate_set_id: CandidateSetId,
    pub state: CandidateState,
    pub base_commit: String,
    pub result_commit: Option<String>,
    pub diff_digest: Option<String>,
    pub summary: Option<String>,
    pub provenance: CandidateProvenance,
    pub resource_intents: Vec<ResourceIntent>,
    pub supersedes_candidate_id: Option<CandidateId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatorIdentity {
    pub session_id: String,
    pub agent_id: String,
    pub model_id: Option<String>,
    pub lineage_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateAssessment {
    pub candidate_id: CandidateId,
    pub eligible: bool,
    pub approve: bool,
    pub acceptance_score: u16,
    pub risk_score: u16,
    pub complexity_score: u16,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjudicationBallot {
    pub id: BallotId,
    pub round_id: AdjudicationRoundId,
    pub validator: ValidatorIdentity,
    pub assessments: Vec<CandidateAssessment>,
    pub ranking: Vec<CandidateId>,
    pub abstained: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AdjudicationDecision {
    Select {
        candidate_id: CandidateId,
    },
    Synthesize {
        source_candidate_ids: Vec<CandidateId>,
    },
    Blocked {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjudicationRound {
    pub id: AdjudicationRoundId,
    pub candidate_set_id: CandidateSetId,
    pub policy_version: u32,
    pub required_quorum: u16,
    pub decision: Option<AdjudicationDecision>,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotionState {
    #[default]
    Prepared,
    RefUpdated,
    Finalized,
    Aborted,
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionIntent {
    pub id: PromotionIntentId,
    pub project_id: ProjectId,
    pub task_id: TaskId,
    pub candidate_set_id: CandidateSetId,
    pub candidate_id: CandidateId,
    pub expected_revision: ProjectRevision,
    pub expected_commit: String,
    pub target_commit: String,
    pub canonical_ref: String,
    pub state: PromotionState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finalized_at: Option<DateTime<Utc>>,
    pub conflict_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrency_policy_rejects_impossible_ensembles() {
        assert!(
            ConcurrencyPolicy::Ensemble {
                candidate_count: 3,
                quorum: 2,
            }
            .validate()
            .is_ok()
        );
        assert!(
            ConcurrencyPolicy::Ensemble {
                candidate_count: 3,
                quorum: 4,
            }
            .validate()
            .is_err()
        );
        assert!(
            ConcurrencyPolicy::Speculative { max_candidates: 0 }
                .validate()
                .is_err()
        );
    }

    #[test]
    fn only_canonical_exclusive_policy_forbids_parallel_authorship() {
        assert!(!ConcurrencyPolicy::Exclusive.permits_parallel_authorship());
        assert!(ConcurrencyPolicy::Speculative { max_candidates: 2 }.permits_parallel_authorship());
        assert!(
            ConcurrencyPolicy::Ensemble {
                candidate_count: 3,
                quorum: 2,
            }
            .permits_parallel_authorship()
        );
    }

    #[test]
    fn submitted_candidates_are_immutable_and_adjudication_is_narrower() {
        assert!(CandidateState::Submitted.is_immutable());
        assert!(CandidateState::Eligible.is_eligible_for_adjudication());
        assert!(!CandidateState::Rejected.is_eligible_for_adjudication());
        assert!(!CandidateState::Authoring.is_immutable());
    }
}
