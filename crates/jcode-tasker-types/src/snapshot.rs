use serde::{Deserialize, Serialize};

use crate::{Feature, Project, ProjectRevision, Task, TaskId};

pub const INITIAL_READINESS_POLICY_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessExplanation {
    pub task_id: TaskId,
    pub ready: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub satisfied_dependencies: Vec<TaskId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsatisfied_dependencies: Vec<TaskId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restrictions: Vec<String>,
    pub revision: ProjectRevision,
    pub policy_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadyTask {
    pub task: Task,
    pub explanation: ReadinessExplanation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSnapshot {
    pub project: Project,
    pub revision: ProjectRevision,
    pub features: Vec<Feature>,
    pub tasks: Vec<Task>,
    pub ready_tasks: Vec<ReadyTask>,
    pub policy_version: u32,
}

impl ProjectSnapshot {
    pub fn task(&self, task_id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == task_id)
    }

    pub fn ready_task(&self, task_id: TaskId) -> Option<&ReadyTask> {
        self.ready_tasks
            .iter()
            .find(|ready| ready.task.id == task_id)
    }
}
