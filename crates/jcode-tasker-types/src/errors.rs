use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{FeatureState, ProjectRevision, TaskState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum TaskerError {
    InvalidId {
        expected_prefix: String,
        value: String,
    },
    InvalidTransition {
        entity: String,
        from: String,
        to: String,
    },
    InvalidInput {
        field: String,
        message: String,
    },
    NotFound {
        entity: String,
        id: String,
    },
    RevisionConflict {
        expected: ProjectRevision,
        actual: ProjectRevision,
    },
    DependencyCycle {
        entity: String,
        cycle: Vec<String>,
    },
    Conflict {
        message: String,
    },
}

impl TaskerError {
    pub fn invalid_feature_transition(from: FeatureState, to: FeatureState) -> Self {
        Self::InvalidTransition {
            entity: "feature".to_string(),
            from: from.as_str().to_string(),
            to: to.as_str().to_string(),
        }
    }

    pub fn invalid_task_transition(from: TaskState, to: TaskState) -> Self {
        Self::InvalidTransition {
            entity: "task".to_string(),
            from: from.as_str().to_string(),
            to: to.as_str().to_string(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidId { .. } => "invalid_id",
            Self::InvalidTransition { .. } => "invalid_transition",
            Self::InvalidInput { .. } => "invalid_input",
            Self::NotFound { .. } => "not_found",
            Self::RevisionConflict { .. } => "revision_conflict",
            Self::DependencyCycle { .. } => "dependency_cycle",
            Self::Conflict { .. } => "conflict",
        }
    }
}

impl fmt::Display for TaskerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId {
                expected_prefix,
                value,
            } => write!(formatter, "invalid {expected_prefix} identifier: {value}"),
            Self::InvalidTransition { entity, from, to } => {
                write!(
                    formatter,
                    "invalid {entity} state transition from {from} to {to}"
                )
            }
            Self::InvalidInput { field, message } => {
                write!(formatter, "invalid {field}: {message}")
            }
            Self::NotFound { entity, id } => write!(formatter, "{entity} not found: {id}"),
            Self::RevisionConflict { expected, actual } => write!(
                formatter,
                "project revision conflict: expected {expected}, current {actual}"
            ),
            Self::DependencyCycle { entity, cycle } => {
                write!(
                    formatter,
                    "{entity} dependency cycle: {}",
                    cycle.join(" -> ")
                )
            }
            Self::Conflict { message } => formatter.write_str(message),
        }
    }
}

impl std::error::Error for TaskerError {}
