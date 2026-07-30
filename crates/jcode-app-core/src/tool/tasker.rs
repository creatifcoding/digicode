use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use jcode_tasker_pi::{
    BatchOperation, ClaimTaskInput, CreateFeature, CreateTask, FeatureGateCheckMode,
    FeatureGateCheckResult, FeaturePlanFeature, FeaturePlanInput, GateResolver, NextWorkUnitInput,
    NoteInput, PiTaskerStore, PlanTask, ProjectPartition, ReleaseClaimInput, ResolveFeatureGate,
    Task, UpdateFeature, UpdateTask, VisualArtifactCreateInput, VisualArtifactQueryInput,
    WorkContextInput,
};
use serde::Deserialize;
use serde_json::{Value, json};
use wait_timeout::ChildExt;

use super::{Tool, ToolContext, ToolOutput};

const BACKEND: &str = "jcode-tasker-pi";
const TASK_STATES: &[&str] = &["todo", "in_progress", "blocked", "done"];
const FEATURE_STATES: &[&str] = &["open", "active", "closed", "archived"];
const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;
const FEATURE_PRIORITIES: &[&str] = &["low", "medium", "high", "critical"];
const EDIN_STAGES: &[&str] = &[
    "experiment",
    "design",
    "implement",
    "validate",
    "negotiate",
    "release",
];
const VISUAL_ARTIFACT_KINDS: &[&str] = &[
    "stage-report",
    "visual-plan",
    "diff-review",
    "evidence-pack",
    "project-recap",
    "design-deck",
    "architecture-diagram",
    "data-table",
    "custom",
];
const DEFAULT_COMMAND_TIMEOUT_MS: i64 = 120_000;
const DEFAULT_SCRIPT_TIMEOUT_MS: i64 = 120_000;
const DEFAULT_AGENT_TIMEOUT_MS: i64 = 300_000;

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
    depends_on: Vec<String>,
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
    summary: Option<String>,
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
    index_add: Option<Value>,
    #[serde(default)]
    index_remove: Option<Vec<String>>,
    #[serde(default)]
    index_set: Option<Value>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    operations: Option<Vec<BatchOperation>>,
    #[serde(default)]
    notes: Vec<NoteInput>,
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
    #[serde(default)]
    active: Option<bool>,
    #[serde(default)]
    clear_dependencies: bool,
    #[serde(default)]
    gate_index: Option<usize>,
    #[serde(default)]
    gate_status: Option<String>,
    #[serde(default)]
    gate_action: Option<String>,
    #[serde(default)]
    resolved_by: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    feature: Option<FeaturePlanFeature>,
    #[serde(default)]
    work_unit_id: Option<String>,
    #[serde(default)]
    stage: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    metadata: Option<Value>,
    #[serde(default)]
    created_by: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    depth: Option<usize>,
    #[serde(default)]
    max_depth: Option<usize>,
    #[serde(default)]
    max_paths: Option<usize>,
    #[serde(default)]
    include_tasks: Option<bool>,
    #[serde(default)]
    include_done: Option<bool>,
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

