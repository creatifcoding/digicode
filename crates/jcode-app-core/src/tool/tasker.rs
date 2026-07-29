use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use jcode_tasker_store::TaskerStore;
use jcode_tasker_types::{
    AddTaskDependency, CreateFeature, CreateProject, CreateTask, Project, ProjectRevision,
    SetTaskState, TaskPriority, TaskState,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Tool, ToolContext, ToolOutput};

pub struct TaskerTool {
    database_path: Option<PathBuf>,
}

impl TaskerTool {
    pub fn new() -> Self {
        Self {
            database_path: None,
        }
    }

    #[cfg(test)]
    fn with_database_path(database_path: PathBuf) -> Self {
        Self {
            database_path: Some(database_path),
        }
    }

    fn database_path(&self) -> Result<PathBuf> {
        match &self.database_path {
            Some(path) => Ok(path.clone()),
            None => Ok(crate::storage::jcode_dir()?.join("tasker/tasks.db")),
        }
    }

    async fn open_store(&self) -> Result<TaskerStore> {
        TaskerStore::open(self.database_path()?)
            .await
            .context("open native Tasker database")
    }
}

#[derive(Debug, Deserialize)]
struct TaskerInput {
    action: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    feature_id: Option<String>,
    #[serde(default)]
    parent_feature_id: Option<String>,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    depends_on_task_id: Option<String>,
    #[serde(default)]
    state: Option<TaskState>,
    #[serde(default)]
    priority: Option<TaskPriority>,
    #[serde(default)]
    rank: Option<i64>,
    #[serde(default)]
    expected_revision: Option<ProjectRevision>,
}

fn required<T>(value: Option<T>, field: &'static str, action: &str) -> Result<T> {
    value.ok_or_else(|| anyhow!("{field} is required for {action}"))
}

fn canonical_root(ctx: &ToolContext) -> Result<PathBuf> {
    let root = ctx
        .working_dir
        .as_deref()
        .ok_or_else(|| anyhow!("tasker requires a session working directory"))?;
    std::fs::canonicalize(root)
        .with_context(|| format!("canonicalize Tasker project root {}", root.display()))
}

fn project_name(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("project")
        .to_string()
}

async fn project_for_root(
    store: &TaskerStore,
    root: &Path,
    create: bool,
) -> Result<Option<Project>> {
    let root_text = root.to_string_lossy().into_owned();
    if let Some(project) = store.project_by_root(root_text.clone()).await? {
        return Ok(Some(project));
    }
    if !create {
        return Ok(None);
    }
    let created = store
        .create_project(CreateProject {
            id: None,
            name: project_name(root),
            canonical_root: Some(root_text),
        })
        .await?;
    Ok(Some(created.value))
}

fn output(title: impl Into<String>, metadata: Value) -> Result<ToolOutput> {
    Ok(ToolOutput::new(serde_json::to_string_pretty(&metadata)?)
        .with_title(title)
        .with_metadata(metadata))
}

#[async_trait]
impl Tool for TaskerTool {
    fn name(&self) -> &str {
        "tasker"
    }

    fn description(&self) -> &str {
        "Manage durable, dependency-aware project work in Jcode's native Tasker database. This is distinct from session-local todo planning."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "enum": ["status", "ready", "show", "create_feature", "create", "add_dependency", "set_state"]
                },
                "title": {"type": "string"},
                "description": {"type": "string"},
                "feature_id": {"type": "string", "description": "Feature reference: feat_<uuid> or #F<number>."},
                "parent_feature_id": {"type": "string", "description": "Optional parent feature reference."},
                "task_id": {"type": "string", "description": "Task reference: task_<uuid> or #<number>."},
                "depends_on_task_id": {"type": "string", "description": "Prerequisite task reference."},
                "state": {"type": "string", "enum": ["todo", "in_progress", "blocked", "done", "cancelled"]},
                "priority": {"type": "string", "enum": ["low", "normal", "high", "critical"]},
                "rank": {"type": "integer"},
                "expected_revision": {"type": "integer", "minimum": 0}
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: TaskerInput = serde_json::from_value(input)?;
        let root = canonical_root(&ctx)?;
        let store = self.open_store().await?;
        let create_project = matches!(
            params.action.as_str(),
            "create_feature" | "create" | "add_dependency" | "set_state"
        );
        let Some(project) = project_for_root(&store, &root, create_project).await? else {
            return output(
                "Tasker not initialized",
                json!({
                    "initialized": false,
                    "canonical_root": root,
                    "message": "Create a feature or task to initialize durable project state."
                }),
            );
        };

