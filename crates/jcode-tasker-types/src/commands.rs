use serde::{Deserialize, Serialize};

use crate::{FeatureId, FeatureState, ProjectId, ProjectRevision, TaskId, TaskPriority, TaskState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateProject {
    #[serde(default)]
    pub id: Option<ProjectId>,
    pub name: String,
    #[serde(default)]
    pub canonical_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateFeature {
    pub project_id: ProjectId,
    #[serde(default)]
    pub id: Option<FeatureId>,
    #[serde(default)]
    pub parent_id: Option<FeatureId>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub expected_revision: Option<ProjectRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTask {
    pub project_id: ProjectId,
    pub feature_id: FeatureId,
    #[serde(default)]
    pub id: Option<TaskId>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub priority: TaskPriority,
    #[serde(default)]
    pub rank: i64,
    #[serde(default)]
    pub expected_revision: Option<ProjectRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddTaskDependency {
    pub project_id: ProjectId,
    pub task_id: TaskId,
    pub depends_on_task_id: TaskId,
    #[serde(default)]
    pub expected_revision: Option<ProjectRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFeatureDependency {
    pub project_id: ProjectId,
    pub feature_id: FeatureId,
    pub depends_on_feature_id: FeatureId,
    #[serde(default)]
    pub expected_revision: Option<ProjectRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetTaskState {
    pub project_id: ProjectId,
    pub task_id: TaskId,
    pub state: TaskState,
    #[serde(default)]
    pub expected_revision: Option<ProjectRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFeatureState {
    pub project_id: ProjectId,
    pub feature_id: FeatureId,
    pub state: FeatureState,
    #[serde(default)]
    pub expected_revision: Option<ProjectRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TaskerCommand {
    CreateProject(CreateProject),
    CreateFeature(CreateFeature),
    CreateTask(CreateTask),
    AddTaskDependency(AddTaskDependency),
    AddFeatureDependency(AddFeatureDependency),
    SetTaskState(SetTaskState),
    SetFeatureState(SetFeatureState),
    Batch(BatchCommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchCommand {
    pub project_id: ProjectId,
    #[serde(default)]
    pub expected_revision: Option<ProjectRevision>,
    pub operations: Vec<MutationCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MutationCommand {
    CreateFeature(CreateFeature),
    CreateTask(CreateTask),
    AddTaskDependency(AddTaskDependency),
    AddFeatureDependency(AddFeatureDependency),
    SetTaskState(SetTaskState),
    SetFeatureState(SetFeatureState),
}
