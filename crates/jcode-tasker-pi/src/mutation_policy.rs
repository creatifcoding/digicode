//! Canonical mutation admission for Tasker candidate rounds.
//!
//! The narrow enforcement boundary is `PiTaskerStore::update_task`: it is the
//! compatibility store's canonical task-write path, and its batch equivalent
//! is checked before the batch transaction starts. Candidate authors call the
//! gate with a candidate context and keep their writes in the candidate lane;
//! canonical callers omit that context and are rejected when an active round
//! owns the task or an overlapping write resource.

use crate::{ConcurrencyStore, ConcurrencyStoreError};
use jcode_tasker_types::{
    CandidateSetState, ConcurrencyPolicy, ResourceAccess, ResourceIntent, ResourceKind,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateMutationContext {
    pub candidate_set_id: String,
    pub candidate_id: String,
}

impl CandidateMutationContext {
    pub fn new(candidate_set_id: impl Into<String>, candidate_id: impl Into<String>) -> Self {
        Self {
            candidate_set_id: candidate_set_id.into(),
            candidate_id: candidate_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalMutation {
    pub task_id: String,
    pub resource_intents: Vec<ResourceIntent>,
    pub candidate_context: Option<CandidateMutationContext>,
}

impl CanonicalMutation {
    pub fn new(
        task_id: impl Into<String>,
        resource_intents: Vec<ResourceIntent>,
        candidate_context: Option<CandidateMutationContext>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            resource_intents,
            candidate_context,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSetSnapshot {
    pub id: String,
    pub task_id: String,
    pub policy: ConcurrencyPolicy,
    pub state: CandidateSetState,
    pub candidates: Vec<CandidateSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSnapshot {
    pub id: String,
    pub candidate_set_id: String,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub resource_intents: Vec<ResourceIntent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationPolicyDecision {
    Admit,
    Reject { reason: MutationPolicyRejection },
    AdmitSpeculative { candidate_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MutationPolicyRejection {
    #[error(
        "canonical mutation for task {task_id} conflicts with active candidate set {candidate_set_id} held by {holder} on {resource}"
    )]
    Conflict {
        candidate_set_id: String,
        task_id: String,
        holder: String,
        resource: String,
    },
    #[error(
        "candidate context {candidate_id} is stale because candidate set {candidate_set_id} is {state}"
    )]
    StaleCandidateContext {
        candidate_set_id: String,
        candidate_id: String,
        state: String,
    },
    #[error("candidate context is invalid: {reason}")]
    InvalidCandidateContext { reason: String },
    #[error(
        "candidate set {candidate_set_id} uses exclusive policy and cannot admit candidate {candidate_id}"
    )]
    SpeculationNotPermitted {
        candidate_set_id: String,
        candidate_id: String,
    },
}

#[derive(Debug, Error)]
pub enum MutationPolicyError {
    #[error("mutation policy store lookup failed: {0}")]
    Store(#[from] ConcurrencyStoreError),
    #[error("invalid candidate-set projection {candidate_set_id}: {message}")]
    InvalidProjection {
        candidate_set_id: String,
        message: String,
    },
}

pub type MutationPolicyResult = Result<MutationPolicyDecision, MutationPolicyError>;

pub struct MutationPolicyGate<'a> {
    store: &'a ConcurrencyStore,
}

impl<'a> MutationPolicyGate<'a> {
    pub fn new(store: &'a ConcurrencyStore) -> Self {
        Self { store }
    }

    /// Read active candidate rounds, validate an optional candidate context,
    /// then delegate the admission decision to the pure function below.
    pub fn evaluate(&self, mutation: &CanonicalMutation) -> MutationPolicyResult {
        if let Some(context) = mutation.candidate_context.as_ref() {
            let Some(projection) = self.store.candidate_set(&context.candidate_set_id)? else {
                return Ok(MutationPolicyDecision::Reject {
                    reason: MutationPolicyRejection::StaleCandidateContext {
                        candidate_set_id: context.candidate_set_id.clone(),
                        candidate_id: context.candidate_id.clone(),
                        state: "missing".into(),
                    },
                });
            };
            let snapshot = parse_candidate_set_projection(&projection, &context.candidate_set_id)?;
            if !is_active(snapshot.state) {
                return Ok(MutationPolicyDecision::Reject {
                    reason: MutationPolicyRejection::StaleCandidateContext {
                        candidate_set_id: context.candidate_set_id.clone(),
                        candidate_id: context.candidate_id.clone(),
                        state: state_name(snapshot.state).into(),
                    },
                });
            }
            if snapshot.task_id != mutation.task_id {
                return Ok(MutationPolicyDecision::Reject {
                    reason: MutationPolicyRejection::InvalidCandidateContext {
                        reason: format!(
                            "candidate set {} belongs to task {}, not {}",
                            context.candidate_set_id, snapshot.task_id, mutation.task_id
                        ),
                    },
                });
            }
            let Some(candidate_projection) = self.store.candidate(&context.candidate_id)? else {
                return Ok(MutationPolicyDecision::Reject {
                    reason: MutationPolicyRejection::InvalidCandidateContext {
                        reason: format!(
                            "candidate {} does not belong to candidate set {}",
                            context.candidate_id, context.candidate_set_id
                        ),
                    },
                });
            };
            let candidate: StoredCandidate =
                serde_json::from_value(candidate_projection).map_err(|error| {
                    MutationPolicyError::InvalidProjection {
                        candidate_set_id: context.candidate_set_id.clone(),
                        message: error.to_string(),
                    }
                })?;
            if candidate.candidate_set_id != context.candidate_set_id {
                return Ok(MutationPolicyDecision::Reject {
                    reason: MutationPolicyRejection::InvalidCandidateContext {
                        reason: format!(
                            "candidate {} belongs to candidate set {}, not {}",
                            context.candidate_id,
                            candidate.candidate_set_id,
                            context.candidate_set_id
                        ),
                    },
                });
            }
        }

        let projections = self.store.active_candidate_set_projections()?;
        let active_sets = projections
            .iter()
            .map(|projection| parse_candidate_set_projection(projection, "active"))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(decide_mutation_policy(mutation, &active_sets))
    }
}

/// Pure policy decision logic. It intentionally knows nothing about SQLite,
/// revisions, or how the candidate projections were obtained.
pub fn decide_mutation_policy(
    mutation: &CanonicalMutation,
    active_sets: &[CandidateSetSnapshot],
) -> MutationPolicyDecision {
    if let Some(context) = mutation.candidate_context.as_ref() {
        let Some(candidate_set) = active_sets
            .iter()
            .find(|candidate_set| candidate_set.id == context.candidate_set_id)
        else {
            return MutationPolicyDecision::Reject {
                reason: MutationPolicyRejection::StaleCandidateContext {
                    candidate_set_id: context.candidate_set_id.clone(),
                    candidate_id: context.candidate_id.clone(),
                    state: "resolved_or_abandoned".into(),
                },
            };
        };
        if !is_active(candidate_set.state) {
            return MutationPolicyDecision::Reject {
                reason: MutationPolicyRejection::StaleCandidateContext {
                    candidate_set_id: context.candidate_set_id.clone(),
                    candidate_id: context.candidate_id.clone(),
                    state: state_name(candidate_set.state).into(),
                },
            };
        }
        if candidate_set.task_id != mutation.task_id {
            return MutationPolicyDecision::Reject {
                reason: MutationPolicyRejection::InvalidCandidateContext {
                    reason: format!(
                        "candidate set {} belongs to task {}, not {}",
                        context.candidate_set_id, candidate_set.task_id, mutation.task_id
                    ),
                },
            };
        }
        if !candidate_set
            .candidates
            .iter()
            .any(|candidate| candidate.id == context.candidate_id)
        {
            return MutationPolicyDecision::Reject {
                reason: MutationPolicyRejection::InvalidCandidateContext {
                    reason: format!(
                        "candidate {} does not belong to candidate set {}",
                        context.candidate_id, context.candidate_set_id
                    ),
                },
            };
        }
        if !candidate_set.policy.permits_parallel_authorship() {
            return MutationPolicyDecision::Reject {
                reason: MutationPolicyRejection::SpeculationNotPermitted {
                    candidate_set_id: context.candidate_set_id.clone(),
                    candidate_id: context.candidate_id.clone(),
                },
            };
        }
        return MutationPolicyDecision::AdmitSpeculative {
            candidate_id: context.candidate_id.clone(),
        };
    }

    for candidate_set in active_sets.iter().filter(|set| is_active(set.state)) {
        let Some((resource, holder)) = conflicting_resource(mutation, candidate_set) else {
            continue;
        };
        return MutationPolicyDecision::Reject {
            reason: MutationPolicyRejection::Conflict {
                candidate_set_id: candidate_set.id.clone(),
                task_id: mutation.task_id.clone(),
                holder,
                resource,
            },
        };
    }

    MutationPolicyDecision::Admit
}

fn conflicting_resource(
    mutation: &CanonicalMutation,
    candidate_set: &CandidateSetSnapshot,
) -> Option<(String, String)> {
    if mutation.task_id == candidate_set.task_id {
        let holder = candidate_set
            .candidates
            .first()
            .map(holder_name)
            .unwrap_or_else(|| format!("candidate set {}", candidate_set.id));
        return Some((format!("task {}", mutation.task_id), holder));
    }
    for candidate in &candidate_set.candidates {
        for active_intent in &candidate.resource_intents {
            for proposed_intent in &mutation.resource_intents {
                if intents_conflict(active_intent, proposed_intent) {
                    return Some((
                        format!(
                            "{}:{}",
                            resource_kind_name(proposed_intent.kind),
                            proposed_intent.selector
                        ),
                        holder_name(candidate),
                    ));
                }
            }
        }
    }
    None
}

fn holder_name(candidate: &CandidateSnapshot) -> String {
    if let Some(agent_id) = candidate.agent_id.as_deref() {
        return format!("agent {agent_id} (candidate {})", candidate.id);
    }
    if let Some(session_id) = candidate.session_id.as_deref() {
        return format!("session {session_id} (candidate {})", candidate.id);
    }
    format!("candidate {}", candidate.id)
}

fn intents_conflict(active: &ResourceIntent, proposed: &ResourceIntent) -> bool {
    if active.access != ResourceAccess::ProposeWrite
        || proposed.access != ResourceAccess::ProposeWrite
    {
        return false;
    }
    match (active.kind, proposed.kind) {
        (
            ResourceKind::File | ResourceKind::Directory,
            ResourceKind::File | ResourceKind::Directory,
        ) => path_selectors_overlap(&active.selector, &proposed.selector),
        (ResourceKind::Task, ResourceKind::Task)
        | (ResourceKind::Schema, ResourceKind::Schema)
        | (ResourceKind::External, ResourceKind::External) => active.selector == proposed.selector,
        _ => false,
    }
}

fn path_selectors_overlap(left: &str, right: &str) -> bool {
    let left = left.trim_end_matches('/');
    let right = right.trim_end_matches('/');
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn resource_kind_name(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::File => "file",
        ResourceKind::Directory => "directory",
        ResourceKind::Task => "task",
        ResourceKind::Schema => "schema",
        ResourceKind::External => "external",
    }
}

fn is_active(state: CandidateSetState) -> bool {
    matches!(
        state,
        CandidateSetState::Open | CandidateSetState::Adjudicating | CandidateSetState::Promoting
    )
}

fn state_name(state: CandidateSetState) -> &'static str {
    match state {
        CandidateSetState::Open => "open",
        CandidateSetState::Adjudicating => "adjudicating",
        CandidateSetState::Decided => "decided",
        CandidateSetState::Promoting => "promoting",
        CandidateSetState::Completed => "completed",
        CandidateSetState::Cancelled => "cancelled",
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCandidateSet {
    id: String,
    task_id: String,
    policy: ConcurrencyPolicy,
    state: CandidateSetState,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCandidate {
    id: String,
    candidate_set_id: String,
    #[serde(default)]
    resource_intents: Vec<ResourceIntent>,
    #[serde(default)]
    provenance: StoredProvenance,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProvenance {
    agent_id: Option<String>,
    session_id: Option<String>,
}

fn parse_candidate_set_projection(
    projection: &serde_json::Value,
    fallback_id: &str,
) -> Result<CandidateSetSnapshot, MutationPolicyError> {
    let set_value = projection.get("candidateSet").unwrap_or(projection);
    let set: StoredCandidateSet = serde_json::from_value(set_value.clone()).map_err(|error| {
        MutationPolicyError::InvalidProjection {
            candidate_set_id: set_value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(fallback_id)
                .into(),
            message: error.to_string(),
        }
    })?;
    let candidates = projection
        .get("candidates")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|value| {
            serde_json::from_value::<StoredCandidate>(value).map(|candidate| CandidateSnapshot {
                id: candidate.id,
                candidate_set_id: candidate.candidate_set_id,
                agent_id: candidate.provenance.agent_id,
                session_id: candidate.provenance.session_id,
                resource_intents: candidate.resource_intents,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| MutationPolicyError::InvalidProjection {
            candidate_set_id: set.id.clone(),
            message: error.to_string(),
        })?;
    Ok(CandidateSetSnapshot {
        id: set.id,
        task_id: set.task_id,
        policy: set.policy,
        state: set.state,
        candidates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write_intent(selector: &str) -> ResourceIntent {
        ResourceIntent {
            kind: ResourceKind::File,
            selector: selector.into(),
            access: ResourceAccess::ProposeWrite,
            rationale: None,
        }
    }

    fn active_set(task_id: &str, policy: ConcurrencyPolicy) -> CandidateSetSnapshot {
        CandidateSetSnapshot {
            id: "set-1".into(),
            task_id: task_id.into(),
            policy,
            state: CandidateSetState::Open,
            candidates: vec![CandidateSnapshot {
                id: "candidate-1".into(),
                candidate_set_id: "set-1".into(),
                agent_id: Some("holder-agent".into()),
                session_id: Some("holder-session".into()),
                resource_intents: vec![write_intent("src/lib.rs")],
            }],
        }
    }

    #[test]
    fn exclusive_conflict_rejection_names_holder() {
        let mutation = CanonicalMutation::new("task-1", vec![write_intent("src/lib.rs")], None);
        let decision = decide_mutation_policy(
            &mutation,
            &[active_set("task-1", ConcurrencyPolicy::Exclusive)],
        );
        let MutationPolicyDecision::Reject { reason } = decision else {
            panic!("expected conflict rejection");
        };
        assert!(matches!(reason, MutationPolicyRejection::Conflict { .. }));
        assert!(reason.to_string().contains("holder-agent"));
    }

    #[test]
    fn valid_candidate_context_is_admitted_speculatively() {
        let mutation = CanonicalMutation::new(
            "task-1",
            vec![write_intent("src/lib.rs")],
            Some(CandidateMutationContext::new("set-1", "candidate-1")),
        );
        assert_eq!(
            decide_mutation_policy(
                &mutation,
                &[active_set(
                    "task-1",
                    ConcurrencyPolicy::Speculative { max_candidates: 2 },
                )],
            ),
            MutationPolicyDecision::AdmitSpeculative {
                candidate_id: "candidate-1".into(),
            }
        );
    }

    #[test]
    fn unrelated_resource_is_admitted() {
        let mutation = CanonicalMutation::new("task-2", vec![write_intent("src/other.rs")], None);
        assert_eq!(
            decide_mutation_policy(
                &mutation,
                &[active_set(
                    "task-1",
                    ConcurrencyPolicy::Speculative { max_candidates: 2 },
                )],
            ),
            MutationPolicyDecision::Admit
        );
    }

    #[test]
    fn stale_candidate_context_is_rejected_distinctly() {
        let projection = json!({
            "candidateSet": {
                "id": "set-1",
                "taskId": "task-1",
                "policy": {"kind": "speculative", "max_candidates": 2},
                "state": "completed"
            },
            "candidates": [{
                "id": "candidate-1",
                "candidateSetId": "set-1",
                "resourceIntents": [],
                "provenance": {"agentId": "holder-agent", "sessionId": "holder-session"}
            }]
        });
        let snapshot = parse_candidate_set_projection(&projection, "set-1").unwrap();
        let mutation = CanonicalMutation::new(
            "task-1",
            vec![],
            Some(CandidateMutationContext::new("set-1", "candidate-1")),
        );
        let decision = decide_mutation_policy(&mutation, &[snapshot]);
        assert!(matches!(
            decision,
            MutationPolicyDecision::Reject {
                reason: MutationPolicyRejection::StaleCandidateContext { .. }
            }
        ));
    }

    #[test]
    fn gate_rejects_resolved_context_from_store() {
        let mut store = ConcurrencyStore::open_in_memory("/repo/root").unwrap();
        store
            .create_candidate_set(
                json!({
                    "id": "set-1",
                    "projectId": "project-1",
                    "taskId": "task-1",
                    "baseRevision": 0,
                    "baseCommit": "base-commit",
                    "acceptanceDigest": "acceptance-digest",
                    "policy": {"kind": "speculative", "max_candidates": 2},
                    "policyVersion": 1,
                    "state": "open"
                }),
                0,
            )
            .unwrap();
        store
            .register_candidate(
                json!({
                    "id": "candidate-1",
                    "candidateSetId": "set-1",
                    "state": "authoring",
                    "baseCommit": "base-commit",
                    "provenance": {"agentId": "holder-agent"},
                    "resourceIntents": []
                }),
                1,
            )
            .unwrap();
        store
            .set_candidate_set_state("set-1", "completed", 2)
            .unwrap();

        let mutation = CanonicalMutation::new(
            "task-1",
            vec![write_intent("src/lib.rs")],
            Some(CandidateMutationContext::new("set-1", "candidate-1")),
        );
        let decision = MutationPolicyGate::new(&store).evaluate(&mutation).unwrap();
        assert!(matches!(
            decision,
            MutationPolicyDecision::Reject {
                reason: MutationPolicyRejection::StaleCandidateContext { .. }
            }
        ));
    }
}