fn last_lines(text: &str, n: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

struct ResolverCommandSpec {
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
    timeout_ms: i64,
    env: Option<std::collections::BTreeMap<String, String>>,
    fail_on_tool_error_json: bool,
}

fn jcode_resolver_bin() -> String {
    std::env::var("JCODE_BIN").unwrap_or_else(|_| "jcode".to_string())
}

fn execute_command_with_timeout(spec: ResolverCommandSpec) -> Result<FeatureGateCheckResult> {
    let start = Instant::now();
    let timeout = Duration::from_millis(spec.timeout_ms.max(1) as u64);
    let stdout_file = tempfile::NamedTempFile::new().context("create resolver stdout capture")?;
    let stderr_file = tempfile::NamedTempFile::new().context("create resolver stderr capture")?;
    let stdout_path = stdout_file.path().to_path_buf();
    let stderr_path = stderr_file.path().to_path_buf();
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdout(Stdio::from(
            stdout_file
                .reopen()
                .context("reopen resolver stdout capture")?,
        ))
        .stderr(Stdio::from(
            stderr_file
                .reopen()
                .context("reopen resolver stderr capture")?,
        ));
    if let Some(env) = &spec.env {
        command.envs(env);
    }
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command.spawn().with_context(|| {
        format!(
            "spawn gate resolver command {} in {}",
            spec.program,
            spec.cwd.display()
        )
    })?;

    let (timed_out, status) = match child
        .wait_timeout(timeout)
        .context("wait for gate resolver command")?
    {
        Some(status) => (false, status),
        None => {
            #[cfg(unix)]
            {
                // The resolver is its own process-group leader. Killing the group also
                // terminates shell descendants that may still hold capture files open.
                // SAFETY: `child.id()` is the live group leader created immediately above,
                // and a negative pid intentionally targets only that process group.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
            }
            #[cfg(not(unix))]
            {
                let _ = child.kill();
            }
            (
                true,
                child
                    .wait()
                    .context("reap timed-out gate resolver command")?,
            )
        }
    };
    let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
    let mut full_log = [stdout.trim(), stderr.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n---stderr---\n");
    if timed_out {
        if !full_log.is_empty() {
            full_log.push('\n');
        }
        full_log.push_str(&format!("timed out after {} ms", spec.timeout_ms));
    }
    let exit_code = if timed_out {
        124
    } else {
        status.code().unwrap_or(1)
    };
    let tool_error = spec.fail_on_tool_error_json && full_log.contains("\"isError\":true");
    Ok(FeatureGateCheckResult {
        status: if !timed_out && !tool_error && status.success() {
            "passed".into()
        } else {
            "failed".into()
        },
        note: last_lines(&full_log, 50),
        full_log,
        exit_code: exit_code.into(),
        duration_ms: start.elapsed().as_millis() as i64,
    })
}

fn execute_gate_resolver(
    resolver: &GateResolver,
    project_root: &std::path::Path,
) -> Result<FeatureGateCheckResult> {
    let spec = match resolver {
        GateResolver::Command {
            run,
            cwd,
            timeout,
            env,
        } => ResolverCommandSpec {
            program: "bash".into(),
            args: vec!["-lc".into(), run.clone()],
            cwd: cwd
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| project_root.to_path_buf()),
            timeout_ms: timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS),
            env: env.clone(),
            fail_on_tool_error_json: false,
        },
        GateResolver::Script { path, timeout } => {
            let script = if std::path::Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                project_root.join(path)
            };
            ResolverCommandSpec {
                program: "bash".into(),
                args: vec![script.to_string_lossy().into_owned()],
                cwd: project_root.to_path_buf(),
                timeout_ms: timeout.unwrap_or(DEFAULT_SCRIPT_TIMEOUT_MS),
                env: None,
                fail_on_tool_error_json: false,
            }
        }
        GateResolver::Tool { tool, args } => {
            let prompt = match args {
                Some(args) => format!(
                    "Call exactly one tool named {tool:?} with these JSON arguments: {}. Do not call any other tool. Return only whether that single tool call succeeded.",
                    serde_json::to_string(args)?
                ),
                None => format!(
                    "Call exactly one tool named {tool:?} with empty JSON arguments. Do not call any other tool. Return only whether that single tool call succeeded."
                ),
            };
            ResolverCommandSpec {
                program: jcode_resolver_bin(),
                args: vec![
                    "--disable-base-tools".into(),
                    "--tools".into(),
                    tool.clone(),
                    "run".into(),
                    "--json".into(),
                    prompt,
                ],
                cwd: project_root.to_path_buf(),
                timeout_ms: DEFAULT_COMMAND_TIMEOUT_MS,
                env: None,
                fail_on_tool_error_json: true,
            }
        }
        GateResolver::Agent { task, model, .. } => {
            let mut args = vec!["run".into(), "--json".into()];
            if let Some(model) = model {
                args.splice(0..0, ["--model".into(), model.clone()]);
            }
            args.push(task.clone());
            ResolverCommandSpec {
                program: jcode_resolver_bin(),
                args,
                cwd: project_root.to_path_buf(),
                timeout_ms: DEFAULT_AGENT_TIMEOUT_MS,
                env: None,
                fail_on_tool_error_json: false,
            }
        }
    };
    execute_command_with_timeout(spec)
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
                        "list", "search", "update", "add_note", "task_index", "feature_list", "feature_update", "feature_status",
                        "link", "unlink", "batch", "plan", "feature_plan", "claim", "release", "working_set", "next_work_unit", "feature_gate",
                        "task_artifact_create", "task_artifacts", "task_stage_report",
                        "task_graph", "task_structure", "topology_summary", "topology_anomalies", "topology_paths", "topology_frontier",
                        "feature_children", "task_neighbors", "feature_tree"
                    ]
                },
                "title": {"type": "string"},
                "description": {"type": "string"},
                "feature_id": {"type": "string", "description": "Feature reference: feat_<id>, F<number>, or #F<number>."},
                "parent_feature_id": {"type": "string", "description": "Optional parent feature reference."},
                "task_id": {"type": "string", "description": "Task reference: task_<id>, <number>, or #<number>."},
                "depends_on_task_id": {"type": "string", "description": "Prerequisite task reference."},
                "depends_on": {"type": "array", "items": {"type": "string"}, "description": "Prerequisite task references. Pi-compatible plural form for create/update."},
                "state": {"type": "string", "enum": ["todo", "in_progress", "blocked", "done", "open", "active", "closed", "archived"]},
                "priority": {"type": "string", "enum": ["low", "normal", "medium", "high", "critical"]},
                "rank": {"type": "integer", "description": "Accepted for compatibility; Pi stores order as display_id."},
                "query": {"type": "string"},
                "category": {"type": "string"},
                "content": {"type": "string"},
                "summary": {"type": "string"},
                "tags": {"type": ["array", "object"]},
                "brief": {"type": "string"},
                "acceptance": {"type": ["array", "object"]},
                "owner": {"type": "string"},
                "gates": {"type": ["array", "object"]},
                "indexes": {"type": ["array", "object"]},
                "index_add": {"type": ["array", "object"], "description": "task_index add entries. Appended after remove when index_set is omitted."},
                "index_remove": {"type": "array", "items": {"type": "string"}, "description": "task_index remove paths. Removes entries whose path exactly matches."},
                "index_set": {"type": ["array", "object"], "description": "task_index replacement entries. When present, add/remove are ignored to match Pi set semantics."},
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
                "notes": {"type": "array", "items": {"type": "object", "required": ["content"], "additionalProperties": false, "properties": {"content": {"type": "string"}, "category": {"type": "string"}}}},
                "claim_kind": {"type": "string", "enum": ["claim", "hold", "lock"]},
                "reason": {"type": "string"},
                "lease_ms": {"type": "integer", "minimum": 1},
                "release_all": {"type": "boolean"},
                "work_priority": {"type": "integer", "description": "Queue priority for next_work_unit."},
                "set_active": {"type": "boolean", "description": "Whether next_work_unit should activate a todo task. Defaults to true in Pi-compatible behavior."},
                "active": {"type": "boolean", "description": "Accepted for Pi update compatibility. Pi activeTaskId is process-local snapshot state and is not persisted in SQL."},
                "clear_dependencies": {"type": "boolean", "description": "Clear all dependencies on update before applying replacement dependencies."},
                "gate_index": {"type": "integer", "minimum": 0, "description": "Zero-based feature gate index."},
                "gate_action": {"type": "string", "enum": ["resolve", "check", "check-all"], "description": "Feature gate operation. Defaults to resolve."},
                "gate_status": {"type": "string", "enum": ["pending", "passed", "failed"]},
                "resolved_by": {"type": "string"},
                "note": {"type": "string", "description": "Gate resolution note. Automated resolver evidence is retained separately by the store adapter."},
                "work_unit_id": {"type": "string"},
                "stage": {"type": "string", "enum": ["experiment", "design", "implement", "validate", "negotiate", "release"]},
                "kind": {"type": "string", "enum": ["stage-report", "visual-plan", "diff-review", "evidence-pack", "project-recap", "design-deck", "architecture-diagram", "data-table", "custom"]},
                "path": {"type": "string"},
                "mime_type": {"type": "string"},
                "metadata": {"type": "object"},
                "created_by": {"type": "string"},
                "from": {"type": "string"},
                "to": {"type": "string"},
                "domain": {"type": "string", "enum": ["task", "feature"]},
                "mode": {"type": "string", "enum": ["shortest", "all_up_to_depth"]},
                "direction": {"type": "string", "enum": ["upstream", "downstream", "both"]},
                "depth": {"type": "integer", "minimum": 0, "maximum": 64},
                "max_depth": {"type": "integer", "minimum": 1, "maximum": 64},
                "max_paths": {"type": "integer", "minimum": 1, "maximum": 100},
                "include_tasks": {"type": "boolean"},
                "include_done": {"type": "boolean"},
                "feature": {
                    "type": "object",
                    "description": "Root of an atomic Pi-compatible feature plan.",
                    "required": ["key", "title"],
                    "additionalProperties": false,
                    "properties": {
                        "key": {"type": "string"},
                        "title": {"type": "string"},
                        "description": {"type": "string"},
                        "priority": {"type": "string", "enum": ["low", "medium", "high", "critical"]},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "brief": {"type": "string"},
                        "acceptance": {"type": "array", "items": {"type": "object", "required": ["criterion"], "additionalProperties": false, "properties": {"criterion": {"type": "string"}, "met": {"type": "boolean"}}}},
                        "owner": {"type": "string"},
                        "gates": {"type": "array", "items": {"type": "object"}},
                        "indexes": {"type": "array", "items": {}},
                        "notes": {"type": "array", "items": {"type": "object", "required": ["content"], "additionalProperties": false, "properties": {"content": {"type": "string"}, "category": {"type": "string"}}}},
                        "children": {"type": "array", "items": {"$ref": "#/$defs/featurePlanChild"}},
                        "tasks": {"type": "array", "items": {"$ref": "#/$defs/featurePlanTask"}}
                    }
                },
                "limit": {"type": "integer", "minimum": 1, "maximum": 500, "description": "Maximum records returned by bounded list, search, and status projections."}
            },
            "$defs": {
                "featurePlanTask": {
                    "type": "object",
                    "required": ["key", "title"],
                    "additionalProperties": false,
                    "properties": {
                        "key": {"type": "string"}, "title": {"type": "string"}, "description": {"type": "string"},
                        "state": {"type": "string", "enum": ["todo", "in_progress", "blocked", "done"]},
                        "after": {"type": "array", "items": {"type": "string"}},
                        "notes": {"type": "array", "items": {"type": "object", "required": ["content"], "additionalProperties": false, "properties": {"content": {"type": "string"}, "category": {"type": "string"}}}},
                        "indexes": {"type": ["array", "object"]}
                    }
                },
                "featurePlanChild": {
                    "type": "object",
                    "required": ["key", "title"],
                    "additionalProperties": false,
                    "properties": {
                        "key": {"type": "string"}, "title": {"type": "string"}, "description": {"type": "string"},
                        "priority": {"type": "string", "enum": ["low", "medium", "high", "critical"]},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "gates": {"type": "array", "items": {"type": "object"}},
                        "children": {"type": "array", "items": {"$ref": "#/$defs/featurePlanChild"}},
                        "tasks": {"type": "array", "items": {"$ref": "#/$defs/featurePlanTask"}}
                    }
                }
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
                "task_graph" => output(
                    "Task graph projection",
                    with_base(json!({"projection": store.task_graph_projection(limit)?})),
                ),
                "task_structure" => output(
                    "Task structure projection",
                    with_base(json!({"projection": store.task_structure_projection(limit)?})),
                ),
                "topology_summary" => output(
                    "Topology summary",
                    with_base(json!({"projection": store.topology_summary_projection(limit)?})),
                ),
                "topology_anomalies" => output(
                    "Topology anomalies",
                    with_base(json!({"projection": store.topology_anomalies_projection(limit)?})),
                ),
                "topology_paths" => {
                    let domain = params.domain.unwrap_or_else(|| "task".into());
                    validate_enum(&domain, &["task", "feature"], "domain")?;
                    let mode = params.mode.unwrap_or_else(|| "shortest".into());
                    validate_enum(&mode, &["shortest", "all_up_to_depth"], "mode")?;
                    let from = required(params.from, "from", "topology_paths")?;
                    let to = required(params.to, "to", "topology_paths")?;
                    output(
                        "Topology paths",
                        with_base(json!({"projection": store.topology_paths_projection(
                            &from,
                            &to,
                            &domain,
                            &mode,
                            params.max_depth.unwrap_or(8),
                            params.max_paths.unwrap_or(5),
                        )?})),
                    )
                }
                "topology_frontier" => output(
                    "Topology frontier",
                    with_base(json!({"projection": store.topology_frontier_projection(limit)?})),
                ),
                "feature_children" => output(
                    "Feature children",
                    with_base(json!({"projection": store.feature_children_projection(
                        &required(params.feature_id, "feature_id", "feature_children")?,
                        params.depth.unwrap_or(1),
                        limit,
                        params.include_tasks.unwrap_or(false),
                    )?})),
                ),
                "task_neighbors" => {
                    let direction = params.direction.unwrap_or_else(|| "both".into());
                    validate_enum(&direction, &["upstream", "downstream", "both"], "direction")?;
                    let task_id = required(params.task_id, "task_id", "task_neighbors")?;
                    output(
                        "Task neighbors",
                        with_base(json!({"projection": store.task_neighbors_projection(
                            &task_id,
                            &direction,
                            params.depth.unwrap_or(1),
                            limit,
                            params.include_done.unwrap_or(false),
                        )?})),
                    )
                }
                "feature_tree" => output(
                    "Feature tree",
                    with_base(json!({"projection": store.feature_tree_projection(
                        params.depth.unwrap_or(2),
                        limit,
                        params.include_tasks.unwrap_or(false),
                    )?})),
                ),
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
                    let mut depends_on = match params.depends_on_task_id {
                        Some(reference) => vec![resolve_task(store, reference, "create")?],
                        None => Vec::new(),
                    };
                    for reference in params.depends_on {
                        depends_on.push(resolve_task(store, reference, "create")?);
                    }
                    let task = store.create_task(CreateTask {
                        title: required(params.title, "title", "create")?,
                        description: params.description,
                        state: params.state,
                        feature_id,
                        indexes: params.indexes,
                        depends_on,
                        notes: params.notes,
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
                "feature_plan" => {
                    let feature = required(params.feature, "feature", "feature_plan")?;
                    let result = store.feature_plan_import(FeaturePlanInput { feature })?;
                    output(
                        format!(
                            "Feature plan: {} features, {} tasks, {} dependencies",
                            result.feature_count, result.task_count, result.dependency_count
                        ),
                        with_base(json!({"feature_plan": result})),
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
                    let feature_id = match params.feature_id {
                        Some(reference) => Some(resolve_feature(store, reference, "next_work_unit")?),
                        None => None,
                    };
                    let result = store.enqueue_next_work_unit(NextWorkUnitInput {
                        context: work_context,
                        claim_kind: params.claim_kind,
                        reason: params.reason,
                        lease_ms: params.lease_ms,
                        feature_id,
                        priority: params.work_priority,
                        set_active: params.set_active,
                    })?;
                    output(
                        "Next work unit",
                        with_base(json!({"next_work_unit": result})),
                    )
                }
                "feature_gate" => {
                    let feature_id = resolve_feature(
                        store,
                        required(params.feature_id, "feature_id", "feature_gate")?,
                        "feature_gate",
                    )?;
                    let gate_action = params.gate_action.as_deref().unwrap_or("resolve");
                    validate_enum(gate_action, &["resolve", "check", "check-all"], "gate_action")?;
                    if gate_action == "check-all" {
                        let project_root = PathBuf::from(store.partition().project_root.clone());
                        let indexes = store.pending_executable_gate_indexes(&feature_id)?;
                        let mut results = Vec::new();
                        for index in indexes {
                            let gate = store.feature_gate(&feature_id, index)?;
                            let resolver = gate.resolver.as_ref().ok_or_else(|| anyhow!("gate {index} has no resolver"))?;
                            let result = execute_gate_resolver(resolver, &project_root)?;
                            let failed = result.status == "failed";
                            results.push((index, result));
                            if failed {
                                break;
                            }
                        }
                        let applied = store.apply_feature_gate_checks(
                            &feature_id,
                            results,
                            FeatureGateCheckMode::FailFast,
                        )?;
                        return output(
                            format!("Checked {} feature gates", applied.len()),
                            with_base(json!({"feature_id": feature_id, "applied": applied})),
                        );
                    }
                    let gate_index = required(params.gate_index, "gate_index", "feature_gate")?;
                    if gate_action == "check" {
                        let project_root = PathBuf::from(store.partition().project_root.clone());
                        let gate = store.feature_gate(&feature_id, gate_index)?;
                        let resolver = gate.resolver.as_ref().ok_or_else(|| anyhow!("gate {gate_index} is manual and cannot be checked automatically"))?;
                        if gate.status != "pending" {
                            return Err(anyhow!("gate {gate_index} already {}", gate.status));
                        }
                        let result = execute_gate_resolver(resolver, &project_root)?;
                        let applied = store.apply_feature_gate_check(&feature_id, gate_index, result)?;
                        return output(
                            format!("Checked feature gate {gate_index}"),
                            with_base(json!({"feature_id": feature_id, "gate_index": gate_index, "applied": applied})),
                        );
                    }
                    let Some(status) = params.gate_status else {
                        let gate = store.feature_gate(&feature_id, gate_index)?;
                        return output(
                            format!("Feature gate {gate_index}"),
                            with_base(json!({
                                "feature_id": feature_id,
                                "gate_index": gate_index,
                                "gate": gate,
                                "readOnly": true,
                            })),
                        );
                    };
                    validate_enum(&status, &["pending", "passed", "failed"], "gate_status")?;
                    let gates = store.resolve_feature_gate(
                        &feature_id,
                        gate_index,
                        ResolveFeatureGate {
                            status,
                            resolved_by: params.resolved_by,
                            note: params.note,
                        },
                    )?;
                    output(
                        format!("Resolved feature gate {gate_index}"),
                        with_base(json!({
                            "feature_id": feature_id,
                            "gate_index": gate_index,
                            "gate": gates.get(gate_index),
                            "gates": gates,
                        })),
                    )
                }
                "task_artifact_create" => {
                    let task_id = match params.task_id {
                        Some(reference) => Some(resolve_task(store, reference, "task_artifact_create")?),
                        None => None,
                    };
                    let feature_id = match params.feature_id {
                        Some(reference) => {
                            Some(resolve_feature(store, reference, "task_artifact_create")?)
                        }
                        None => None,
                    };
                    let kind = required(params.kind, "kind", "task_artifact_create")?;
                    validate_enum(&kind, VISUAL_ARTIFACT_KINDS, "kind")?;
                    if let Some(stage) = params.stage.as_deref() {
                        validate_enum(stage, EDIN_STAGES, "stage")?;
                    }
                    let artifact = store.create_visual_artifact(VisualArtifactCreateInput {
                        task_id,
                        feature_id,
                        work_unit_id: params.work_unit_id,
                        stage: params.stage,
                        kind,
                        title: required(params.title, "title", "task_artifact_create")?,
                        summary: required(params.summary.or(params.content), "summary", "task_artifact_create")?,
                        path: required(params.path, "path", "task_artifact_create")?,
                        mime_type: params.mime_type,
                        metadata: params.metadata,
                        created_by: params.created_by.unwrap_or(work_context.agent_id),
                    })?;
                    output(
                        format!("✓ Artifact {} · {}", artifact.kind, artifact.title),
                        with_base(json!({"artifact": artifact, "workingSetScope": "self"})),
                    )
                }
                "task_artifacts" => {
                    let task_id = match params.task_id {
                        Some(reference) => Some(resolve_task(store, reference, "task_artifacts")?),
                        None => None,
                    };
                    let feature_id = match params.feature_id {
                        Some(reference) => Some(resolve_feature(store, reference, "task_artifacts")?),
                        None => None,
                    };
                    if let Some(stage) = params.stage.as_deref() {
                        validate_enum(stage, EDIN_STAGES, "stage")?;
                    }
                    if let Some(kind) = params.kind.as_deref() {
                        validate_enum(kind, VISUAL_ARTIFACT_KINDS, "kind")?;
                    }
                    let artifacts = store.list_visual_artifacts(VisualArtifactQueryInput {
                        task_id,
                        feature_id,
                        work_unit_id: params.work_unit_id,
                        stage: params.stage,
                        kind: params.kind,
                        limit: Some(limit),
                    })?;
                    let lines = artifacts
                        .iter()
                        .map(|artifact| {
                            format!(
                                "{} · {} · {} · {}",
                                artifact.stage.as_deref().unwrap_or("no-stage"),
                                artifact.kind,
                                artifact.title,
                                artifact.path
                            )
                        })
                        .collect::<Vec<_>>();
                    let count = artifacts.len();
                    output(
                        if lines.is_empty() {
                            "No visual artifacts found".to_string()
                        } else {
                            lines.join("\n")
                        },
                        with_base(json!({"artifacts": artifacts, "count": count})),
                    )
                }
                "task_stage_report" => {
                    let stage = required(params.stage, "stage", "task_stage_report")?;
                    validate_enum(&stage, EDIN_STAGES, "stage")?;
                    let task_id = match params.task_id {
                        Some(reference) => Some(resolve_task(store, reference, "task_stage_report")?),
                        None => None,
                    };
                    let feature_id = match params.feature_id {
                        Some(reference) => Some(resolve_feature(store, reference, "task_stage_report")?),
                        None => None,
                    };
                    let work_unit_id = params.work_unit_id.clone();
                    let report_input = json!({
                        "stage": stage.clone(),
                        "generatedAt": chrono::Utc::now().timestamp_millis(),
                        "task": task_id.as_ref().and_then(|id| store.list_tasks(None).ok()?.into_iter().find(|task| &task.id == id)),
                        "feature": feature_id.as_ref().and_then(|id| store.list_features().ok()?.into_iter().find(|feature| &feature.id == id)),
                        "workUnit": Value::Null,
                        "dependencies": task_id.as_ref().map(|id| store.list_dependencies().unwrap_or_default().into_iter().filter(|dep| &dep.task_id == id).collect::<Vec<_>>()).unwrap_or_default(),
                        "dependents": task_id.as_ref().map(|id| store.list_dependencies().unwrap_or_default().into_iter().filter(|dep| &dep.depends_on_id == id).collect::<Vec<_>>()).unwrap_or_default(),
                        "taskNotes": task_id.as_ref().map(|id| store.list_task_notes().unwrap_or_default().into_iter().filter(|note| &note.task_id == id).collect::<Vec<_>>()).unwrap_or_default(),
                        "featureNotes": feature_id.as_ref().map(|id| store.list_feature_notes().unwrap_or_default().into_iter().filter(|note| &note.feature_id == id).collect::<Vec<_>>()).unwrap_or_default(),
                        "artifacts": store.list_visual_artifacts(VisualArtifactQueryInput { task_id: task_id.clone(), feature_id: feature_id.clone(), work_unit_id: work_unit_id.clone(), stage: Some(stage.clone()), kind: None, limit: Some(50) })?,
                        "workingSet": store.get_working_set(work_context.clone())?,
                    });
                    let report_task_id = task_id.clone();
                    let report_feature_id = feature_id.clone();
                    let report_work_unit_id = work_unit_id.clone();
                    let mut metadata = params.metadata.unwrap_or_else(|| Value::Object(Default::default()));
                    if let Value::Object(object) = &mut metadata {
                        object.insert("taskerReportInput".into(), json!({
                            "generatedAt": report_input.get("generatedAt").cloned().unwrap_or(Value::Null),
                            "taskId": report_task_id,
                            "featureId": report_feature_id,
                            "workUnitId": report_work_unit_id,
                            "dependencyCount": report_input["dependencies"].as_array().map_or(0, Vec::len),
                            "dependentCount": report_input["dependents"].as_array().map_or(0, Vec::len),
                            "taskNoteCount": report_input["taskNotes"].as_array().map_or(0, Vec::len),
                            "featureNoteCount": report_input["featureNotes"].as_array().map_or(0, Vec::len),
                            "priorArtifactCount": report_input["artifacts"].as_array().map_or(0, Vec::len),
                            "workingClaimCount": report_input["workingSet"]["claims"].as_array().map_or(0, Vec::len),
                            "workingWorkUnitCount": report_input["workingSet"]["workUnits"].as_array().map_or(0, Vec::len),
                        }));
                    }
                    let artifact = store.create_visual_artifact(VisualArtifactCreateInput {
                        task_id,
                        feature_id,
                        work_unit_id,
                        stage: Some(stage),
                        kind: "stage-report".into(),
                        title: params.title.unwrap_or_else(|| format!("{} Stage Report", report_input["stage"].as_str().unwrap_or("stage").to_uppercase())),
                        summary: required(params.summary.or(params.content), "summary", "task_stage_report")?,
                        path: required(params.path, "path", "task_stage_report")?,
                        mime_type: Some("text/html".into()),
                        metadata: Some(metadata),
                        created_by: params.created_by.unwrap_or(work_context.agent_id),
                    })?;
                    output(
                        format!("✓ Stage report attached · {} · {}", artifact.stage.as_deref().unwrap_or("no-stage"), artifact.title),
                        with_base(json!({"artifact": artifact, "reportInput": report_input, "workingSetScope": "self"})),
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
                            depends_on: if params.depends_on.is_empty() {
                                None
                            } else {
                                Some(
                                    params
                                        .depends_on
                                        .into_iter()
                                        .map(|reference| resolve_task(store, reference, action.as_str()))
                                        .collect::<Result<Vec<_>>>()?,
                                )
                            },
                            clear_dependencies: params.clear_dependencies,
                            active: params.active,
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
                "task_index" => {
                    let task_id = resolve_task(
                        store,
                        required(params.task_id, "task_id", "task_index")?,
                        "task_index",
                    )?;
                    let task = if let Some(indexes) = params.index_set {
                        store.set_task_indexes(&task_id, indexes)?
                    } else {
                        if let Some(remove) = params.index_remove.as_ref().filter(|paths| !paths.is_empty()) {
                            store.remove_task_indexes(&task_id, remove)?;
                        }
                        if let Some(add) = params.index_add {
                            store.add_task_indexes(&task_id, add)?
                        } else {
                            store
                                .list_tasks(None)?
                                .into_iter()
                                .find(|task| task.id == task_id)
                                .ok_or_else(|| anyhow!("task disappeared while updating indexes"))?
                        }
                    };
                    let index_count = task.indexes.as_array().map_or(0, Vec::len);
                    let indexes = task.indexes.clone();
                    output(
                        format!("Indexes updated on #{} ({index_count} entries)", task.display_id),
                        with_base(json!({"task": task, "indexes": indexes, "indexCount": index_count})),
                    )
                }
                "feature_list" => {
                    let snapshot_features = store.list_features()?;
                    let parent_filter = match params.parent_feature_id.as_deref() {
                        Some("root") => Some(None),
                        Some(reference) => Some(Some(resolve_feature(store, reference.to_string(), "feature_list")?)),
                        None => None,
                    };
                    let mut features = snapshot_features
                        .iter()
                        .filter(|feature| params.state.as_deref().is_none_or(|state| feature.state == state))
                        .filter(|feature| match parent_filter.as_ref() {
                            Some(Some(parent)) => feature.parent_feature_id.as_deref() == Some(parent.as_str()),
                            Some(None) => feature.parent_feature_id.is_none(),
                            None => true,
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    features.sort_by(|a, b| a.display_id.cmp(&b.display_id).then_with(|| a.id.cmp(&b.id)));
                    let total = features.len();
                    let progress = features
                        .iter()
                        .map(|feature| {
                            let gates = feature.gates.as_array().cloned().unwrap_or_default();
                            let passed = gates
                                .iter()
                                .filter(|gate| gate.get("status").and_then(Value::as_str) == Some("passed"))
                                .count();
                            json!({"featureId": feature.id, "displayId": feature.display_id, "gatePassed": passed, "gateTotal": gates.len()})
                        })
                        .collect::<Vec<_>>();
                    output(
                        format!("{total} features"),
                        with_base(json!({
                            "features": features.into_iter().take(limit).collect::<Vec<_>>(),
                            "progress": progress.into_iter().take(limit).collect::<Vec<_>>(),
                            "counts": {
                                "total": snapshot_features.len(),
                                "open": snapshot_features.iter().filter(|feature| feature.state == "open").count(),
                                "active": snapshot_features.iter().filter(|feature| feature.state == "active").count(),
                                "closed": snapshot_features.iter().filter(|feature| feature.state == "closed").count(),
                                "archived": snapshot_features.iter().filter(|feature| feature.state == "archived").count(),
                            },
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
                    let all_features = store.list_features()?;
                    let child_features = all_features
                        .iter()
                        .filter(|candidate| candidate.parent_feature_id.as_deref() == Some(feature_id.as_str()))
                        .cloned()
                        .collect::<Vec<_>>();
                    let subtree_ids = {
                        let mut ids = std::collections::BTreeSet::new();
                        let mut queue = vec![feature_id.clone()];
                        while let Some(id) = queue.pop() {
                            if ids.insert(id.clone()) {
                                queue.extend(
                                    all_features
                                        .iter()
                                        .filter(|candidate| candidate.parent_feature_id.as_deref() == Some(id.as_str()))
                                        .map(|candidate| candidate.id.clone()),
                                );
                            }
                        }
                        ids
                    };
                    let tasks = store
                        .list_tasks(None)?
                        .into_iter()
                        .filter(|task| task.feature_id.as_deref() == Some(feature_id.as_str()))
                        .collect::<Vec<_>>();
                    let subtree_tasks = store
                        .list_tasks(None)?
                        .into_iter()
                        .filter(|task| task.feature_id.as_ref().is_some_and(|id| subtree_ids.contains(id)))
                        .collect::<Vec<_>>();
                    let done_tasks = tasks.iter().filter(|task| task.state == "done").count();
                    let rollup_done_tasks = subtree_tasks.iter().filter(|task| task.state == "done").count();
                    let total_tasks = tasks.len();
                    let total_subtree_tasks = subtree_tasks.len();
                    let progress_ratio = if total_tasks == 0 {
                        0.0
                    } else {
                        done_tasks as f64 / total_tasks as f64
                    };
                    let rollup_progress_ratio = if total_subtree_tasks == 0 {
                        0.0
                    } else {
                        rollup_done_tasks as f64 / total_subtree_tasks as f64
                    };
                    let gates = feature.gates.as_array().cloned().unwrap_or_default();
                    let passed_gates = gates
                        .iter()
                        .filter(|gate| gate.get("status").and_then(Value::as_str) == Some("passed"))
                        .count();
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
                            "childFeatures": child_features,
                            "subtree": {"featureIds": subtree_ids, "tasks": subtree_tasks},
                            "progress": {"doneTasks": done_tasks, "totalTasks": total_tasks, "ratio": progress_ratio},
                            "rollupProgress": {"doneTasks": rollup_done_tasks, "totalTasks": total_subtree_tasks, "ratio": rollup_progress_ratio},
                            "gateProgress": {"passed": passed_gates, "total": gates.len()},
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
    use std::sync::{Mutex, OnceLock};

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

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    struct EnvGuard {
        key: &'static str,
        old: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let old = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, old }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(old) = &self.old {
                    std::env::set_var(self.key, old);
                } else {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    fn write_executable(path: &Path, content: &str) {
        std::fs::write(path, content).expect("write fake executable");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).expect("chmod fake executable");
        }
    }

    #[test]
    fn command_resolver_honors_env_cwd_and_records_output() {
        let root = tempfile::tempdir().expect("workspace tempdir");
        let cwd = root.path().join("subdir");
        std::fs::create_dir(&cwd).expect("create cwd");
        let result = execute_gate_resolver(
            &GateResolver::Command {
                run: "printf 'out:%s:%s\\n' \"$SPECIAL_VALUE\" \"$PWD\"; printf 'errline\\n' >&2"
                    .into(),
                cwd: Some(cwd.to_string_lossy().into_owned()),
                timeout: Some(5_000),
                env: Some(std::collections::BTreeMap::from([(
                    "SPECIAL_VALUE".into(),
                    "present".into(),
                )])),
            },
            root.path(),
        )
        .expect("command resolver");

        assert_eq!(result.status, "passed");
        assert_eq!(result.exit_code, 0);
        assert!(result.full_log.contains("out:present:"));
        assert!(result.full_log.contains(cwd.to_string_lossy().as_ref()));
        assert!(result.full_log.contains("---stderr---\nerrline"));
        assert_eq!(result.note, result.full_log);
    }

    #[test]
    fn command_resolver_times_out_kills_and_reaps() {
        let root = tempfile::tempdir().expect("workspace tempdir");
        let result = execute_gate_resolver(
            &GateResolver::Command {
                run: "echo before-timeout; sleep 5; echo after-timeout".into(),
                cwd: None,
                timeout: Some(50),
                env: None,
            },
            root.path(),
        )
        .expect("timeout resolver");

        assert_eq!(result.status, "failed");
        assert_eq!(result.exit_code, 124);
        assert!(result.full_log.contains("timed out after 50 ms"));
        assert!(result.duration_ms < 5_000);
    }

    #[test]
    fn tool_resolver_invokes_fake_jcode_with_allow_list_and_prompt() {
        let _lock = env_lock();
        let root = tempfile::tempdir().expect("workspace tempdir");
        let fake = root.path().join("fake-jcode");
        let args_file = root.path().join("args.txt");
        write_executable(
            &fake,
            &format!(
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$PWD\" > {}\nprintf '%s\\n' \"$@\" >> {}\necho tool-ok\n",
                args_file.display(),
                args_file.display()
            ),
        );
        let _guard = EnvGuard::set("JCODE_BIN", &fake);

        let result = execute_gate_resolver(
            &GateResolver::Tool {
                tool: "tasker".into(),
                args: Some(json!({"action": "status"})),
            },
            root.path(),
        )
        .expect("tool resolver");

        assert_eq!(result.status, "passed");
        assert!(result.full_log.contains("tool-ok"));
        let captured = std::fs::read_to_string(args_file).expect("captured args");
        assert!(captured.starts_with(&format!("{}\n", root.path().display())));
        assert!(captured.contains("--disable-base-tools\n--tools\ntasker\nrun\n--json\n"));
        assert!(captured.contains("Call exactly one tool named \"tasker\""));
        assert!(captured.contains("{\"action\":\"status\"}"));
    }

    #[test]
    fn tool_resolver_fails_when_fake_jcode_fails() {
        let _lock = env_lock();
        let root = tempfile::tempdir().expect("workspace tempdir");
        let fake = root.path().join("fake-jcode-fail");
        write_executable(&fake, "#!/usr/bin/env bash\necho nope >&2\nexit 7\n");
        let _guard = EnvGuard::set("JCODE_BIN", &fake);

        let result = execute_gate_resolver(
            &GateResolver::Tool {
                tool: "tasker".into(),
                args: None,
            },
            root.path(),
        )
        .expect("tool resolver failure result");

        assert_eq!(result.status, "failed");
        assert_eq!(result.exit_code, 7);
        assert!(result.full_log.contains("nope"));
    }

    #[test]
    fn agent_resolver_invokes_fake_jcode_with_model_and_task() {
        let _lock = env_lock();
        let root = tempfile::tempdir().expect("workspace tempdir");
        let fake = root.path().join("fake-jcode-agent");
        let args_file = root.path().join("agent-args.txt");
        write_executable(
            &fake,
            &format!(
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$@\" > {}\necho agent-ok\n",
                args_file.display()
            ),
        );
        let _guard = EnvGuard::set("JCODE_BIN", &fake);

        let result = execute_gate_resolver(
            &GateResolver::Agent {
                agent: "reviewer".into(),
                task: "Check the feature".into(),
                model: Some("test-model".into()),
            },
            root.path(),
        )
        .expect("agent resolver");

        assert_eq!(result.status, "passed");
        assert!(result.full_log.contains("agent-ok"));
        let captured = std::fs::read_to_string(args_file).expect("captured args");
        assert_eq!(
            captured,
            "--model\ntest-model\nrun\n--json\nCheck the feature\n"
        );
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
            "task_index",
            "feature_list",
            "feature_update",
            "feature_status",
            "link",
            "unlink",
            "batch",
            "plan",
            "feature_plan",
            "claim",
            "release",
            "working_set",
            "next_work_unit",
            "feature_gate",
            "task_artifact_create",
            "task_artifacts",
            "task_stage_report",
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
    async fn artifact_actions_round_trip_through_public_tool() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let (_database_dir, tool) = temp_tool();
        let ctx = context(workspace.path());

        let task = tool
            .execute(
                json!({"action": "create", "title": "Artifact task"}),
                ctx.clone(),
            )
            .await
            .expect("create task")
            .metadata
            .expect("task metadata")["task"]
            .clone();
        let task_id = task["id"].as_str().expect("task id");

        let artifact_meta = tool
            .execute(
                json!({
                    "action": "task_artifact_create",
                    "task_id": task_id,
                    "stage": "design",
                    "kind": "visual-plan",
                    "title": "Design plan",
                    "summary": "A visual plan",
                    "path": "reports/design.html",
                    "metadata": {"source": "test"}
                }),
                ctx.clone(),
            )
            .await
            .expect("create artifact")
            .metadata
            .expect("artifact metadata");
        assert_eq!(artifact_meta["artifact"]["taskId"], task_id);
        assert_eq!(artifact_meta["artifact"]["mimeType"], "text/html");
        assert_eq!(artifact_meta["artifact"]["metadata"]["source"], "test");

        let listed = tool
            .execute(
                json!({
                    "action": "task_artifacts",
                    "task_id": task_id,
                    "stage": "design",
                    "kind": "visual-plan"
                }),
                ctx.clone(),
            )
            .await
            .expect("list artifacts")
            .metadata
            .expect("listed metadata");
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["artifacts"][0]["title"], "Design plan");

        let report = tool
            .execute(
                json!({
                    "action": "task_stage_report",
                    "task_id": task_id,
                    "stage": "validate",
                    "summary": "Validation complete",
                    "path": "reports/validate.html"
                }),
                ctx,
            )
            .await
            .expect("stage report")
            .metadata
            .expect("report metadata");
        assert_eq!(report["artifact"]["kind"], "stage-report");
        assert_eq!(report["artifact"]["stage"], "validate");
        assert_eq!(
            report["artifact"]["metadata"]["taskerReportInput"]["taskId"],
            task_id
        );
        assert_eq!(report["reportInput"]["stage"], "validate");
    }

    #[tokio::test]
    async fn read_projection_actions_are_provider_safe_and_bounded() {
        let (database_dir, tool) = temp_tool();
        let root = database_dir.path();
        let schema = tool.parameters_schema();
        assert!(
            schema["properties"]["action"]["enum"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "topology_paths")
        );
        assert_eq!(schema["properties"]["max_depth"]["maximum"], 64);
        assert_eq!(schema["additionalProperties"], false);

        let feature_output = tool
            .execute(
                json!({"action": "create_feature", "title": "Projection Root"}),
                context(root),
            )
            .await
            .unwrap();
        let feature_metadata = feature_output.metadata.expect("feature metadata");
        let feature_id = feature_metadata["feature"]["id"].as_str().unwrap();

        let task_output = tool
            .execute(
                json!({"action": "create", "title": "Projection Task", "feature_id": feature_id}),
                context(root),
            )
            .await
            .unwrap();
        let task_metadata = task_output.metadata.expect("task metadata");
        let task_id = task_metadata["task"]["id"].as_str().unwrap();

        let graph = tool
            .execute(json!({"action": "task_graph", "limit": 1}), context(root))
            .await
            .unwrap();
        let graph_metadata = graph.metadata.expect("graph metadata");
        assert_eq!(graph_metadata["projection"]["limit"], 1);
        assert_eq!(graph_metadata["projection"]["counts"]["tasks"], 1);

        let neighbors = tool
            .execute(
                json!({"action": "task_neighbors", "task_id": task_id, "direction": "both", "depth": 1, "limit": 5}),
                context(root),
            )
            .await
            .unwrap();
        let neighbor_metadata = neighbors.metadata.expect("neighbor metadata");
        assert_eq!(neighbor_metadata["projection"]["nodeCount"], 1);
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
    async fn task_index_and_feature_progress_match_pi_ergonomics() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let (_database_dir, tool) = temp_tool();
        let ctx = context(workspace.path());

        tool.execute(
            json!({"action": "create_feature", "title": "Root", "gates": [{"label": "Manual gate"}]}),
            ctx.clone(),
        )
        .await
        .expect("create root feature");
        tool.execute(
            json!({"action": "create_feature", "title": "Child", "parent_feature_id": "#F1", "state": "active"}),
            ctx.clone(),
        )
        .await
        .expect("create child feature");
        tool.execute(
            json!({"action": "create", "title": "Root task", "feature_id": "#F1", "state": "done"}),
            ctx.clone(),
        )
        .await
        .expect("create root task");
        tool.execute(
            json!({"action": "create", "title": "Child task", "feature_id": "#F2"}),
            ctx.clone(),
        )
        .await
        .expect("create child task");

        let set = tool
            .execute(
                json!({
                    "action": "task_index",
                    "task_id": "#2",
                    "index_set": [{"type": "file", "path": "src/a.rs"}, {"type": "url", "path": "https://example.test"}]
                }),
                ctx.clone(),
            )
            .await
            .expect("set indexes")
            .metadata
            .expect("set metadata");
        assert_eq!(set["indexCount"], 2);

        let changed = tool
            .execute(
                json!({
                    "action": "task_index",
                    "task_id": "#2",
                    "index_remove": ["src/a.rs"],
                    "index_add": [{"type": "symbol", "path": "TaskerTool"}]
                }),
                ctx.clone(),
            )
            .await
            .expect("add/remove indexes")
            .metadata
            .expect("changed metadata");
        assert_eq!(changed["indexCount"], 2);
        assert_eq!(changed["indexes"][0]["path"], "https://example.test");
        assert_eq!(changed["indexes"][1]["path"], "TaskerTool");

        let roots = tool
            .execute(
                json!({"action": "feature_list", "parent_feature_id": "root"}),
                ctx.clone(),
            )
            .await
            .expect("list roots")
            .metadata
            .expect("roots metadata");
        assert_eq!(roots["features"].as_array().unwrap().len(), 1);
        assert_eq!(roots["counts"]["active"], 1);
        assert_eq!(roots["progress"][0]["gateTotal"], 1);

        let active = tool
            .execute(
                json!({"action": "feature_list", "state": "active"}),
                ctx.clone(),
            )
            .await
            .expect("list active")
            .metadata
            .expect("active metadata");
        assert_eq!(active["features"][0]["title"], "Child");

        let status = tool
            .execute(
                json!({"action": "feature_status", "feature_id": "#F1"}),
                ctx.clone(),
            )
            .await
            .expect("feature status")
            .metadata
            .expect("status metadata");
        assert_eq!(status["progress"]["doneTasks"], 1);
        assert_eq!(status["rollupProgress"]["totalTasks"], 2);
        assert_eq!(status["childFeatures"][0]["title"], "Child");

        let read_gate = tool
            .execute(
                json!({"action": "feature_gate", "feature_id": "#F1", "gate_index": 0}),
                ctx.clone(),
            )
            .await
            .expect("read gate")
            .metadata
            .expect("gate metadata");
        assert_eq!(read_gate["readOnly"], true);
        assert_eq!(read_gate["gate"]["status"], "pending");

        let no_scope = tool
            .execute(json!({"action": "next_work_unit"}), ctx)
            .await
            .expect("next work unit without feature scope")
            .metadata
            .expect("next metadata");
        assert_eq!(no_scope["next_work_unit"]["ok"], false);
        assert_eq!(
            no_scope["next_work_unit"]["error"],
            "feature_scope_required"
        );
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

    #[tokio::test]
    async fn resolves_manual_feature_gates_through_the_public_tool() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let (_database_dir, tool) = temp_tool();
        let ctx = context(workspace.path());

        tool.execute(
            json!({
                "action": "create_feature",
                "title": "Gated feature",
                "gates": [{"label": "Operator approval", "status": "pending"}]
            }),
            ctx.clone(),
        )
        .await
        .expect("create gated feature");

        let resolved = tool
            .execute(
                json!({
                    "action": "feature_gate",
                    "feature_id": "#F1",
                    "gate_index": 0,
                    "gate_status": "passed",
                    "resolved_by": "test",
                    "note": "approved"
                }),
                ctx.clone(),
            )
            .await
            .expect("resolve gate")
            .metadata
            .expect("gate metadata");
        assert_eq!(resolved["gate"]["status"], "passed");
        assert_eq!(resolved["gate"]["resolvedBy"], "test");
        assert_eq!(resolved["gate"]["note"], "approved");

        let status = tool
            .execute(
                json!({"action": "feature_status", "feature_id": "#F1"}),
                ctx.clone(),
            )
            .await
            .expect("feature status")
            .metadata
            .expect("status metadata");
        assert_eq!(status["feature"]["gates"][0]["status"], "passed");

        let invalid = tool
            .execute(
                json!({
                    "action": "feature_gate",
                    "feature_id": "#F1",
                    "gate_index": 0,
                    "gate_status": "waived"
                }),
                ctx,
            )
            .await;
        assert!(invalid.is_err());
    }

    #[tokio::test]
    async fn imports_atomic_feature_plans_through_the_public_tool() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let (_database_dir, tool) = temp_tool();
        let ctx = context(workspace.path());

        let imported = tool
            .execute(
                json!({
                    "action": "feature_plan",
                    "feature": {
                        "key": "root",
                        "title": "Migration program",
                        "brief": "Retire Pi after measured parity",
                        "acceptance": [{"criterion": "Canonical writes verified"}],
                        "notes": [{"category": "context", "content": "atomic feature plan"}],
                        "gates": [{"label": "Operator approval"}],
                        "children": [{
                            "key": "bridge",
                            "title": "Compatibility bridge",
                            "tasks": [{"key": "ground", "title": "Ground Pi semantics", "state": "done"}]
                        }],
                        "tasks": [{"key": "port", "title": "Port behavior", "after": ["ground"]}]
                    }
                }),
                ctx.clone(),
            )
            .await
            .expect("import feature plan")
            .metadata
            .expect("feature plan metadata");
        assert_eq!(imported["feature_plan"]["featureCount"], 2);
        assert_eq!(imported["feature_plan"]["taskCount"], 2);
        assert_eq!(imported["feature_plan"]["dependencyCount"], 1);
        assert_eq!(
            imported["feature_plan"]["feature"]["brief"],
            "Retire Pi after measured parity"
        );
        assert_eq!(
            imported["feature_plan"]["feature"]["gates"][0]["status"],
            "pending"
        );

        let before = tool
            .execute(json!({"action": "status"}), ctx.clone())
            .await
            .expect("status before duplicate")
            .metadata
            .expect("before metadata");
        let duplicate = tool
            .execute(
                json!({
                    "action": "feature_plan",
                    "feature": {
                        "key": "duplicate",
                        "title": "Must roll back",
                        "tasks": [
                            {"key": "same", "title": "One"},
                            {"key": "same", "title": "Two"}
                        ]
                    }
                }),
                ctx.clone(),
            )
            .await;
        assert!(duplicate.is_err());
        let after = tool
            .execute(json!({"action": "status"}), ctx)
            .await
            .expect("status after duplicate")
            .metadata
            .expect("after metadata");
        assert_eq!(before["counts"]["features"], after["counts"]["features"]);
        assert_eq!(before["counts"]["tasks"], after["counts"]["tasks"]);
    }
}
