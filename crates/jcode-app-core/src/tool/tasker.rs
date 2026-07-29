use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use jcode_tasker_pi::{
    BatchOperation, ClaimTaskInput, CreateFeature, CreateTask, NextWorkUnitInput, PiTaskerStore,
    PlanTask, ProjectPartition, ReleaseClaimInput, Task, UpdateFeature, UpdateTask,
    WorkContextInput,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Tool, ToolContext, ToolOutput};

const BACKEND: &str = "jcode-tasker-pi";
const TASK_STATES: &[&str] = &["todo", "in_progress", "blocked", "done"];
const FEATURE_STATES: &[&str] = &["open", "active", "closed", "archived"];
const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;
const FEATURE_PRIORITIES: &[&str] = &["low", "medium", "high", "critical"];

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

    fn database_path(&self) -> PathBuf {
        self.database_path
            .clone()
            .unwrap_or_else(jcode_tasker_pi::default_db_path)
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
    state: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    rank: Option<i64>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tags: Option<Value>,
    #[serde(default)]
    brief: Option<String>,
    #[serde(default)]
    acceptance: Option<Value>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    gates: Option<Value>,
    #[serde(default)]
    indexes: Option<Value>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    operations: Option<Vec<BatchOperation>>,
    #[serde(default)]
    tasks: Option<Vec<PlanTask>>,
    #[serde(default)]
    claim_id: Option<String>,
    #[serde(default)]
    claim_kind: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    lease_ms: Option<i64>,
    #[serde(default)]
    release_all: Option<bool>,
    #[serde(default)]
    work_priority: Option<i64>,
    #[serde(default)]
    set_active: Option<bool>,
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

fn validate_enum(value: &str, allowed: &[&str], field: &str) -> Result<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(anyhow!(
            "invalid {field}: {value}; expected one of {}",
            allowed.join(", ")
        ))
    }
}

fn task_title(task: &Task) -> String {
    format!("#{} {}", task.display_id, task.title)
}

fn bounded_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn output(title: impl Into<String>, metadata: Value) -> Result<ToolOutput> {
    Ok(ToolOutput::new(serde_json::to_string_pretty(&metadata)?)
        .with_title(title)
        .with_metadata(metadata))
}

fn provider_metadata(store: &PiTaskerStore) -> Result<Value> {
    let partition = store.partition();
    Ok(json!({
        "backend": BACKEND,
        "database_path": partition.db_path,
        "list_id": partition.list_id,
        "project_root": partition.project_root,
        "schema_fingerprint": store.schema_fingerprint()?,
    }))
}

fn resolve_task(store: &PiTaskerStore, reference: String, action: &str) -> Result<String> {
    store
        .resolve_task_id(&reference)?
        .ok_or_else(|| anyhow!("task_id {reference:?} was not found for {action}"))
}

fn resolve_feature(store: &PiTaskerStore, reference: String, action: &str) -> Result<String> {
    store
        .resolve_feature_id(&reference)?
        .ok_or_else(|| anyhow!("feature_id {reference:?} was not found for {action}"))
}

fn task_ready(store: &PiTaskerStore, task_id: &str) -> Result<Value> {
    let tasks = store.list_tasks(None)?;
    let deps = store.list_dependencies()?;
    let done = tasks.iter().filter(|t| t.state == "done").map(|t| &t.id);
    let done = done.collect::<std::collections::BTreeSet<_>>();
    let blocking = deps
        .iter()
        .filter(|dep| dep.task_id == task_id && !done.contains(&dep.depends_on_id))
        .collect::<Vec<_>>();
    Ok(json!({
        "ready": blocking.is_empty(),
        "blocked_by": blocking,
    }))
}

fn run_pi<R, F>(db_path: PathBuf, root: PathBuf, f: F) -> tokio::task::JoinHandle<Result<R>>
where
    R: Send + 'static,
    F: FnOnce(&mut PiTaskerStore) -> Result<R> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("create Pi tasker database directory {}", parent.display())
            })?;
        }
        let partition = ProjectPartition::with_db_path(
            db_path,
            jcode_tasker_pi::canonical_project_root(root.as_path()),
        );
        let mut store = PiTaskerStore::open(partition).context("open Pi Tasker database")?;
        f(&mut store)
    })
}

fn work_context(ctx: &ToolContext) -> WorkContextInput {
    let agent_id = format!("session:{}", ctx.session_id);
    let session_instance_id = format!("jcode:{}", ctx.session_id);
    WorkContextInput {
        agent_id,
        session_id: ctx.session_id.clone(),
        session_instance_id,
        session_file: None,
        pid: i64::from(std::process::id()),
        model: None,
        leaf_id_at_start: None,
        current_leaf_id: Some(ctx.message_id.clone()),
    }
}

#[async_trait]
impl Tool for TaskerTool {
    fn name(&self) -> &str {
        "tasker"
    }