        match params.action.as_str() {
            "status" => {
                let snapshot = store.snapshot(project.id).await?;
                output(
                    format!("Tasker revision {}", snapshot.revision),
                    serde_json::to_value(snapshot)?,
                )
            }
            "ready" => {
                let ready = store.list_ready(project.id).await?;
                output(
                    format!("{} ready tasks", ready.len()),
                    json!({"project": project, "ready": ready}),
                )
            }
            "show" => {
                let task_reference = required(params.task_id, "task_id", "show")?;
                let task = store.resolve_task(project.id, task_reference).await?;
                let task_id = task.id;
                let readiness = store.task_readiness(project.id, task_id).await?;
                let dependencies = store.task_dependencies(project.id, task_id).await?;
                output(
                    format!("{} {}", task.alias, task.title),
                    json!({
                        "project": project,
                        "task": task,
                        "readiness": readiness,
                        "dependencies": dependencies
                    }),
                )
            }
            "create_feature" => {
                let title = required(params.title, "title", "create_feature")?;
                let parent_id = match params.parent_feature_id {
                    Some(reference) => Some(store.resolve_feature(project.id, reference).await?.id),
                    None => None,
                };
                let mutation = store
                    .create_feature(CreateFeature {
                        project_id: project.id,
                        id: None,
                        parent_id,
                        title,
                        description: params.description.unwrap_or_default(),
                        expected_revision: params.expected_revision.or(Some(project.revision)),
                    })
                    .await?;
                output(
                    format!("Created {}", mutation.value.alias),
                    json!({"feature": mutation.value, "revision": mutation.revision, "event_id": mutation.event_id}),
                )
            }
            "create" => {
                let title = required(params.title, "title", "create")?;
                let feature_reference = required(params.feature_id, "feature_id", "create")?;
                let feature_id = store
                    .resolve_feature(project.id, feature_reference)
                    .await?
                    .id;
                let mutation = store
                    .create_task(CreateTask {
                        project_id: project.id,
                        feature_id,
                        id: None,
                        title,
                        description: params.description.unwrap_or_default(),
                        priority: params.priority.unwrap_or_default(),
                        rank: params.rank.unwrap_or_default(),
                        expected_revision: params.expected_revision.or(Some(project.revision)),
                    })
                    .await?;
                output(
                    format!("Created {}", mutation.value.alias),
                    json!({"task": mutation.value, "revision": mutation.revision, "event_id": mutation.event_id}),
                )
            }
            "add_dependency" => {
                let task_reference = required(params.task_id, "task_id", "add_dependency")?;
                let dependency_reference = required(
                    params.depends_on_task_id,
                    "depends_on_task_id",
                    "add_dependency",
                )?;
                let task_id = store.resolve_task(project.id, task_reference).await?.id;
                let depends_on_task_id = store
                    .resolve_task(project.id, dependency_reference)
                    .await?
                    .id;
                let mutation = store
                    .add_task_dependency(AddTaskDependency {
                        project_id: project.id,
                        task_id,
                        depends_on_task_id,
                        expected_revision: params.expected_revision.or(Some(project.revision)),
                    })
                    .await?;
                output(
                    "Added task dependency",
                    json!({"dependency": mutation.value, "revision": mutation.revision, "event_id": mutation.event_id}),
                )
            }
            "set_state" => {
                let task_reference = required(params.task_id, "task_id", "set_state")?;
                let task_id = store.resolve_task(project.id, task_reference).await?.id;
                let state = required(params.state, "state", "set_state")?;
                let mutation = store
                    .set_task_state(SetTaskState {
                        project_id: project.id,
                        task_id,
                        state,
                        expected_revision: params.expected_revision.or(Some(project.revision)),
                    })
                    .await?;
                output(
                    format!("Updated {}", mutation.value.alias),
                    json!({"task": mutation.value, "revision": mutation.revision, "event_id": mutation.event_id}),
                )
            }
            action => Err(anyhow!("unsupported tasker action: {action}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jcode_tool_core::ToolExecutionMode;

    fn context(root: &Path) -> ToolContext {
        ToolContext {
            session_id: "session-test".into(),
            message_id: "message-test".into(),
            tool_call_id: "tool-test".into(),
            working_dir: Some(root.to_path_buf()),
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: ToolExecutionMode::Direct,
        }
    }

    #[test]
    fn schema_exposes_durable_actions_and_not_subagent_semantics() {
        let definition = TaskerTool::new().to_definition();
        assert_eq!(definition.name, "tasker");
        assert!(definition.description.contains("dependency-aware"));
        assert!(
            definition.input_schema["properties"]["action"]["enum"]
                .as_array()
                .expect("action enum")
                .contains(&json!("ready"))
        );
    }

    #[tokio::test]
    async fn initializes_project_and_round_trips_feature_task_and_status() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let database_dir = tempfile::tempdir().expect("database tempdir");
        let tool = TaskerTool::with_database_path(database_dir.path().join("tasker.db"));
        let ctx = context(workspace.path());

        let empty = tool
            .execute(json!({"action": "status"}), ctx.clone())
            .await
            .expect("empty status");
        assert_eq!(
            empty.metadata.expect("empty metadata")["initialized"],
            false
        );

        let feature = tool
            .execute(
                json!({"action": "create_feature", "title": "Native Tasker"}),
                ctx.clone(),
            )
            .await
            .expect("create feature");
        assert_eq!(
            feature.metadata.expect("feature metadata")["feature"]["alias"],
            1
        );

        let task = tool
            .execute(
                json!({
                    "action": "create",
                    "feature_id": "#F1",
                    "title": "Expose tasker tool",
                    "priority": "critical"
                }),
                ctx.clone(),
            )
            .await
            .expect("create task");
        let task_metadata = task.metadata.expect("task metadata");
        assert_eq!(task_metadata["task"]["alias"], 1);
        assert_eq!(task_metadata["task"]["state"], "todo");

        let shown = tool
            .execute(json!({"action": "show", "task_id": "#1"}), ctx.clone())
            .await
            .expect("show task by alias")
            .metadata
            .expect("shown metadata");
        assert_eq!(shown["task"]["title"], "Expose tasker tool");

        let status = tool
            .execute(json!({"action": "status"}), ctx)
            .await
            .expect("populated status")
            .metadata
            .expect("status metadata");
        assert_eq!(status["features"].as_array().expect("features").len(), 1);
        assert_eq!(status["tasks"].as_array().expect("tasks").len(), 1);
        assert_eq!(
            status["ready_tasks"].as_array().expect("ready tasks").len(),
            1
        );
    }
}
