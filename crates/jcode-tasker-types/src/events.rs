use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{FeatureId, OutboxEventId, ProjectId, ProjectRevision, TaskId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    ProjectCreated,
    FeatureCreated,
    FeatureStateChanged,
    FeatureDependencyAdded,
    TaskCreated,
    TaskStateChanged,
    TaskDependencyAdded,
}

impl ChangeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProjectCreated => "project_created",
            Self::FeatureCreated => "feature_created",
            Self::FeatureStateChanged => "feature_state_changed",
            Self::FeatureDependencyAdded => "feature_dependency_added",
            Self::TaskCreated => "task_created",
            Self::TaskStateChanged => "task_state_changed",
            Self::TaskDependencyAdded => "task_dependency_added",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub kind: ChangeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_id: Option<FeatureId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEvent {
    pub id: OutboxEventId,
    pub project_id: ProjectId,
    pub revision: ProjectRevision,
    pub change: ChangeSummary,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableOutboxEvent {
    pub event: ProjectEvent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched_at: Option<DateTime<Utc>>,
}