    fn description(&self) -> &str {
        "Manage durable, dependency-aware project work in the canonical Pi-compatible Tasker SQLite backend at ~/.pi/tasker/tasks.db, partitioned exactly like Pi by list_id and project_root. This public tool uses Pi compatibility while Jcode-native Tasker crates remain for the future native-superset direction."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "additionalProperties": false,
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "enum": [
                        "status", "ready", "show", "create_feature", "create", "add_dependency", "set_state",
                        "list", "search", "update", "add_note", "feature_list", "feature_update", "feature_status",
                        "link", "unlink", "batch", "plan", "claim", "release", "working_set", "next_work_unit"
                    ]
                },
                "title": {"type": "string"},
                "description": {"type": "string"},
                "feature_id": {"type": "string", "description": "Feature reference: feat_<id>, F<number>, or #F<number>."},
                "parent_feature_id": {"type": "string", "description": "Optional parent feature reference."},
                "task_id": {"type": "string", "description": "Task reference: task_<id>, <number>, or #<number>."},
                "depends_on_task_id": {"type": "string", "description": "Prerequisite task reference."},
                "state": {"type": "string", "enum": ["todo", "in_progress", "blocked", "done", "open", "active", "closed", "archived"]},
                "priority": {"type": "string", "enum": ["low", "normal", "medium", "high", "critical"]},
                "rank": {"type": "integer", "description": "Accepted for compatibility; Pi stores order as display_id."},
                "query": {"type": "string"},
                "category": {"type": "string"},
                "content": {"type": "string"},
                "tags": {"type": ["array", "object"]},
                "brief": {"type": "string"},
                "acceptance": {"type": ["array", "object"]},
                "owner": {"type": "string"},
                "gates": {"type": ["array", "object"]},
                "indexes": {"type": ["array", "object"]},
                "operations": {
                    "type": "array",
                    "description": "Atomic Pi-compatible create/update operations. Create keys may be referenced by later dependencies and updates.",
                    "items": {
                        "type": "object",
                        "required": ["op"],
                        "additionalProperties": false,
                        "properties": {
                            "op": {"type": "string", "enum": ["create", "update"]},
                            "key": {"type": "string"},
                            "taskId": {"type": "string"},
                            "title": {"type": "string"},
                            "description": {"type": ["string", "null"]},
                            "state": {"type": "string", "enum": ["todo", "in_progress", "blocked", "done"]},
                            "dependsOn": {"type": "array", "items": {"type": "string"}},
                            "notes": {"type": "array", "items": {"type": "object", "required": ["content"], "additionalProperties": false, "properties": {"content": {"type": "string"}, "category": {"type": "string"}}}},
                            "indexes": {"type": ["array", "object"]},
                            "clearDependencies": {"type": "boolean"},
                            "active": {"type": "boolean"}
                        }
                    }
                },
                "tasks": {
                    "type": "array",
                    "description": "Atomic pure-creation task plan. Each after entry references another task key in this plan.",
                    "items": {
                        "type": "object",
                        "required": ["key", "title"],
                        "additionalProperties": false,
                        "properties": {
                            "key": {"type": "string"},
                            "title": {"type": "string"},
                            "description": {"type": "string"},
                            "state": {"type": "string", "enum": ["todo", "in_progress", "blocked", "done"]},
                            "after": {"type": "array", "items": {"type": "string"}},
                            "notes": {"type": "array", "items": {"type": "object", "required": ["content"], "additionalProperties": false, "properties": {"content": {"type": "string"}, "category": {"type": "string"}}}},
                            "indexes": {"type": ["array", "object"]}
                        }
                    }
                },
                "claim_id": {"type": "string", "description": "Claim identifier returned by claim or next_work_unit."},
                "claim_kind": {"type": "string", "enum": ["claim", "hold", "lock"]},
                "reason": {"type": "string"},
                "lease_ms": {"type": "integer", "minimum": 1},
                "release_all": {"type": "boolean"},
                "work_priority": {"type": "integer", "description": "Queue priority for next_work_unit."},
                "set_active": {"type": "boolean", "description": "Whether next_work_unit should activate a todo task. Defaults to true in Pi-compatible behavior."},
                "limit": {"type": "integer", "minimum": 1, "maximum": 500, "description": "Maximum records returned by bounded list, search, and status projections."}
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: TaskerInput = serde_json::from_value(input)?;
        let _compat_ignored = params.rank;
        let root = canonical_root(&ctx)?;
        let db_path = self.database_path();
        let action = params.action.clone();
        let work_context = work_context(&ctx);
        run_pi(db_path, root, move |store| {
            let base = provider_metadata(store)?;
            let limit = bounded_limit(params.limit);
            let with_base = |payload: Value| -> Value {
                let mut merged = base.clone();
                if let (Some(dst), Some(src)) = (merged.as_object_mut(), payload.as_object()) {
                    dst.extend(src.clone());
                }
                merged
            };

            match action.as_str() {
                "status" => {
                    let snapshot = store.snapshot()?;
                    let task_count = snapshot.tasks.len();
                    let feature_count = snapshot.features.len();
                    let dependency_count = snapshot.dependencies.len();
                    let note_count = snapshot.task_notes.len() + snapshot.feature_notes.len();
                    let ready_tasks = store.ready_tasks()?;
                    let ready_count = ready_tasks.len();
                    output(
                        format!("Pi Tasker {task_count} tasks"),
                        with_base(json!({
                            "initialized": snapshot.list_meta.is_some(),
                            "list_meta": snapshot.list_meta,
                            "counts": {
                                "tasks": task_count,
                                "features": feature_count,
                                "dependencies": dependency_count,
                                "notes": note_count,
                                "ready": ready_count,
                            },
                            "ready_tasks": ready_tasks.into_iter().take(limit).collect::<Vec<_>>(),
                            "truncated": ready_count > limit,
                            "limit": limit,
                        })),
                    )
                }
                "ready" => {
                    let ready = store.ready_tasks()?;
                    let total = ready.len();
                    output(
                        format!("{total} ready tasks"),
                        with_base(json!({
                            "ready": ready.into_iter().take(limit).collect::<Vec<_>>(),
                            "total": total,
                            "truncated": total > limit,
                            "limit": limit,
                        })),
                    )
                }
                "list" => {
                    if let Some(state) = params.state.as_deref() {
                        validate_enum(state, TASK_STATES, "state")?;
                    }
                    let tasks = store.list_tasks(params.state.as_deref())?;
                    let total = tasks.len();
                    output(
                        format!("{total} tasks"),
                        with_base(json!({
                            "tasks": tasks.into_iter().take(limit).collect::<Vec<_>>(),
                            "total": total,
                            "truncated": total > limit,
                            "limit": limit,
                        })),
                    )
                }
                "search" => {
                    let query = required(params.query, "query", "search")?;
                    if let Some(state) = params.state.as_deref() {
                        validate_enum(state, TASK_STATES, "state")?;
                    }
                    let tasks = store.search_tasks(&query, params.state.as_deref())?;
                    let total = tasks.len();
                    output(
                        format!("{total} matching tasks"),
                        with_base(json!({
                            "query": query,
                            "tasks": tasks.into_iter().take(limit).collect::<Vec<_>>(),
                            "total": total,
                            "truncated": total > limit,
                            "limit": limit,
                        })),
                    )
                }
                "show" => {
                    let task_id =
                        resolve_task(store, required(params.task_id, "task_id", "show")?, "show")?;
                    let task = store
                        .list_tasks(None)?
                        .into_iter()
                        .find(|t| t.id == task_id)
                        .ok_or_else(|| anyhow!("task disappeared while showing it"))?;
                    let dependencies = store
                        .list_dependencies()?
                        .into_iter()
                        .filter(|dep| dep.task_id == task.id)
                        .collect::<Vec<_>>();
                    let notes = store
                        .list_task_notes()?
                        .into_iter()
                        .filter(|note| note.task_id == task.id)
                        .collect::<Vec<_>>();
                    output(
                        task_title(&task),
                        with_base(json!({
                            "task": task,
                            "readiness": task_ready(store, &task_id)?,
                            "dependencies": dependencies,
                            "notes": notes,
                        })),
                    )
                }
                "create_feature" => {
                    let parent_feature_id = match params.parent_feature_id {
                        Some(reference) => {
                            Some(resolve_feature(store, reference, "create_feature")?)
                        }
                        None => None,
                    };
                    if let Some(state) = params.state.as_deref() {
                        validate_enum(state, FEATURE_STATES, "state")?;
                    }
                    if let Some(priority) = params.priority.as_deref() {
                        validate_enum(priority, FEATURE_PRIORITIES, "priority")?;
                    }
                    let feature = store.create_feature(CreateFeature {
                        title: required(params.title, "title", "create_feature")?,
                        description: params.description,
                        parent_feature_id,
                        state: params.state,
                        priority: params.priority,
                        tags: params.tags.unwrap_or_else(|| json!([])),
                        brief: params.brief,
                        acceptance: params.acceptance.unwrap_or_else(|| json!([])),
                        owner: params.owner,
                        gates: params.gates.unwrap_or_else(|| json!([])),
                        indexes: params.indexes.unwrap_or_else(|| json!([])),
                    })?;
                    output(
                        format!("Created F{}", feature.display_id),
                        with_base(json!({"feature": feature})),
                    )
                }
                "create" => {
                    if let Some(state) = params.state.as_deref() {
                        validate_enum(state, TASK_STATES, "state")?;
                    }
                    let feature_id = match params.feature_id {
                        Some(reference) => Some(resolve_feature(store, reference, "create")?),
                        None => None,
                    };
                    let depends_on = match params.depends_on_task_id {
                        Some(reference) => vec![resolve_task(store, reference, "create")?],
                        None => Vec::new(),
                    };
                    let task = store.create_task(CreateTask {
                        title: required(params.title, "title", "create")?,
                        description: params.description,
                        state: params.state,
                        feature_id,
                        indexes: params.indexes,
                        depends_on,
                    })?;
                    output(
                        format!("Created #{}", task.display_id),
                        with_base(json!({"task": task})),
                    )
                }
                "batch" => {
                    let operations = required(params.operations, "operations", "batch")?;
                    if operations.is_empty() {
                        return Err(anyhow!("operations must not be empty for batch"));
                    }
                    for operation in &operations {
                        let state = match operation {
                            BatchOperation::Create { state, .. }
                            | BatchOperation::Update { state, .. } => state.as_deref(),
                        };
                        if let Some(state) = state {
                            validate_enum(state, TASK_STATES, "state")?;
                        }
                    }
                    let result = store.batch_execute(operations)?;
                    output(
                        format!(
                            "Batch: {} created, {} updated",
                            result.created, result.updated
                        ),
                        with_base(json!({"batch": result})),
                    )
                }
                "plan" => {
                    let tasks = required(params.tasks, "tasks", "plan")?;
                    if tasks.is_empty() {
                        return Err(anyhow!("tasks must not be empty for plan"));
                    }
                    for task in &tasks {
                        if let Some(state) = task.state.as_deref() {
                            validate_enum(state, TASK_STATES, "state")?;
                        }
                    }
                    let result = store.plan_import(tasks)?;
                    output(
                        format!(
                            "Plan: {} tasks, {} dependencies",
                            result.task_count, result.dependency_count
                        ),
                        with_base(json!({"plan": result})),
                    )
                }
                "claim" => {
                    if let Some(kind) = params.claim_kind.as_deref() {
                        validate_enum(kind, &["claim", "hold", "lock"], "claim_kind")?;
                    }
                    let task_id = resolve_task(
                        store,
                        required(params.task_id, "task_id", "claim")?,
                        "claim",
                    )?;
                    let scope_feature_id = match params.feature_id {
                        Some(reference) => Some(resolve_feature(store, reference, "claim")?),
                        None => None,
                    };
                    let result = store.claim_task(ClaimTaskInput {
                        task_id,
                        context: work_context,
                        claim_kind: params.claim_kind,
                        reason: params.reason,
                        lease_ms: params.lease_ms,
                        scope_feature_id,
                    })?;
                    output("Task claim", with_base(json!({"claim": result})))
                }
                "release" => {
                    let task_id = match params.task_id {
                        Some(reference) => Some(resolve_task(store, reference, "release")?),
                        None => None,
                    };
                    if task_id.is_none()
                        && params.claim_id.is_none()
                        && !params.release_all.unwrap_or(false)
                    {
                        return Err(anyhow!(
                            "task_id, claim_id, or release_all=true is required for release"
                        ));
                    }
                    let result = store.release_claim(ReleaseClaimInput {
                        context: work_context,
                        task_id,
                        claim_id: params.claim_id,
                        release_all: params.release_all.unwrap_or(false),
                        reason: params.reason,
                    })?;
                    output(
                        format!("Released {} claims", result.count),
                        with_base(json!({"release": result})),
                    )
                }
                "working_set" => {
                    let result = store.get_working_set(work_context)?;
                    output(
                        format!(
                            "Working set: {} claims, {} work units",
                            result.claims.len(),
                            result.work_units.len()
                        ),
                        with_base(json!({"working_set": result})),
                    )
                }
                "next_work_unit" => {
                    if let Some(kind) = params.claim_kind.as_deref() {
                        validate_enum(kind, &["claim", "hold", "lock"], "claim_kind")?;
                    }
                    let feature_id = resolve_feature(
                        store,
                        required(params.feature_id, "feature_id", "next_work_unit")?,
                        "next_work_unit",
                    )?;
                    let result = store.enqueue_next_work_unit(NextWorkUnitInput {
                        context: work_context,
                        claim_kind: params.claim_kind,
                        reason: params.reason,
                        lease_ms: params.lease_ms,
                        feature_id: Some(feature_id),
                        priority: params.work_priority,
                        set_active: params.set_active,
                    })?;
                    output(
                        "Next work unit",
                        with_base(json!({"next_work_unit": result})),
                    )
                }
                "update" | "set_state" => {
                    let task_id = resolve_task(
                        store,
                        required(params.task_id, "task_id", action.as_str())?,
                        action.as_str(),
                    )?;
                    if let Some(state) = params.state.as_deref() {
                        validate_enum(state, TASK_STATES, "state")?;
                    }
                    let feature_id = match params.feature_id {
                        Some(reference) => {
                            Some(Some(resolve_feature(store, reference, action.as_str())?))
                        }
                        None => None,
                    };
                    if action == "set_state" && params.state.is_none() {
                        return Err(anyhow!("state is required for set_state"));
                    }
                    let task = store.update_task(
                        &task_id,
                        UpdateTask {
                            title: params.title,
                            description: params.description.map(Some),
                            state: params.state,
                            feature_id,
                            indexes: params.indexes,
                        },
                    )?;
                    output(
                        format!("Updated #{}", task.display_id),
                        with_base(json!({"task": task})),
                    )
                }
                "add_dependency" => {
                    let task_id = resolve_task(
                        store,
                        required(params.task_id, "task_id", "add_dependency")?,
                        "add_dependency",
                    )?;
                    let dependency = resolve_task(
                        store,
                        required(
                            params.depends_on_task_id,
                            "depends_on_task_id",
                            "add_dependency",
                        )?,
                        "add_dependency",
                    )?;
                    let existing = store
                        .list_dependencies()?
                        .into_iter()
                        .filter(|dep| dep.task_id == task_id)
                        .map(|dep| dep.depends_on_id)
                        .chain(std::iter::once(dependency))
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>();
                    let dependencies = store.set_dependencies(&task_id, &existing)?;
                    output(
                        "Added task dependency",
                        with_base(json!({"dependencies": dependencies})),
                    )
                }
                "add_note" => {
                    let content = required(params.content, "content", "add_note")?;
                    match (params.task_id, params.feature_id) {
                        (Some(task_reference), None) => {
                            let task_id = resolve_task(store, task_reference, "add_note")?;
                            let note = store.append_task_note(
                                &task_id,
                                params.category.as_deref(),
                                &content,
                            )?;
                            output("Added task note", with_base(json!({"note": note})))
                        }
                        (None, Some(feature_reference)) => {
                            let feature_id = resolve_feature(store, feature_reference, "add_note")?;
                            let note = store.append_feature_note(
                                &feature_id,
                                params.category.as_deref(),
                                &content,
                            )?;
                            output("Added feature note", with_base(json!({"note": note})))
                        }
                        _ => Err(anyhow!(
                            "add_note requires exactly one of task_id or feature_id"
                        )),
                    }
                }
                "feature_list" => {
                    let features = store.list_features()?;
                    let total = features.len();
                    output(
                        format!("{total} features"),
                        with_base(json!({
                            "features": features.into_iter().take(limit).collect::<Vec<_>>(),
                            "total": total,
                            "truncated": total > limit,
                            "limit": limit,
                        })),
                    )
                }
                "feature_status" => {
                    let feature_id = resolve_feature(
                        store,
                        required(params.feature_id, "feature_id", "feature_status")?,
                        "feature_status",
                    )?;
                    let feature = store
                        .list_features()?
                        .into_iter()
                        .find(|f| f.id == feature_id)
                        .ok_or_else(|| anyhow!("feature disappeared while showing it"))?;
                    let tasks = store
                        .list_tasks(None)?
                        .into_iter()
                        .filter(|task| task.feature_id.as_deref() == Some(feature_id.as_str()))
                        .collect::<Vec<_>>();
                    let dependencies = store
                        .list_feature_dependencies()?
                        .into_iter()
                        .filter(|dep| dep.feature_id == feature_id)
                        .collect::<Vec<_>>();
                    let notes = store
                        .list_feature_notes()?
                        .into_iter()
                        .filter(|note| note.feature_id == feature_id)
                        .collect::<Vec<_>>();
                    output(
                        format!("F{} {}", feature.display_id, feature.title),
                        with_base(json!({
                            "feature": feature,
                            "tasks": tasks,
                            "dependencies": dependencies,
                            "notes": notes,
                        })),
                    )
                }
                "feature_update" => {
                    let feature_id = resolve_feature(
                        store,
                        required(params.feature_id, "feature_id", "feature_update")?,
                        "feature_update",
                    )?;
                    if let Some(state) = params.state.as_deref() {
                        validate_enum(state, FEATURE_STATES, "state")?;
                    }
                    if let Some(priority) = params.priority.as_deref() {
                        validate_enum(priority, FEATURE_PRIORITIES, "priority")?;
                    }
                    let parent_feature_id = match params.parent_feature_id {
                        Some(reference) => {
                            Some(Some(resolve_feature(store, reference, "feature_update")?))
                        }
                        None => None,
                    };
                    let feature = store.update_feature(
                        &feature_id,
                        UpdateFeature {
                            title: params.title,
                            description: params.description.map(Some),
                            parent_feature_id,
                            state: params.state,
                            priority: params.priority,
                            tags: params.tags,
                            brief: params.brief.map(Some),
                            acceptance: params.acceptance,
                            owner: params.owner.map(Some),
                            gates: params.gates,
                            indexes: params.indexes,
                        },
                    )?;
                    output(
                        format!("Updated F{}", feature.display_id),
                        with_base(json!({"feature": feature})),
                    )
                }
                "link" => {
                    let task_id =
                        resolve_task(store, required(params.task_id, "task_id", "link")?, "link")?;
                    let feature_id = resolve_feature(
                        store,
                        required(params.feature_id, "feature_id", "link")?,
                        "link",
                    )?;
                    store.link_task(&task_id, &feature_id)?;
                    let task = store
                        .list_tasks(None)?
                        .into_iter()
                        .find(|task| task.id == task_id)
                        .ok_or_else(|| anyhow!("task disappeared after link"))?;
                    output("Linked task to feature", with_base(json!({"task": task})))
                }
                "unlink" => {
                    let task_id = resolve_task(
                        store,
                        required(params.task_id, "task_id", "unlink")?,
                        "unlink",
                    )?;
                    store.unlink_task(&task_id)?;
                    let task = store
                        .list_tasks(None)?
                        .into_iter()
                        .find(|task| task.id == task_id)
                        .ok_or_else(|| anyhow!("task disappeared after unlink"))?;
                    output(
                        "Unlinked task from feature",
                        with_base(json!({"task": task})),
                    )
                }
                action => Err(anyhow!("unsupported tasker action: {action}")),
            }
        })
        .await
        .context("Pi Tasker blocking task panicked or was cancelled")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jcode_tool_core::ToolExecutionMode;
    use rusqlite::Connection;
    use std::path::Path;

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

    fn install_pi_schema(path: &Path) {
        let conn = Connection::open(path).expect("open temp pi db");
        conn.execute_batch(
            r#"
            CREATE TABLE task_lists (list_id TEXT NOT NULL, project_root TEXT NOT NULL, name TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY (list_id, project_root));
            CREATE TABLE tasks (id TEXT PRIMARY KEY, list_id TEXT NOT NULL, project_root TEXT NOT NULL, display_id INTEGER NOT NULL, title TEXT NOT NULL, description TEXT, state TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, indexes TEXT DEFAULT '[]', feature_id TEXT);
            CREATE TABLE task_dependencies (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, depends_on TEXT NOT NULL, list_id TEXT NOT NULL, project_root TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE task_notes (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, list_id TEXT NOT NULL, project_root TEXT NOT NULL, category TEXT, content TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE features (id TEXT PRIMARY KEY, list_id TEXT NOT NULL, project_root TEXT NOT NULL, display_id INTEGER NOT NULL, parent_feature_id TEXT, title TEXT NOT NULL, description TEXT, state TEXT NOT NULL DEFAULT 'open', priority TEXT DEFAULT 'medium', tags TEXT DEFAULT '[]', brief TEXT, acceptance TEXT DEFAULT '[]', owner TEXT, gates TEXT DEFAULT '[]', indexes TEXT DEFAULT '[]', depth INTEGER DEFAULT 0, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
            CREATE TABLE feature_dependencies (id TEXT PRIMARY KEY, feature_id TEXT NOT NULL, depends_on TEXT NOT NULL, list_id TEXT NOT NULL, project_root TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE feature_notes (id TEXT PRIMARY KEY, feature_id TEXT NOT NULL, list_id TEXT NOT NULL, project_root TEXT NOT NULL, category TEXT, content TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE tasker_session_instances (id TEXT PRIMARY KEY, list_id TEXT NOT NULL, project_root TEXT NOT NULL, agent_id TEXT NOT NULL, session_id TEXT NOT NULL, session_file TEXT, pid INTEGER NOT NULL, model TEXT, leaf_id_at_start TEXT, current_leaf_id TEXT, started_at INTEGER NOT NULL, last_seen_at INTEGER NOT NULL, ended_at INTEGER);
            CREATE TABLE task_claims (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, list_id TEXT NOT NULL, project_root TEXT NOT NULL, agent_id TEXT NOT NULL, session_id TEXT NOT NULL, session_file TEXT, session_instance_id TEXT NOT NULL, pid INTEGER NOT NULL, claim_kind TEXT NOT NULL, reason TEXT, claimed_at INTEGER NOT NULL, expires_at INTEGER, released_at INTEGER, release_reason TEXT, scope_feature_id TEXT);
            CREATE TABLE work_units (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, claim_id TEXT, list_id TEXT NOT NULL, project_root TEXT NOT NULL, agent_id TEXT NOT NULL, session_id TEXT NOT NULL, session_file TEXT, session_instance_id TEXT NOT NULL, status TEXT NOT NULL, priority INTEGER NOT NULL DEFAULT 0, note TEXT, created_at INTEGER NOT NULL, dispatched_at INTEGER, completed_at INTEGER, cancelled_at INTEGER, scope_feature_id TEXT);
            CREATE TABLE visual_artifacts (id TEXT PRIMARY KEY, list_id TEXT NOT NULL, project_root TEXT NOT NULL, task_id TEXT, feature_id TEXT, work_unit_id TEXT, stage TEXT, kind TEXT NOT NULL, title TEXT NOT NULL, summary TEXT NOT NULL, path TEXT NOT NULL, mime_type TEXT NOT NULL, metadata TEXT NOT NULL DEFAULT '{}', created_by TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
            "#,
        )
        .expect("install temp pi schema");
    }

    fn temp_tool() -> (tempfile::TempDir, TaskerTool) {
        let database_dir = tempfile::tempdir().expect("database tempdir");
        let path = database_dir.path().join("tasks.db");
        install_pi_schema(&path);
        (database_dir, TaskerTool::with_database_path(path))
    }

    #[test]
    fn schema_exposes_pi_actions_and_provider_safe_shape() {
        let definition = TaskerTool::new().to_definition();
        assert_eq!(definition.name, "tasker");
        assert!(definition.description.contains("Pi-compatible"));
        assert!(definition.description.contains("native-superset"));
        let schema = &definition.input_schema;
        assert_eq!(schema["additionalProperties"], false);
        assert!(!schema.to_string().contains("expected_revision"));
        assert!(!schema.to_string().contains("cancelled"));
        assert!(validate_enum("done", TASK_STATES, "state").is_ok());
        assert!(validate_enum("cancelled", TASK_STATES, "state").is_err());
        for (_name, property) in schema["properties"].as_object().expect("properties") {
            assert!(
                property.get("type").is_some(),
                "property lacks type: {property}"
            );
            assert!(
                property.get("default").is_none(),
                "property has default: {property}"
            );
        }
        let actions = schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum");
        for action in [
            "status",
            "ready",
            "show",
            "create_feature",
            "create",
            "add_dependency",
            "set_state",
            "list",
            "search",
            "update",
            "add_note",
            "feature_list",
            "feature_update",
            "feature_status",
            "link",
            "unlink",
            "batch",
            "plan",
            "claim",
            "release",
            "working_set",
            "next_work_unit",
        ] {
            assert!(actions.contains(&json!(action)), "missing {action}");
        }
    }

    #[tokio::test]
    async fn uses_temp_pi_schema_and_outputs_partition_metadata() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let (_database_dir, tool) = temp_tool();
        let ctx = context(workspace.path());

        let status = tool
            .execute(json!({"action": "status"}), ctx.clone())
            .await
            .expect("empty status")
            .metadata
            .expect("status metadata");
        assert_eq!(status["backend"], BACKEND);
        assert_eq!(status["initialized"], false);
        assert!(
            status["database_path"]
                .as_str()
                .expect("database_path")
                .contains("tasks.db")
        );
        assert!(
            status["list_id"]
                .as_str()
                .expect("list_id")
                .starts_with("list_")
        );
        assert_eq!(
            status["project_root"],
            workspace
                .path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(status["schema_fingerprint"].as_str().unwrap().len(), 40);
    }

    #[tokio::test]
    async fn round_trips_pi_feature_task_dependencies_notes_and_search() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let (_database_dir, tool) = temp_tool();
        let ctx = context(workspace.path());

        let feature = tool
            .execute(
                json!({"action": "create_feature", "title": "Pi Tasker", "priority": "high", "tags": ["pi"]}),
                ctx.clone(),
            )
            .await
            .expect("create feature")
            .metadata
            .expect("feature metadata");
        assert_eq!(feature["feature"]["displayId"], 1);

        let setup = tool
            .execute(
                json!({"action": "create", "title": "Setup", "state": "done"}),
                ctx.clone(),
            )
            .await
            .expect("create setup")
            .metadata
            .expect("setup metadata");
        assert_eq!(setup["task"]["displayId"], 1);

        let build = tool
            .execute(
                json!({"action": "create", "feature_id": "#F1", "title": "Build needle", "depends_on_task_id": "#1"}),
                ctx.clone(),
            )
            .await
            .expect("create build")
            .metadata
            .expect("build metadata");
        assert_eq!(build["task"]["featureId"], feature["feature"]["id"]);

        let ready = tool
            .execute(json!({"action": "ready"}), ctx.clone())
            .await
            .expect("ready")
            .metadata
            .expect("ready metadata");
        assert_eq!(ready["ready"].as_array().unwrap().len(), 1);

        let note = tool
            .execute(
                json!({"action": "add_note", "task_id": "#2", "category": "context", "content": "note body"}),
                ctx.clone(),
            )
            .await
            .expect("add note")
            .metadata
            .expect("note metadata");
        assert_eq!(note["note"]["content"], "note body");

        let search = tool
            .execute(json!({"action": "search", "query": "needle"}), ctx.clone())
            .await
            .expect("search")
            .metadata
            .expect("search metadata");
        assert_eq!(search["tasks"].as_array().unwrap().len(), 1);

        let updated = tool
            .execute(
                json!({"action": "update", "task_id": "#2", "state": "in_progress", "description": "working"}),
                ctx.clone(),
            )
            .await
            .expect("update")
            .metadata
            .expect("update metadata");
        assert_eq!(updated["task"]["state"], "in_progress");

        let feature_status = tool
            .execute(
                json!({"action": "feature_status", "feature_id": "F1"}),
                ctx.clone(),
            )
            .await
            .expect("feature status")
            .metadata
            .expect("feature status metadata");
        assert_eq!(feature_status["tasks"].as_array().unwrap().len(), 1);

        let unlinked = tool
            .execute(json!({"action": "unlink", "task_id": "#2"}), ctx)
            .await
            .expect("unlink")
            .metadata
            .expect("unlink metadata");
        assert_eq!(unlinked["task"]["featureId"], Value::Null);
    }

    #[tokio::test]
    async fn imports_atomic_plans_and_batches_through_the_public_tool() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let (_database_dir, tool) = temp_tool();
        let ctx = context(workspace.path());

        let plan = tool
            .execute(
                json!({
                    "action": "plan",
                    "tasks": [
                        {"key": "ground", "title": "Ground source", "state": "done"},
                        {"key": "build", "title": "Build bridge", "after": ["ground"], "notes": [{"category": "context", "content": "atomic plan"}]}
                    ]
                }),
                ctx.clone(),
            )
            .await
            .expect("import plan")
            .metadata
            .expect("plan metadata");
        assert_eq!(plan["plan"]["taskCount"], 2);
        assert_eq!(plan["plan"]["dependencyCount"], 1);

        let batch = tool
            .execute(
                json!({
                    "action": "batch",
                    "operations": [
                        {"op": "create", "key": "verify", "title": "Verify bridge", "dependsOn": ["#2"]},
                        {"op": "update", "taskId": "verify", "state": "done"}
                    ]
                }),
                ctx.clone(),
            )
            .await
            .expect("execute batch")
            .metadata
            .expect("batch metadata");
        assert_eq!(batch["batch"]["created"], 1);
        assert_eq!(batch["batch"]["updated"], 1);
        assert_eq!(batch["batch"]["operations"][1]["task"]["state"], "done");

        let before = tool
            .execute(json!({"action": "status"}), ctx.clone())
            .await
            .expect("status before invalid plan")
            .metadata
            .expect("status metadata");
        let invalid = tool
            .execute(
                json!({"action": "plan", "tasks": [{"key": "bad", "title": "Must roll back", "after": ["missing"]}]}),
                ctx.clone(),
            )
            .await;
        assert!(invalid.is_err());
        let invalid_state = tool
            .execute(
                json!({"action": "batch", "operations": [{"op": "create", "title": "Nope", "state": "cancelled"}]}),
                ctx.clone(),
            )
            .await;
        assert!(invalid_state.is_err());
        let after = tool
            .execute(json!({"action": "status"}), ctx)
            .await
            .expect("status after invalid plan")
            .metadata
            .expect("status metadata");
        assert_eq!(before["counts"]["tasks"], after["counts"]["tasks"]);
        assert_eq!(
            before["counts"]["dependencies"],
            after["counts"]["dependencies"]
        );
    }

    #[tokio::test]
    async fn coordinates_private_claims_and_work_units_through_the_public_tool() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let (_database_dir, tool) = temp_tool();
        let ctx = context(workspace.path());

        tool.execute(
            json!({"action": "create_feature", "title": "Coordinator scope"}),
            ctx.clone(),
        )
        .await
        .expect("create feature");
        tool.execute(
            json!({"action": "create", "feature_id": "#F1", "title": "First ready"}),
            ctx.clone(),
        )
        .await
        .expect("create first task");
        tool.execute(
            json!({"action": "create", "feature_id": "#F1", "title": "Second ready"}),
            ctx.clone(),
        )
        .await
        .expect("create second task");

        let next = tool
            .execute(
                json!({
                    "action": "next_work_unit",
                    "feature_id": "#F1",
                    "claim_kind": "lock",
                    "reason": "public integration test"
                }),
                ctx.clone(),
            )
            .await
            .expect("next work unit")
            .metadata
            .expect("next metadata");
        assert_eq!(next["next_work_unit"]["ok"], true);
        assert_eq!(next["next_work_unit"]["task"]["displayId"], 1);
        assert_eq!(next["next_work_unit"]["task"]["state"], "in_progress");

        let claimed = tool
            .execute(
                json!({"action": "claim", "task_id": "#2", "claim_kind": "claim"}),
                ctx.clone(),
            )
            .await
            .expect("claim second task")
            .metadata
            .expect("claim metadata");
        assert_eq!(claimed["claim"]["ok"], true);

        let working = tool
            .execute(json!({"action": "working_set"}), ctx.clone())
            .await
            .expect("working set")
            .metadata
            .expect("working set metadata");
        assert_eq!(
            working["working_set"]["claims"].as_array().unwrap().len(),
            2
        );
        assert_eq!(
            working["working_set"]["workUnits"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let mut other_ctx = ctx.clone();
        other_ctx.session_id = "session-other".into();
        let other = tool
            .execute(json!({"action": "working_set"}), other_ctx)
            .await
            .expect("other working set")
            .metadata
            .expect("other metadata");
        assert!(
            other["working_set"]["claims"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            other["working_set"]["workUnits"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let released = tool
            .execute(
                json!({"action": "release", "release_all": true, "reason": "test complete"}),
                ctx.clone(),
            )
            .await
            .expect("release claims")
            .metadata
            .expect("release metadata");
        assert_eq!(released["release"]["count"], 2);

        let empty = tool
            .execute(json!({"action": "working_set"}), ctx)
            .await
            .expect("empty working set")
            .metadata
            .expect("empty metadata");
        assert!(
            empty["working_set"]["claims"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            empty["working_set"]["workUnits"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
