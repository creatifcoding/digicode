use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, PiTaskerError>;

#[derive(Debug, Error)]
pub enum PiTaskerError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("schema preflight failed: {0}")]
    Schema(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid reference: {0}")]
    InvalidReference(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPartition {
    pub db_path: PathBuf,
    pub project_root: String,
    pub list_id: String,
}

impl ProjectPartition {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        let root = canonical_project_root(project_root.as_ref());
        Self::with_db_path(default_db_path(), root)
    }

    pub fn with_db_path(db_path: impl Into<PathBuf>, project_root: impl Into<String>) -> Self {
        let project_root = project_root.into();
        let list_id = derive_list_id(&project_root);
        Self {
            db_path: db_path.into(),
            project_root,
            list_id,
        }
    }
}

pub fn default_db_path() -> PathBuf {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"));
    home.join(".pi/tasker/tasks.db")
}

pub fn canonical_project_root(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

pub fn derive_list_id(project_root: &str) -> String {
    if let Ok(explicit) = env::var("CLAUDE_CODE_TASK_LIST_ID") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return make_task_list_id(trimmed);
        }
    }
    let mut hasher = Sha1::new();
    hasher.update(project_root.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    make_task_list_id(&hex[..10])
}

pub fn make_task_list_id(raw: &str) -> String {
    if raw.starts_with("list_") {
        raw.to_owned()
    } else {
        format!("list_{raw}")
    }
}

pub fn make_task_id() -> String {
    prefixed_uuid("task_")
}
pub fn make_feature_id() -> String {
    prefixed_uuid("feat_")
}
pub fn make_task_note_id() -> String {
    prefixed_uuid("note_")
}
pub fn make_feature_note_id() -> String {
    prefixed_uuid("fnote_")
}
pub fn make_session_instance_id() -> String {
    prefixed_uuid("sessinst_")
}
pub fn make_task_claim_id() -> String {
    prefixed_uuid("claim_")
}
pub fn make_work_unit_id() -> String {
    prefixed_uuid("wu_")
}
pub fn make_visual_artifact_id() -> String {
    prefixed_uuid("artifact_")
}
pub fn make_task_dependency_id(task_id: &str, depends_on_id: &str) -> String {
    format!("dep_{task_id}_{depends_on_id}")
}
pub fn make_feature_dependency_id(feature_id: &str, depends_on_id: &str) -> String {
    format!("fdep_{feature_id}_{depends_on_id}")
}

fn prefixed_uuid(prefix: &str) -> String {
    let compact = Uuid::new_v4().simple().to_string();
    format!("{prefix}{}", &compact[..12])
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn project_name(project_root: &str) -> &str {
    Path::new(project_root)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(project_root)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskListMeta {
    pub list_id: String,
    pub project_root: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub list_id: String,
    pub project_root: String,
    pub display_id: i64,
    pub feature_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub indexes: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub id: String,
    pub list_id: String,
    pub project_root: String,
    pub display_id: i64,
    pub parent_feature_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub priority: String,
    pub tags: Value,
    pub brief: Option<String>,
    pub acceptance: Value,
    pub owner: Option<String>,
    pub gates: Value,
    pub indexes: Value,
    pub depth: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeatureGate {
    pub label: String,
    pub resolver: Option<GateResolver>,
    pub status: String,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<i64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "_tag", rename_all = "camelCase")]
pub enum GateResolver {
    #[serde(rename = "command", rename_all = "camelCase")]
    Command {
        run: String,
        cwd: Option<String>,
        timeout: Option<i64>,
        env: Option<BTreeMap<String, String>>,
    },
    #[serde(rename = "tool", rename_all = "camelCase")]
    Tool { tool: String, args: Option<Value> },
    #[serde(rename = "agent", rename_all = "camelCase")]
    Agent {
        agent: String,
        task: String,
        model: Option<String>,
    },
    #[serde(rename = "script", rename_all = "camelCase")]
    Script { path: String, timeout: Option<i64> },
}

impl GateResolver {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Command { .. } => "command",
            Self::Tool { .. } => "tool",
            Self::Agent { .. } => "agent",
            Self::Script { .. } => "script",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFeatureGate {
    pub status: String,
    pub resolved_by: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeatureGateCheckResult {
    pub status: String,
    pub note: String,
    pub full_log: String,
    pub exit_code: i64,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureGateCheckMode {
    FailFast,
    CheckAll,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppliedFeatureGateCheck {
    pub gate_index: usize,
    pub gate: FeatureGate,
    pub evidence_note: Option<FeatureNote>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskDependency {
    pub id: String,
    pub task_id: String,
    pub depends_on_id: String,
    pub list_id: String,
    pub project_root: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDependency {
    pub id: String,
    pub feature_id: String,
    pub depends_on_id: String,
    pub list_id: String,
    pub project_root: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskNote {
    pub id: String,
    pub task_id: String,
    pub list_id: String,
    pub project_root: String,
    pub category: Option<String>,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeatureNote {
    pub id: String,
    pub feature_id: String,
    pub list_id: String,
    pub project_root: String,
    pub category: Option<String>,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionInstance {
    pub id: String,
    pub list_id: String,
    pub project_root: String,
    pub agent_id: String,
    pub session_id: String,
    pub session_file: Option<String>,
    pub pid: i64,
    pub model: Option<String>,
    pub leaf_id_at_start: Option<String>,
    pub current_leaf_id: Option<String>,
    pub started_at: i64,
    pub last_seen_at: i64,
    pub ended_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskClaim {
    pub id: String,
    pub task_id: String,
    pub scope_feature_id: Option<String>,
    pub list_id: String,
    pub project_root: String,
    pub agent_id: String,
    pub session_id: String,
    pub session_file: Option<String>,
    pub session_instance_id: String,
    pub pid: i64,
    pub claim_kind: String,
    pub reason: Option<String>,
    pub claimed_at: i64,
    pub expires_at: Option<i64>,
    pub released_at: Option<i64>,
    pub release_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkUnit {
    pub id: String,
    pub task_id: String,
    pub claim_id: Option<String>,
    pub scope_feature_id: Option<String>,
    pub list_id: String,
    pub project_root: String,
    pub agent_id: String,
    pub session_id: String,
    pub session_file: Option<String>,
    pub session_instance_id: String,
    pub status: String,
    pub priority: i64,
    pub note: Option<String>,
    pub created_at: i64,
    pub dispatched_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub cancelled_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VisualArtifact {
    pub id: String,
    pub list_id: String,
    pub project_root: String,
    pub task_id: Option<String>,
    pub feature_id: Option<String>,
    pub work_unit_id: Option<String>,
    pub stage: Option<String>,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub path: String,
    pub mime_type: String,
    pub metadata: Value,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VisualArtifactCreateInput {
    pub task_id: Option<String>,
    pub feature_id: Option<String>,
    pub work_unit_id: Option<String>,
    pub stage: Option<String>,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub path: String,
    pub mime_type: Option<String>,
    pub metadata: Option<Value>,
    pub created_by: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VisualArtifactQueryInput {
    pub task_id: Option<String>,
    pub feature_id: Option<String>,
    pub work_unit_id: Option<String>,
    pub stage: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkContextInput {
    pub agent_id: String,
    pub session_id: String,
    pub session_instance_id: String,
    pub session_file: Option<String>,
    pub pid: i64,
    pub model: Option<String>,
    pub leaf_id_at_start: Option<String>,
    pub current_leaf_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClaimTaskInput {
    pub task_id: String,
    pub context: WorkContextInput,
    pub claim_kind: Option<String>,
    pub reason: Option<String>,
    pub lease_ms: Option<i64>,
    pub scope_feature_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClaimResult {
    pub ok: bool,
    pub task: Option<Task>,
    pub claim: Option<TaskClaim>,
    pub scope_feature_id: Option<String>,
    pub already_held: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseClaimInput {
    pub context: WorkContextInput,
    pub task_id: Option<String>,
    pub claim_id: Option<String>,
    pub release_all: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseResult {
    pub released: Vec<TaskClaim>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClaimedTask {
    #[serde(flatten)]
    pub claim: TaskClaim,
    pub task: Option<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkUnitWithTask {
    #[serde(flatten)]
    pub work_unit: WorkUnit,
    pub task: Option<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkingSetResult {
    pub claims: Vec<ClaimedTask>,
    pub work_units: Vec<WorkUnitWithTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NextWorkUnitInput {
    pub context: WorkContextInput,
    pub claim_kind: Option<String>,
    pub reason: Option<String>,
    pub lease_ms: Option<i64>,
    pub feature_id: Option<String>,
    pub priority: Option<i64>,
    pub set_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NextWorkUnitResult {
    pub ok: bool,
    pub task: Option<Task>,
    pub claim: Option<TaskClaim>,
    pub work_unit: Option<WorkUnit>,
    pub scope_feature_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub list_meta: Option<TaskListMeta>,
    pub tasks: Vec<Task>,
    pub dependencies: Vec<TaskDependency>,
    pub features: Vec<Feature>,
    pub feature_dependencies: Vec<FeatureDependency>,
    pub task_notes: Vec<TaskNote>,
    pub feature_notes: Vec<FeatureNote>,
}

#[derive(Debug, Clone, Default)]
pub struct CreateTask {
    pub title: String,
    pub description: Option<String>,
    pub state: Option<String>,
    pub feature_id: Option<String>,
    pub indexes: Option<Value>,
    pub depends_on: Vec<String>,
}
#[derive(Debug, Clone, Default)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub state: Option<String>,
    pub feature_id: Option<Option<String>>,
    pub indexes: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NoteInput {
    pub content: String,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum BatchOperation {
    #[serde(rename = "create", rename_all = "camelCase")]
    Create {
        key: Option<String>,
        title: String,
        description: Option<String>,
        state: Option<String>,
        #[serde(default)]
        depends_on: Vec<String>,
        #[serde(default)]
        notes: Vec<NoteInput>,
        indexes: Option<Value>,
    },
    #[serde(rename = "update", rename_all = "camelCase")]
    Update {
        task_id: String,
        title: Option<String>,
        #[serde(default)]
        description: Option<Option<String>>,
        state: Option<String>,
        #[serde(default)]
        depends_on: Vec<String>,
        #[serde(default)]
        clear_dependencies: bool,
        active: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BatchResultEntry {
    pub op: String,
    pub key: Option<String>,
    pub task_id: Option<String>,
    pub task: Task,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub operations: Vec<BatchResultEntry>,
    pub key_map: BTreeMap<String, String>,
    pub created: usize,
    pub updated: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanTask {
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub state: Option<String>,
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(default)]
    pub notes: Vec<NoteInput>,
    pub indexes: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanResult {
    pub tasks: Vec<Task>,
    pub key_map: BTreeMap<String, String>,
    pub dependencies: Vec<TaskDependency>,
    pub task_count: usize,
    pub dependency_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeaturePlanInput {
    pub feature: FeaturePlanFeature,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeaturePlanFeature {
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub brief: Option<String>,
    #[serde(default)]
    pub acceptance: Vec<FeaturePlanAcceptance>,
    pub owner: Option<String>,
    #[serde(default)]
    pub gates: Vec<Value>,
    #[serde(default)]
    pub indexes: Vec<Value>,
    #[serde(default)]
    pub notes: Vec<NoteInput>,
    #[serde(default)]
    pub children: Vec<FeaturePlanChild>,
    #[serde(default)]
    pub tasks: Vec<FeaturePlanTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeaturePlanChild {
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub gates: Vec<Value>,
    #[serde(default)]
    pub children: Vec<FeaturePlanChild>,
    #[serde(default)]
    pub tasks: Vec<FeaturePlanTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeaturePlanTask {
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub state: Option<String>,
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(default)]
    pub notes: Vec<NoteInput>,
    pub indexes: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeaturePlanAcceptance {
    pub criterion: String,
    #[serde(default)]
    pub met: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeaturePlanResult {
    pub feature: Feature,
    pub child_features: Vec<Feature>,
    pub tasks: Vec<Task>,
    pub key_map: BTreeMap<String, String>,
    pub feature_count: usize,
    pub task_count: usize,
    pub dependency_count: usize,
}
#[derive(Debug, Clone)]
pub struct CreateFeature {
    pub title: String,
    pub description: Option<String>,
    pub parent_feature_id: Option<String>,
    pub state: Option<String>,
    pub priority: Option<String>,
    pub tags: Value,
    pub brief: Option<String>,
    pub acceptance: Value,
    pub owner: Option<String>,
    pub gates: Value,
    pub indexes: Value,
}
#[derive(Debug, Clone, Default)]
pub struct UpdateFeature {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub parent_feature_id: Option<Option<String>>,
    pub state: Option<String>,
    pub priority: Option<String>,
    pub tags: Option<Value>,
    pub brief: Option<Option<String>>,
    pub acceptance: Option<Value>,
    pub owner: Option<Option<String>>,
    pub gates: Option<Value>,
    pub indexes: Option<Value>,
}

pub struct PiTaskerStore {
    conn: Connection,
    partition: ProjectPartition,
}

impl PiTaskerStore {
    pub fn open(partition: ProjectPartition) -> Result<Self> {
        let conn = Connection::open(&partition.db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_millis(5_000))?;
        let store = Self { conn, partition };
        store.preflight()?;
        Ok(store)
    }

    pub fn partition(&self) -> &ProjectPartition {
        &self.partition
    }
    pub fn schema_fingerprint(&self) -> Result<String> {
        schema_fingerprint(&self.conn)
    }
    pub fn preflight(&self) -> Result<()> {
        preflight(&self.conn)
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        Ok(Snapshot {
            list_meta: self.list_meta()?,
            tasks: self.list_tasks(None)?,
            dependencies: self.list_dependencies()?,
            features: self.list_features()?,
            feature_dependencies: self.list_feature_dependencies()?,
            task_notes: self.list_task_notes()?,
            feature_notes: self.list_feature_notes()?,
        })
    }

    pub fn list_meta(&self) -> Result<Option<TaskListMeta>> {
        self.conn.query_row("SELECT list_id,project_root,name,created_at,updated_at FROM task_lists WHERE list_id=?1 AND project_root=?2", params![self.partition.list_id, self.partition.project_root], |r| Ok(TaskListMeta{list_id:r.get(0)?, project_root:r.get(1)?, name:r.get(2)?, created_at:r.get(3)?, updated_at:r.get(4)?})).optional().map_err(Into::into)
    }

    pub fn ensure_list_meta(&self) -> Result<TaskListMeta> {
        if let Some(meta) = self.list_meta()? {
            return Ok(meta);
        }
        let now = now_ms();
        let name = project_name(&self.partition.project_root);
        self.conn.execute("INSERT INTO task_lists (list_id, project_root, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![self.partition.list_id, self.partition.project_root, name, now, now])?;
        Ok(self.list_meta()?.expect("inserted list meta"))
    }

    pub fn list_tasks(&self, state: Option<&str>) -> Result<Vec<Task>> {
        let mut sql = "SELECT id,list_id,project_root,display_id,feature_id,title,description,state,indexes,created_at,updated_at FROM tasks WHERE list_id=?1 AND project_root=?2".to_string();
        if state.is_some() {
            sql.push_str(" AND state=?3");
        }
        sql.push_str(" ORDER BY display_id ASC");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = if let Some(state) = state {
            stmt.query_map(
                params![self.partition.list_id, self.partition.project_root, state],
                row_task,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(
                params![self.partition.list_id, self.partition.project_root],
                row_task,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        Ok(rows)
    }

    pub fn list_features(&self) -> Result<Vec<Feature>> {
        let mut stmt = self.conn.prepare("SELECT id,list_id,project_root,display_id,parent_feature_id,title,description,state,priority,tags,brief,acceptance,owner,gates,indexes,depth,created_at,updated_at FROM features WHERE list_id=?1 AND project_root=?2 ORDER BY display_id ASC")?;
        Ok(stmt
            .query_map(
                params![self.partition.list_id, self.partition.project_root],
                row_feature,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_dependencies(&self) -> Result<Vec<TaskDependency>> {
        let mut stmt = self.conn.prepare("SELECT id,task_id,depends_on,list_id,project_root,created_at FROM task_dependencies WHERE list_id=?1 AND project_root=?2 ORDER BY created_at ASC")?;
        Ok(stmt
            .query_map(
                params![self.partition.list_id, self.partition.project_root],
                |r| {
                    Ok(TaskDependency {
                        id: r.get(0)?,
                        task_id: r.get(1)?,
                        depends_on_id: r.get(2)?,
                        list_id: r.get(3)?,
                        project_root: r.get(4)?,
                        created_at: r.get(5)?,
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_feature_dependencies(&self) -> Result<Vec<FeatureDependency>> {
        let mut stmt = self.conn.prepare("SELECT id,feature_id,depends_on,list_id,project_root,created_at FROM feature_dependencies WHERE list_id=?1 AND project_root=?2 ORDER BY created_at ASC")?;
        Ok(stmt
            .query_map(
                params![self.partition.list_id, self.partition.project_root],
                |r| {
                    Ok(FeatureDependency {
                        id: r.get(0)?,
                        feature_id: r.get(1)?,
                        depends_on_id: r.get(2)?,
                        list_id: r.get(3)?,
                        project_root: r.get(4)?,
                        created_at: r.get(5)?,
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_task_notes(&self) -> Result<Vec<TaskNote>> {
        let mut stmt = self.conn.prepare("SELECT id,task_id,list_id,project_root,category,content,created_at FROM task_notes WHERE list_id=?1 AND project_root=?2 ORDER BY created_at ASC")?;
        Ok(stmt
            .query_map(
                params![self.partition.list_id, self.partition.project_root],
                |r| {
                    Ok(TaskNote {
                        id: r.get(0)?,
                        task_id: r.get(1)?,
                        list_id: r.get(2)?,
                        project_root: r.get(3)?,
                        category: r.get(4)?,
                        content: r.get(5)?,
                        created_at: r.get(6)?,
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_feature_notes(&self) -> Result<Vec<FeatureNote>> {
        let mut stmt = self.conn.prepare("SELECT id,feature_id,list_id,project_root,category,content,created_at FROM feature_notes WHERE list_id=?1 AND project_root=?2 ORDER BY created_at ASC")?;
        Ok(stmt
            .query_map(
                params![self.partition.list_id, self.partition.project_root],
                |r| {
                    Ok(FeatureNote {
                        id: r.get(0)?,
                        feature_id: r.get(1)?,
                        list_id: r.get(2)?,
                        project_root: r.get(3)?,
                        category: r.get(4)?,
                        content: r.get(5)?,
                        created_at: r.get(6)?,
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn create_visual_artifact(
        &mut self,
        input: VisualArtifactCreateInput,
    ) -> Result<VisualArtifact> {
        let tx = self.conn.transaction()?;
        let now = now_ms();
        let artifact = VisualArtifact {
            id: make_visual_artifact_id(),
            list_id: self.partition.list_id.clone(),
            project_root: self.partition.project_root.clone(),
            task_id: input.task_id,
            feature_id: input.feature_id,
            work_unit_id: input.work_unit_id,
            stage: input.stage,
            kind: input.kind,
            title: input.title,
            summary: input.summary,
            path: input.path,
            mime_type: input.mime_type.unwrap_or_else(|| "text/html".into()),
            metadata: input
                .metadata
                .unwrap_or_else(|| Value::Object(Default::default())),
            created_by: input.created_by,
            created_at: now,
            updated_at: now,
        };
        tx.execute(
            "INSERT INTO visual_artifacts (id,list_id,project_root,task_id,feature_id,work_unit_id,stage,kind,title,summary,path,mime_type,metadata,created_by,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                artifact.id,
                artifact.list_id,
                artifact.project_root,
                artifact.task_id,
                artifact.feature_id,
                artifact.work_unit_id,
                artifact.stage,
                artifact.kind,
                artifact.title,
                artifact.summary,
                artifact.path,
                artifact.mime_type,
                serde_json::to_string(&artifact.metadata)?,
                artifact.created_by,
                artifact.created_at,
                artifact.updated_at,
            ],
        )?;
        tx.commit()?;
        Ok(artifact)
    }

    pub fn list_visual_artifacts(
        &self,
        query: VisualArtifactQueryInput,
    ) -> Result<Vec<VisualArtifact>> {
        let mut stmt = self.conn.prepare("SELECT id,list_id,project_root,task_id,feature_id,work_unit_id,stage,kind,title,summary,path,mime_type,metadata,created_by,created_at,updated_at FROM visual_artifacts WHERE list_id=?1 AND project_root=?2 ORDER BY created_at DESC")?;
        let artifacts = stmt
            .query_map(
                params![self.partition.list_id, self.partition.project_root],
                row_visual_artifact,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let limit = query.limit.unwrap_or(100).max(1);
        Ok(artifacts
            .into_iter()
            .filter(|artifact| {
                query
                    .task_id
                    .as_ref()
                    .is_none_or(|v| artifact.task_id.as_ref() == Some(v))
                    && query
                        .feature_id
                        .as_ref()
                        .is_none_or(|v| artifact.feature_id.as_ref() == Some(v))
                    && query
                        .work_unit_id
                        .as_ref()
                        .is_none_or(|v| artifact.work_unit_id.as_ref() == Some(v))
                    && query
                        .stage
                        .as_ref()
                        .is_none_or(|v| artifact.stage.as_ref() == Some(v))
                    && query.kind.as_ref().is_none_or(|v| &artifact.kind == v)
            })
            .take(limit)
            .collect())
    }

    pub fn resolve_task_id(&self, input: &str) -> Result<Option<String>> {
        resolve_task_id_from(&self.list_tasks(None)?, input)
    }
    pub fn resolve_feature_id(&self, input: &str) -> Result<Option<String>> {
        resolve_feature_id_from(&self.list_features()?, input)
    }
    pub fn ready_tasks(&self) -> Result<Vec<Task>> {
        Ok(compute_ready_tasks(
            self.list_tasks(None)?,
            self.list_dependencies()?,
        ))
    }
    pub fn search_tasks(&self, query: &str, state: Option<&str>) -> Result<Vec<Task>> {
        let needle = query.to_lowercase();
        Ok(self
            .list_tasks(state)?
            .into_iter()
            .filter(|t| {
                t.title.to_lowercase().contains(&needle)
                    || t.description
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&needle)
            })
            .collect())
    }

    pub fn claim_task(&mut self, input: ClaimTaskInput) -> Result<ClaimResult> {
        let tx = self.conn.transaction()?;
        cleanup_stale_claims_tx(&tx, &self.partition)?;
        touch_session_instance_tx(&tx, &self.partition, &input.context)?;
        let task = match load_task_tx(&tx, &self.partition, &input.task_id)? {
            Some(task) => task,
            None => {
                tx.commit()?;
                return Ok(ClaimResult {
                    ok: false,
                    task: None,
                    claim: None,
                    scope_feature_id: None,
                    already_held: false,
                    error: Some("task_not_found".into()),
                });
            }
        };
        let features = load_features_tx(&tx, &self.partition)?;
        let scope_feature_id = resolve_task_scope_feature_id(&task, &features);
        let Some(scope_feature_id) = scope_feature_id else {
            tx.commit()?;
            return Ok(ClaimResult {
                ok: false,
                task: Some(task),
                claim: None,
                scope_feature_id: None,
                already_held: false,
                error: Some("missing_feature_scope".into()),
            });
        };
        #[allow(clippy::collapsible_if)]
        if let Some(requested) = &input.scope_feature_id {
            if find_root_feature_id(&features, requested).as_deref()
                != Some(scope_feature_id.as_str())
            {
                tx.commit()?;
                return Ok(ClaimResult {
                    ok: false,
                    task: Some(task),
                    claim: None,
                    scope_feature_id: Some(scope_feature_id),
                    already_held: false,
                    error: Some("scope_mismatch".into()),
                });
            }
        }
        if let Some(existing) =
            active_claim_for_task_tx(&tx, &self.partition, &input.task_id, &scope_feature_id)?
        {
            if same_owner(&existing, &input.context) {
                tx.execute("UPDATE task_claims SET session_instance_id=?1, scope_feature_id=?2, agent_id=?3, session_file=?4, pid=?5 WHERE id=?6 AND list_id=?7 AND project_root=?8", params![input.context.session_instance_id, scope_feature_id, input.context.agent_id, input.context.session_file, input.context.pid, existing.id, self.partition.list_id, self.partition.project_root])?;
                let mut refreshed = existing;
                refreshed.session_instance_id = input.context.session_instance_id;
                refreshed.scope_feature_id = Some(scope_feature_id.clone());
                refreshed.agent_id = input.context.agent_id;
                refreshed.session_file = input.context.session_file;
                refreshed.pid = input.context.pid;
                tx.commit()?;
                return Ok(ClaimResult {
                    ok: true,
                    task: Some(task),
                    claim: Some(refreshed),
                    scope_feature_id: Some(scope_feature_id),
                    already_held: true,
                    error: None,
                });
            }
            tx.commit()?;
            return Ok(ClaimResult {
                ok: false,
                task: Some(task),
                claim: Some(existing),
                scope_feature_id: Some(scope_feature_id),
                already_held: false,
                error: Some("already_claimed".into()),
            });
        }
        let claim = insert_claim_tx(
            &tx,
            &self.partition,
            InsertClaim {
                task_id: &input.task_id,
                scope_feature_id: &scope_feature_id,
                context: &input.context,
                claim_kind: input.claim_kind.as_deref().unwrap_or("claim"),
                reason: input.reason.as_deref(),
                lease_ms: input.lease_ms,
            },
        )?;
        tx.commit()?;
        Ok(ClaimResult {
            ok: true,
            task: Some(task),
            claim: Some(claim),
            scope_feature_id: Some(scope_feature_id),
            already_held: false,
            error: None,
        })
    }

    pub fn release_claim(&mut self, input: ReleaseClaimInput) -> Result<ReleaseResult> {
        let tx = self.conn.transaction()?;
        cleanup_stale_claims_tx(&tx, &self.partition)?;
        touch_session_instance_tx(&tx, &self.partition, &input.context)?;
        let active = load_active_claims_tx(&tx, &self.partition)?;
        let has_target = input.release_all || input.task_id.is_some() || input.claim_id.is_some();
        let owned: Vec<_> = if has_target {
            active
                .into_iter()
                .filter(|claim| {
                    same_owner(claim, &input.context)
                        && (input.release_all
                            || input.task_id.as_ref().is_none_or(|id| claim.task_id == *id))
                        && input.claim_id.as_ref().is_none_or(|id| claim.id == *id)
                })
                .collect()
        } else {
            Vec::new()
        };
        let now = now_ms();
        for claim in &owned {
            tx.execute("UPDATE task_claims SET released_at=?1, release_reason=?2 WHERE id=?3 AND list_id=?4 AND project_root=?5 AND released_at IS NULL", params![now, input.reason.as_deref().unwrap_or("released"), claim.id, self.partition.list_id, self.partition.project_root])?;
            tx.execute("UPDATE work_units SET status='cancelled', cancelled_at=?1 WHERE claim_id=?2 AND list_id=?3 AND project_root=?4 AND status IN ('queued','active')", params![now, claim.id, self.partition.list_id, self.partition.project_root])?;
        }
        let count = owned.len();
        tx.commit()?;
        Ok(ReleaseResult {
            released: owned,
            count,
        })
    }

    pub fn get_working_set(&mut self, context: WorkContextInput) -> Result<WorkingSetResult> {
        let tx = self.conn.transaction()?;
        cleanup_stale_claims_tx(&tx, &self.partition)?;
        touch_session_instance_tx(&tx, &self.partition, &context)?;
        let tasks = load_tasks_tx(&tx, &self.partition)?;
        let by_id: BTreeMap<_, _> = tasks
            .into_iter()
            .map(|task| (task.id.clone(), task))
            .collect();
        let claims = load_active_claims_tx(&tx, &self.partition)?
            .into_iter()
            .filter(|claim| same_owner(claim, &context))
            .map(|claim| ClaimedTask {
                task: by_id.get(&claim.task_id).cloned(),
                claim,
            })
            .collect();
        let work_units = load_open_work_units_tx(&tx, &self.partition)?
            .into_iter()
            .filter(|unit| same_work_owner(unit, &context))
            .map(|work_unit| WorkUnitWithTask {
                task: by_id.get(&work_unit.task_id).cloned(),
                work_unit,
            })
            .collect();
        tx.commit()?;
        Ok(WorkingSetResult { claims, work_units })
    }

    pub fn enqueue_next_work_unit(
        &mut self,
        input: NextWorkUnitInput,
    ) -> Result<NextWorkUnitResult> {
        let tx = self.conn.transaction()?;
        cleanup_stale_claims_tx(&tx, &self.partition)?;
        touch_session_instance_tx(&tx, &self.partition, &input.context)?;
        let Some(feature_id) = input.feature_id.as_deref() else {
            tx.commit()?;
            return Ok(NextWorkUnitResult {
                ok: false,
                task: None,
                claim: None,
                work_unit: None,
                scope_feature_id: None,
                error: Some("feature_scope_required".into()),
            });
        };
        let tasks = load_tasks_tx(&tx, &self.partition)?;
        let deps = load_dependencies_tx(&tx, &self.partition)?;
        let features = load_features_tx(&tx, &self.partition)?;
        let Some(scope_feature_id) = find_root_feature_id(&features, feature_id) else {
            tx.commit()?;
            return Ok(NextWorkUnitResult {
                ok: false,
                task: None,
                claim: None,
                work_unit: None,
                scope_feature_id: None,
                error: Some("feature_scope_not_found".into()),
            });
        };
        let scoped = collect_feature_subtree_ids(&features, &scope_feature_id);
        let active_tasks: BTreeSet<_> = load_active_claims_tx(&tx, &self.partition)?
            .into_iter()
            .map(|claim| claim.task_id)
            .collect();
        let open_work_tasks: BTreeSet<_> = load_open_work_units_tx(&tx, &self.partition)?
            .into_iter()
            .map(|unit| unit.task_id)
            .collect();
        let task = compute_ready_tasks(tasks, deps)
            .into_iter()
            .filter(|task| {
                task.feature_id
                    .as_ref()
                    .is_some_and(|id| scoped.contains(id))
            })
            .filter(|task| !active_tasks.contains(&task.id) && !open_work_tasks.contains(&task.id))
            .min_by(|a, b| {
                a.display_id
                    .cmp(&b.display_id)
                    .then_with(|| a.id.cmp(&b.id))
            });
        let Some(mut task) = task else {
            tx.commit()?;
            return Ok(NextWorkUnitResult {
                ok: false,
                task: None,
                claim: None,
                work_unit: None,
                scope_feature_id: Some(scope_feature_id),
                error: Some("none_ready".into()),
            });
        };
        let reason = input.reason.as_deref().unwrap_or("next working unit");
        let claim = insert_claim_tx(
            &tx,
            &self.partition,
            InsertClaim {
                task_id: &task.id,
                scope_feature_id: &scope_feature_id,
                context: &input.context,
                claim_kind: input.claim_kind.as_deref().unwrap_or("lock"),
                reason: Some(reason),
                lease_ms: input.lease_ms,
            },
        )?;
        let work_unit = insert_work_unit_tx(
            &tx,
            &self.partition,
            InsertWorkUnit {
                task_id: &task.id,
                claim_id: &claim.id,
                scope_feature_id: &scope_feature_id,
                context: &input.context,
                priority: input.priority.unwrap_or(0),
                note: Some(reason),
            },
        )?;
        if task.state == "todo" && input.set_active != Some(false) {
            task.state = "in_progress".into();
            task.updated_at = now_ms();
            tx.execute("UPDATE tasks SET state=?1, updated_at=?2 WHERE id=?3 AND list_id=?4 AND project_root=?5", params![task.state, task.updated_at, task.id, self.partition.list_id, self.partition.project_root])?;
        }
        tx.commit()?;
        Ok(NextWorkUnitResult {
            ok: true,
            task: Some(task),
            claim: Some(claim),
            work_unit: Some(work_unit),
            scope_feature_id: Some(scope_feature_id),
            error: None,
        })
    }

    pub fn create_task(&mut self, input: CreateTask) -> Result<Task> {
        let tx = self.conn.transaction()?;
        ensure_list_meta_tx(&tx, &self.partition)?;
        let now = now_ms();
        let display_id: i64 = tx.query_row(
            "SELECT COALESCE(MAX(display_id),0)+1 FROM tasks WHERE list_id=?1 AND project_root=?2",
            params![self.partition.list_id, self.partition.project_root],
            |r| r.get(0),
        )?;
        let task = Task {
            id: make_task_id(),
            list_id: self.partition.list_id.clone(),
            project_root: self.partition.project_root.clone(),
            display_id,
            feature_id: input.feature_id,
            title: input.title,
            description: input.description,
            state: input.state.unwrap_or_else(|| "todo".into()),
            indexes: input.indexes.unwrap_or_else(|| Value::Array(vec![])),
            created_at: now,
            updated_at: now,
        };
        tx.execute("INSERT INTO tasks (id,list_id,project_root,display_id,feature_id,title,description,state,indexes,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)", params![task.id, task.list_id, task.project_root, task.display_id, task.feature_id, task.title, task.description, task.state, serde_json::to_string(&task.indexes)?, task.created_at, task.updated_at])?;
        set_dependencies_tx(&tx, &self.partition, &task.id, &input.depends_on)?;
        tx.commit()?;
        Ok(task)
    }

    pub fn update_task(&mut self, task_id: &str, input: UpdateTask) -> Result<Task> {
        let tx = self.conn.transaction()?;
        let mut task = load_task_tx(&tx, &self.partition, task_id)?
            .ok_or_else(|| PiTaskerError::NotFound(task_id.into()))?;
        if let Some(v) = input.title {
            task.title = v;
        }
        if let Some(v) = input.description {
            task.description = v;
        }
        if let Some(v) = input.state {
            task.state = v;
        }
        if let Some(v) = input.feature_id {
            task.feature_id = v;
        }
        if let Some(v) = input.indexes {
            task.indexes = v;
        }
        task.updated_at = now_ms();
        tx.execute("UPDATE tasks SET title=?1,description=?2,state=?3,feature_id=?4,indexes=?5,updated_at=?6 WHERE id=?7 AND list_id=?8 AND project_root=?9", params![task.title, task.description, task.state, task.feature_id, serde_json::to_string(&task.indexes)?, task.updated_at, task.id, self.partition.list_id, self.partition.project_root])?;
        if task.state == "done" {
            tx.execute(
                "UPDATE work_units SET status='done', completed_at=?1 WHERE task_id=?2 AND list_id=?3 AND project_root=?4 AND status IN ('queued','active')",
                params![task.updated_at, task.id, self.partition.list_id, self.partition.project_root],
            )?;
            tx.execute(
                "UPDATE task_claims SET released_at=?1, release_reason='task_done' WHERE task_id=?2 AND list_id=?3 AND project_root=?4 AND released_at IS NULL",
                params![task.updated_at, task.id, self.partition.list_id, self.partition.project_root],
            )?;
        }
        tx.commit()?;
        Ok(task)
    }

    pub fn batch_execute(&mut self, operations: Vec<BatchOperation>) -> Result<BatchResult> {
        let tx = self.conn.transaction()?;
        ensure_list_meta_tx(&tx, &self.partition)?;
        let snapshot_tasks = load_tasks_tx(&tx, &self.partition)?;
        let mut visible_tasks = snapshot_tasks.clone();
        let mut key_map = BTreeMap::new();
        let mut results = Vec::new();
        let mut created = 0;
        let mut updated = 0;

        for op in &operations {
            let BatchOperation::Create {
                key,
                title,
                description,
                state,
                depends_on,
                notes,
                indexes,
            } = op
            else {
                continue;
            };
            let task = create_task_tx(
                &tx,
                &self.partition,
                title.clone(),
                description.clone(),
                state.clone(),
                None,
                indexes.clone(),
            )?;
            append_task_notes_tx(&tx, &self.partition, &task.id, notes)?;
            if let Some(key) = key {
                key_map.insert(key.clone(), task.id.clone());
            }
            let resolved_deps = resolve_task_refs(depends_on, &key_map, &snapshot_tasks)?;
            if !resolved_deps.is_empty() {
                set_dependencies_tx(&tx, &self.partition, &task.id, &resolved_deps)?;
            }
            visible_tasks.push(task.clone());
            results.push(BatchResultEntry {
                op: "create".into(),
                key: key.clone(),
                task_id: None,
                task,
            });
            created += 1;
        }

        for op in &operations {
            let BatchOperation::Update {
                task_id,
                title,
                description,
                state,
                depends_on,
                clear_dependencies,
                active: _,
            } = op
            else {
                continue;
            };
            let resolved_task_id = key_map
                .get(task_id)
                .cloned()
                .or_else(|| {
                    resolve_task_id_from(&snapshot_tasks, task_id)
                        .ok()
                        .flatten()
                })
                .ok_or_else(|| PiTaskerError::InvalidReference(task_id.clone()))?;
            let mut task = visible_tasks
                .iter()
                .find(|task| task.id == resolved_task_id)
                .cloned()
                .ok_or_else(|| PiTaskerError::NotFound(task_id.clone()))?;
            if let Some(title) = title {
                task.title = title.clone();
            }
            if let Some(description) = description {
                task.description = description.clone();
            }
            if let Some(state) = state {
                task.state = state.clone();
            }
            task.updated_at = now_ms();
            tx.execute("UPDATE tasks SET title=?1,description=?2,state=?3,updated_at=?4 WHERE id=?5 AND list_id=?6 AND project_root=?7", params![task.title, task.description, task.state, task.updated_at, task.id, self.partition.list_id, self.partition.project_root])?;
            if task.state == "done" {
                complete_task_work_tx(&tx, &self.partition, &task.id, task.updated_at)?;
            }
            if *clear_dependencies {
                set_dependencies_tx(&tx, &self.partition, &task.id, &[])?;
            } else if !depends_on.is_empty() {
                let resolved_deps = resolve_task_refs(depends_on, &key_map, &snapshot_tasks)?;
                set_dependencies_tx(&tx, &self.partition, &task.id, &resolved_deps)?;
            }
            if let Some(existing) = visible_tasks
                .iter_mut()
                .find(|existing| existing.id == task.id)
            {
                *existing = task.clone();
            }
            results.push(BatchResultEntry {
                op: "update".into(),
                key: None,
                task_id: Some(task_id.clone()),
                task,
            });
            updated += 1;
        }

        tx.commit()?;
        Ok(BatchResult {
            operations: results,
            key_map,
            created,
            updated,
        })
    }

    pub fn plan_import(&mut self, tasks: Vec<PlanTask>) -> Result<PlanResult> {
        let valid_keys: BTreeSet<_> = tasks.iter().map(|task| task.key.as_str()).collect();
        for task in &tasks {
            for dep_key in &task.after {
                if !valid_keys.contains(dep_key.as_str()) {
                    return Err(PiTaskerError::InvalidReference(dep_key.clone()));
                }
            }
        }

        let tx = self.conn.transaction()?;
        ensure_list_meta_tx(&tx, &self.partition)?;
        let mut key_map = BTreeMap::new();
        let mut created_tasks = Vec::new();
        let mut dependencies = Vec::new();

        for task in &tasks {
            let created = create_task_tx(
                &tx,
                &self.partition,
                task.title.clone(),
                task.description.clone(),
                task.state.clone(),
                None,
                task.indexes.clone(),
            )?;
            append_task_notes_tx(&tx, &self.partition, &created.id, &task.notes)?;
            key_map.insert(task.key.clone(), created.id.clone());
            created_tasks.push(created);
        }

        for task in &tasks {
            if task.after.is_empty() {
                continue;
            }
            let task_id = key_map
                .get(&task.key)
                .cloned()
                .ok_or_else(|| PiTaskerError::InvalidReference(task.key.clone()))?;
            let dep_ids = task
                .after
                .iter()
                .map(|key| {
                    key_map
                        .get(key)
                        .cloned()
                        .ok_or_else(|| PiTaskerError::InvalidReference(key.clone()))
                })
                .collect::<Result<Vec<_>>>()?;
            set_dependencies_tx(&tx, &self.partition, &task_id, &dep_ids)?;
            dependencies.extend(load_task_dependencies_tx(&tx, &self.partition, &task_id)?);
        }

        let task_count = created_tasks.len();
        let dependency_count = dependencies.len();
        tx.commit()?;
        Ok(PlanResult {
            tasks: created_tasks,
            key_map,
            dependencies,
            task_count,
            dependency_count,
        })
    }

    pub fn feature_plan_import(&mut self, input: FeaturePlanInput) -> Result<FeaturePlanResult> {
        validate_feature_plan_keys(&input.feature)?;

        let tx = self.conn.transaction()?;
        ensure_list_meta_tx(&tx, &self.partition)?;

        let mut key_map = BTreeMap::new();
        let mut features = Vec::new();
        let mut tasks = Vec::new();
        let mut dependency_count = 0;

        create_feature_plan_node_tx(
            &tx,
            &self.partition,
            FeaturePlanNode::Root(&input.feature),
            None,
            0,
            &mut key_map,
            &mut features,
            &mut tasks,
            &mut dependency_count,
        )?;

        let feature_count = features.len();
        let task_count = tasks.len();
        let feature = features
            .first()
            .cloned()
            .ok_or_else(|| PiTaskerError::InvalidReference("feature plan root".into()))?;
        let child_features = features.iter().skip(1).cloned().collect();
        tx.commit()?;

        Ok(FeaturePlanResult {
            feature,
            child_features,
            tasks,
            key_map,
            feature_count,
            task_count,
            dependency_count,
        })
    }

    pub fn create_feature(&mut self, input: CreateFeature) -> Result<Feature> {
        let tx = self.conn.transaction()?;
        ensure_list_meta_tx(&tx, &self.partition)?;
        let now = now_ms();
        let display_id: i64 = tx.query_row("SELECT COALESCE(MAX(display_id),0)+1 FROM features WHERE list_id=?1 AND project_root=?2", params![self.partition.list_id, self.partition.project_root], |r| r.get(0))?;
        let depth = if let Some(parent) = &input.parent_feature_id {
            tx.query_row(
                "SELECT depth+1 FROM features WHERE id=?1 AND list_id=?2 AND project_root=?3",
                params![parent, self.partition.list_id, self.partition.project_root],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| PiTaskerError::InvalidReference(parent.clone()))?
        } else {
            0
        };
        let feature = Feature {
            id: make_feature_id(),
            list_id: self.partition.list_id.clone(),
            project_root: self.partition.project_root.clone(),
            display_id,
            parent_feature_id: input.parent_feature_id,
            title: input.title,
            description: input.description,
            state: input.state.unwrap_or_else(|| "open".into()),
            priority: input.priority.unwrap_or_else(|| "medium".into()),
            tags: input.tags,
            brief: input.brief,
            acceptance: input.acceptance,
            owner: input.owner,
            gates: input.gates,
            indexes: input.indexes,
            depth,
            created_at: now,
            updated_at: now,
        };
        tx.execute("INSERT INTO features (id,list_id,project_root,display_id,parent_feature_id,title,description,state,priority,tags,brief,acceptance,owner,gates,indexes,depth,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)", params![feature.id, feature.list_id, feature.project_root, feature.display_id, feature.parent_feature_id, feature.title, feature.description, feature.state, feature.priority, serde_json::to_string(&feature.tags)?, feature.brief, serde_json::to_string(&feature.acceptance)?, feature.owner, serde_json::to_string(&feature.gates)?, serde_json::to_string(&feature.indexes)?, feature.depth, feature.created_at, feature.updated_at])?;
        tx.commit()?;
        Ok(feature)
    }

    pub fn update_feature(&mut self, feature_id: &str, input: UpdateFeature) -> Result<Feature> {
        let tx = self.conn.transaction()?;
        let mut f = load_feature_tx(&tx, &self.partition, feature_id)?
            .ok_or_else(|| PiTaskerError::NotFound(feature_id.into()))?;
        if let Some(v) = input.title {
            f.title = v;
        }
        if let Some(v) = input.description {
            f.description = v;
        }
        if let Some(v) = input.parent_feature_id {
            f.parent_feature_id = v;
        }
        if let Some(v) = input.state {
            f.state = v;
        }
        if let Some(v) = input.priority {
            f.priority = v;
        }
        if let Some(v) = input.tags {
            f.tags = v;
        }
        if let Some(v) = input.brief {
            f.brief = v;
        }
        if let Some(v) = input.acceptance {
            f.acceptance = v;
        }
        if let Some(v) = input.owner {
            f.owner = v;
        }
        if let Some(v) = input.gates {
            f.gates = v;
        }
        if let Some(v) = input.indexes {
            f.indexes = v;
        }
        f.depth = if let Some(parent) = &f.parent_feature_id {
            tx.query_row(
                "SELECT depth+1 FROM features WHERE id=?1 AND list_id=?2 AND project_root=?3",
                params![parent, self.partition.list_id, self.partition.project_root],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| PiTaskerError::InvalidReference(parent.clone()))?
        } else {
            0
        };
        f.updated_at = now_ms();
        tx.execute("UPDATE features SET parent_feature_id=?1,title=?2,description=?3,state=?4,priority=?5,tags=?6,brief=?7,acceptance=?8,owner=?9,gates=?10,indexes=?11,depth=?12,updated_at=?13 WHERE id=?14 AND list_id=?15 AND project_root=?16", params![f.parent_feature_id, f.title, f.description, f.state, f.priority, serde_json::to_string(&f.tags)?, f.brief, serde_json::to_string(&f.acceptance)?, f.owner, serde_json::to_string(&f.gates)?, serde_json::to_string(&f.indexes)?, f.depth, f.updated_at, f.id, self.partition.list_id, self.partition.project_root])?;
        tx.commit()?;
        Ok(f)
    }

    pub fn feature_gates(&self, feature_id: &str) -> Result<Vec<FeatureGate>> {
        let feature = load_feature_conn(&self.conn, &self.partition, feature_id)?
            .ok_or_else(|| PiTaskerError::NotFound(feature_id.into()))?;
        feature_gates_from_value(feature.gates)
    }

    pub fn feature_gate(&self, feature_id: &str, gate_index: usize) -> Result<FeatureGate> {
        self.feature_gates(feature_id)?
            .into_iter()
            .nth(gate_index)
            .ok_or_else(|| PiTaskerError::InvalidReference(format!("gate index {gate_index}")))
    }

    pub fn pending_executable_gate_indexes(&self, feature_id: &str) -> Result<Vec<usize>> {
        Ok(self
            .feature_gates(feature_id)?
            .into_iter()
            .enumerate()
            .filter_map(|(index, gate)| {
                (gate.status == "pending" && gate.resolver.is_some()).then_some(index)
            })
            .collect())
    }

    pub fn resolve_feature_gate(
        &mut self,
        feature_id: &str,
        gate_index: usize,
        input: ResolveFeatureGate,
    ) -> Result<Vec<FeatureGate>> {
        let tx = self.conn.transaction()?;
        let mut feature = load_feature_tx(&tx, &self.partition, feature_id)?
            .ok_or_else(|| PiTaskerError::NotFound(feature_id.into()))?;
        let mut gates = feature_gates_from_value(feature.gates)?;
        let gate = gates
            .get_mut(gate_index)
            .ok_or_else(|| PiTaskerError::InvalidReference(format!("gate index {gate_index}")))?;
        gate.status = input.status;
        gate.resolved_by = input.resolved_by.or_else(|| gate.resolved_by.clone());
        gate.resolved_at = Some(now_ms());
        gate.note = input.note.or_else(|| gate.note.clone());
        feature.updated_at = now_ms();
        tx.execute(
            "UPDATE features SET gates=?1, updated_at=?2 WHERE id=?3 AND list_id=?4 AND project_root=?5",
            params![
                serde_json::to_string(&gates)?,
                feature.updated_at,
                feature_id,
                self.partition.list_id,
                self.partition.project_root
            ],
        )?;
        tx.commit()?;
        Ok(gates)
    }

    pub fn apply_feature_gate_check(
        &mut self,
        feature_id: &str,
        gate_index: usize,
        result: FeatureGateCheckResult,
    ) -> Result<AppliedFeatureGateCheck> {
        let mut applied = self.apply_feature_gate_checks(
            feature_id,
            vec![(gate_index, result)],
            FeatureGateCheckMode::FailFast,
        )?;
        applied
            .pop()
            .ok_or_else(|| PiTaskerError::InvalidReference(format!("gate index {gate_index}")))
    }

    pub fn apply_feature_gate_checks(
        &mut self,
        feature_id: &str,
        results: Vec<(usize, FeatureGateCheckResult)>,
        mode: FeatureGateCheckMode,
    ) -> Result<Vec<AppliedFeatureGateCheck>> {
        let tx = self.conn.transaction()?;
        let mut feature = load_feature_tx(&tx, &self.partition, feature_id)?
            .ok_or_else(|| PiTaskerError::NotFound(feature_id.into()))?;
        let mut gates = feature_gates_from_value(feature.gates)?;
        let mut applied = Vec::new();
        let mut now = now_ms();

        for (gate_index, result) in results {
            let gate = gates.get_mut(gate_index).ok_or_else(|| {
                PiTaskerError::InvalidReference(format!("gate index {gate_index}"))
            })?;
            let resolver_kind = gate
                .resolver
                .as_ref()
                .ok_or_else(|| {
                    PiTaskerError::InvalidReference(format!("manual gate {gate_index}"))
                })?
                .kind();
            if gate.status != "pending" {
                return Err(PiTaskerError::InvalidReference(format!(
                    "gate {gate_index} already {}",
                    gate.status
                )));
            }
            gate.status = result.status.clone();
            gate.resolved_by = Some(format!("resolver:{resolver_kind}"));
            gate.resolved_at = Some(now);
            gate.note = Some(truncate_chars(&result.note, 2_000));

            let evidence_note = if result.full_log.is_empty() {
                None
            } else {
                let note = FeatureNote {
                    id: make_feature_note_id(),
                    feature_id: feature_id.into(),
                    list_id: self.partition.list_id.clone(),
                    project_root: self.partition.project_root.clone(),
                    category: Some("ref".into()),
                    content: truncate_chars(
                        &format!(
                            "[gate:{gate_index}] {}\n\nExit: {} | Duration: {}ms\n\n{}",
                            gate.label, result.exit_code, result.duration_ms, result.full_log
                        ),
                        10_000,
                    ),
                    created_at: now,
                };
                tx.execute("INSERT INTO feature_notes (id,feature_id,list_id,project_root,category,content,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![note.id, note.feature_id, note.list_id, note.project_root, note.category, note.content, note.created_at])?;
                Some(note)
            };
            applied.push(AppliedFeatureGateCheck {
                gate_index,
                gate: gate.clone(),
                evidence_note,
            });

            if mode == FeatureGateCheckMode::FailFast && result.status == "failed" {
                break;
            }
            now = now_ms();
        }

        feature.updated_at = now_ms();
        tx.execute(
            "UPDATE features SET gates=?1, updated_at=?2 WHERE id=?3 AND list_id=?4 AND project_root=?5",
            params![
                serde_json::to_string(&gates)?,
                feature.updated_at,
                feature_id,
                self.partition.list_id,
                self.partition.project_root
            ],
        )?;
        tx.commit()?;
        Ok(applied)
    }

    pub fn set_dependencies(
        &mut self,
        task_id: &str,
        depends_on: &[String],
    ) -> Result<Vec<TaskDependency>> {
        let tx = self.conn.transaction()?;
        set_dependencies_tx(&tx, &self.partition, task_id, depends_on)?;
        tx.commit()?;
        Ok(self
            .list_dependencies()?
            .into_iter()
            .filter(|d| d.task_id == task_id)
            .collect())
    }

    pub fn set_feature_dependencies(
        &mut self,
        feature_id: &str,
        depends_on: &[String],
    ) -> Result<Vec<FeatureDependency>> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM feature_dependencies WHERE feature_id=?1 AND list_id=?2 AND project_root=?3", params![feature_id, self.partition.list_id, self.partition.project_root])?;
        let now = now_ms();
        for dep in depends_on {
            let id = make_feature_dependency_id(feature_id, dep);
            tx.execute("INSERT INTO feature_dependencies (id,feature_id,depends_on,list_id,project_root,created_at) VALUES (?1,?2,?3,?4,?5,?6)", params![id, feature_id, dep, self.partition.list_id, self.partition.project_root, now])?;
        }
        tx.commit()?;
        Ok(self
            .list_feature_dependencies()?
            .into_iter()
            .filter(|d| d.feature_id == feature_id)
            .collect())
    }

    pub fn link_task(&mut self, task_id: &str, feature_id: &str) -> Result<()> {
        self.conn.execute("UPDATE tasks SET feature_id=?1, updated_at=?2 WHERE id=?3 AND list_id=?4 AND project_root=?5", params![feature_id, now_ms(), task_id, self.partition.list_id, self.partition.project_root])?;
        Ok(())
    }
    pub fn unlink_task(&mut self, task_id: &str) -> Result<()> {
        self.conn.execute("UPDATE tasks SET feature_id=NULL, updated_at=?1 WHERE id=?2 AND list_id=?3 AND project_root=?4", params![now_ms(), task_id, self.partition.list_id, self.partition.project_root])?;
        Ok(())
    }

    pub fn append_task_note(
        &mut self,
        task_id: &str,
        category: Option<&str>,
        content: &str,
    ) -> Result<TaskNote> {
        let note = TaskNote {
            id: make_task_note_id(),
            task_id: task_id.into(),
            list_id: self.partition.list_id.clone(),
            project_root: self.partition.project_root.clone(),
            category: category.map(str::to_owned),
            content: content.into(),
            created_at: now_ms(),
        };
        self.conn.execute("INSERT INTO task_notes (id,task_id,list_id,project_root,category,content,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![note.id, note.task_id, note.list_id, note.project_root, note.category, note.content, note.created_at])?;
        Ok(note)
    }

    pub fn append_feature_note(
        &mut self,
        feature_id: &str,
        category: Option<&str>,
        content: &str,
    ) -> Result<FeatureNote> {
        let note = FeatureNote {
            id: make_feature_note_id(),
            feature_id: feature_id.into(),
            list_id: self.partition.list_id.clone(),
            project_root: self.partition.project_root.clone(),
            category: category.map(str::to_owned),
            content: content.into(),
            created_at: now_ms(),
        };
        self.conn.execute("INSERT INTO feature_notes (id,feature_id,list_id,project_root,category,content,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![note.id, note.feature_id, note.list_id, note.project_root, note.category, note.content, note.created_at])?;
        Ok(note)
    }
}

fn row_task(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        list_id: r.get(1)?,
        project_root: r.get(2)?,
        display_id: r.get(3)?,
        feature_id: r.get(4)?,
        title: r.get(5)?,
        description: r.get(6)?,
        state: r.get(7)?,
        indexes: json_or_empty_array(r.get::<_, Option<String>>(8)?),
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}
fn row_feature(r: &rusqlite::Row<'_>) -> rusqlite::Result<Feature> {
    Ok(Feature {
        id: r.get(0)?,
        list_id: r.get(1)?,
        project_root: r.get(2)?,
        display_id: r.get(3)?,
        parent_feature_id: r.get(4)?,
        title: r.get(5)?,
        description: r.get(6)?,
        state: r.get(7)?,
        priority: r
            .get::<_, Option<String>>(8)?
            .unwrap_or_else(|| "medium".into()),
        tags: json_or_empty_array(r.get(9)?),
        brief: r.get(10)?,
        acceptance: json_or_empty_array(r.get(11)?),
        owner: r.get(12)?,
        gates: json_or_empty_array(r.get(13)?),
        indexes: json_or_empty_array(r.get(14)?),
        depth: r.get::<_, Option<i64>>(15)?.unwrap_or(0),
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
    })
}
fn row_task_claim(r: &rusqlite::Row<'_>) -> rusqlite::Result<TaskClaim> {
    Ok(TaskClaim {
        id: r.get(0)?,
        task_id: r.get(1)?,
        scope_feature_id: r.get(2)?,
        list_id: r.get(3)?,
        project_root: r.get(4)?,
        agent_id: r.get(5)?,
        session_id: r.get(6)?,
        session_file: r.get(7)?,
        session_instance_id: r.get(8)?,
        pid: r.get(9)?,
        claim_kind: r.get(10)?,
        reason: r.get(11)?,
        claimed_at: r.get(12)?,
        expires_at: r.get(13)?,
        released_at: r.get(14)?,
        release_reason: r.get(15)?,
    })
}
fn row_work_unit(r: &rusqlite::Row<'_>) -> rusqlite::Result<WorkUnit> {
    Ok(WorkUnit {
        id: r.get(0)?,
        task_id: r.get(1)?,
        claim_id: r.get(2)?,
        scope_feature_id: r.get(3)?,
        list_id: r.get(4)?,
        project_root: r.get(5)?,
        agent_id: r.get(6)?,
        session_id: r.get(7)?,
        session_file: r.get(8)?,
        session_instance_id: r.get(9)?,
        status: r.get(10)?,
        priority: r.get(11)?,
        note: r.get(12)?,
        created_at: r.get(13)?,
        dispatched_at: r.get(14)?,
        completed_at: r.get(15)?,
        cancelled_at: r.get(16)?,
    })
}
fn row_visual_artifact(r: &rusqlite::Row<'_>) -> rusqlite::Result<VisualArtifact> {
    Ok(VisualArtifact {
        id: r.get(0)?,
        list_id: r.get(1)?,
        project_root: r.get(2)?,
        task_id: r.get(3)?,
        feature_id: r.get(4)?,
        work_unit_id: r.get(5)?,
        stage: r.get(6)?,
        kind: r.get(7)?,
        title: r.get(8)?,
        summary: r.get(9)?,
        path: r.get(10)?,
        mime_type: r.get(11)?,
        metadata: json_or_empty_object(r.get(12)?),
        created_by: r.get(13)?,
        created_at: r.get(14)?,
        updated_at: r.get(15)?,
    })
}
fn json_or_empty_array(raw: Option<String>) -> Value {
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| Value::Array(vec![]))
}

fn json_or_empty_object(raw: Option<String>) -> Value {
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| Value::Object(Default::default()))
}

fn feature_gates_from_value(value: Value) -> Result<Vec<FeatureGate>> {
    let Value::Array(raw_gates) = value else {
        return Ok(Vec::new());
    };
    raw_gates
        .into_iter()
        .map(|raw| {
            let mut gate: FeatureGate = match raw {
                Value::Object(mut object) => {
                    if let (false, Some(legacy_type)) =
                        (object.contains_key("label"), object.get("type").cloned())
                    {
                        object.insert("label".into(), legacy_type);
                    }
                    object
                        .entry("label")
                        .or_insert_with(|| Value::String("Unknown".into()));
                    object.entry("resolver").or_insert_with(|| Value::Null);
                    object
                        .entry("status")
                        .or_insert_with(|| Value::String("pending".into()));
                    serde_json::from_value(Value::Object(object))?
                }
                other => serde_json::from_value(other)?,
            };
            if gate.status.is_empty() {
                gate.status = "pending".into();
            }
            if gate.label.is_empty() {
                gate.label = "Unknown".into();
            }
            Ok(gate)
        })
        .collect()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn load_features_tx(tx: &rusqlite::Transaction<'_>, p: &ProjectPartition) -> Result<Vec<Feature>> {
    let mut stmt = tx.prepare("SELECT id,list_id,project_root,display_id,parent_feature_id,title,description,state,priority,tags,brief,acceptance,owner,gates,indexes,depth,created_at,updated_at FROM features WHERE list_id=?1 AND project_root=?2 ORDER BY display_id ASC")?;
    Ok(stmt
        .query_map(params![p.list_id, p.project_root], row_feature)?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn load_dependencies_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
) -> Result<Vec<TaskDependency>> {
    let mut stmt = tx.prepare("SELECT id,task_id,depends_on,list_id,project_root,created_at FROM task_dependencies WHERE list_id=?1 AND project_root=?2 ORDER BY created_at ASC")?;
    Ok(stmt
        .query_map(params![p.list_id, p.project_root], |r| {
            Ok(TaskDependency {
                id: r.get(0)?,
                task_id: r.get(1)?,
                depends_on_id: r.get(2)?,
                list_id: r.get(3)?,
                project_root: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn load_active_claims_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
) -> Result<Vec<TaskClaim>> {
    let now = now_ms();
    let mut stmt = tx.prepare("SELECT id,task_id,scope_feature_id,list_id,project_root,agent_id,session_id,session_file,session_instance_id,pid,claim_kind,reason,claimed_at,expires_at,released_at,release_reason FROM task_claims WHERE list_id=?1 AND project_root=?2 AND released_at IS NULL AND (expires_at IS NULL OR expires_at>?3) ORDER BY claimed_at ASC")?;
    Ok(stmt
        .query_map(params![p.list_id, p.project_root, now], row_task_claim)?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn load_open_work_units_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
) -> Result<Vec<WorkUnit>> {
    let mut stmt = tx.prepare("SELECT id,task_id,claim_id,scope_feature_id,list_id,project_root,agent_id,session_id,session_file,session_instance_id,status,priority,note,created_at,dispatched_at,completed_at,cancelled_at FROM work_units WHERE list_id=?1 AND project_root=?2 AND status IN ('queued','active') ORDER BY priority DESC, created_at ASC")?;
    Ok(stmt
        .query_map(params![p.list_id, p.project_root], row_work_unit)?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn cleanup_stale_claims_tx(tx: &rusqlite::Transaction<'_>, p: &ProjectPartition) -> Result<()> {
    let now = now_ms();
    tx.execute("UPDATE task_claims SET released_at=?1, release_reason='expired' WHERE list_id=?2 AND project_root=?3 AND released_at IS NULL AND expires_at IS NOT NULL AND expires_at<=?1", params![now, p.list_id, p.project_root])?;
    tx.execute("UPDATE work_units SET status='cancelled', cancelled_at=?1 WHERE list_id=?2 AND project_root=?3 AND status IN ('queued','active') AND claim_id IN (SELECT id FROM task_claims WHERE list_id=?2 AND project_root=?3 AND release_reason='expired' AND released_at=?1)", params![now, p.list_id, p.project_root])?;
    Ok(())
}

fn touch_session_instance_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    context: &WorkContextInput,
) -> Result<()> {
    let now = now_ms();
    tx.execute("INSERT INTO tasker_session_instances (id,list_id,project_root,agent_id,session_id,session_file,pid,model,leaf_id_at_start,current_leaf_id,started_at,last_seen_at,ended_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,NULL) ON CONFLICT(id) DO UPDATE SET agent_id=excluded.agent_id, session_id=excluded.session_id, session_file=excluded.session_file, pid=excluded.pid, model=excluded.model, current_leaf_id=excluded.current_leaf_id, last_seen_at=excluded.last_seen_at", params![context.session_instance_id, p.list_id, p.project_root, context.agent_id, context.session_id, context.session_file, context.pid, context.model, context.leaf_id_at_start, context.current_leaf_id, now])?;
    Ok(())
}

fn active_claim_for_task_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    task_id: &str,
    scope_feature_id: &str,
) -> Result<Option<TaskClaim>> {
    Ok(load_active_claims_tx(tx, p)?.into_iter().find(|claim| {
        claim.task_id == task_id
            && (claim.scope_feature_id.as_deref() == Some(scope_feature_id)
                || claim.scope_feature_id.is_none())
    }))
}

struct InsertClaim<'a> {
    task_id: &'a str,
    scope_feature_id: &'a str,
    context: &'a WorkContextInput,
    claim_kind: &'a str,
    reason: Option<&'a str>,
    lease_ms: Option<i64>,
}

fn insert_claim_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    input: InsertClaim<'_>,
) -> Result<TaskClaim> {
    let now = now_ms();
    let claim = TaskClaim {
        id: make_task_claim_id(),
        task_id: input.task_id.into(),
        scope_feature_id: Some(input.scope_feature_id.into()),
        list_id: p.list_id.clone(),
        project_root: p.project_root.clone(),
        agent_id: input.context.agent_id.clone(),
        session_id: input.context.session_id.clone(),
        session_file: input.context.session_file.clone(),
        session_instance_id: input.context.session_instance_id.clone(),
        pid: input.context.pid,
        claim_kind: input.claim_kind.into(),
        reason: input.reason.map(str::to_owned),
        claimed_at: now,
        expires_at: input.lease_ms.filter(|ms| *ms > 0).map(|ms| now + ms),
        released_at: None,
        release_reason: None,
    };
    tx.execute("INSERT INTO task_claims (id,task_id,scope_feature_id,list_id,project_root,agent_id,session_id,session_file,session_instance_id,pid,claim_kind,reason,claimed_at,expires_at,released_at,release_reason) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)", params![claim.id, claim.task_id, claim.scope_feature_id, claim.list_id, claim.project_root, claim.agent_id, claim.session_id, claim.session_file, claim.session_instance_id, claim.pid, claim.claim_kind, claim.reason, claim.claimed_at, claim.expires_at, claim.released_at, claim.release_reason])?;
    Ok(claim)
}

struct InsertWorkUnit<'a> {
    task_id: &'a str,
    claim_id: &'a str,
    scope_feature_id: &'a str,
    context: &'a WorkContextInput,
    priority: i64,
    note: Option<&'a str>,
}

fn insert_work_unit_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    input: InsertWorkUnit<'_>,
) -> Result<WorkUnit> {
    let now = now_ms();
    let unit = WorkUnit {
        id: make_work_unit_id(),
        task_id: input.task_id.into(),
        claim_id: Some(input.claim_id.into()),
        scope_feature_id: Some(input.scope_feature_id.into()),
        list_id: p.list_id.clone(),
        project_root: p.project_root.clone(),
        agent_id: input.context.agent_id.clone(),
        session_id: input.context.session_id.clone(),
        session_file: input.context.session_file.clone(),
        session_instance_id: input.context.session_instance_id.clone(),
        status: "active".into(),
        priority: input.priority,
        note: input.note.map(str::to_owned),
        created_at: now,
        dispatched_at: Some(now),
        completed_at: None,
        cancelled_at: None,
    };
    tx.execute("INSERT INTO work_units (id,task_id,claim_id,scope_feature_id,list_id,project_root,agent_id,session_id,session_file,session_instance_id,status,priority,note,created_at,dispatched_at,completed_at,cancelled_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)", params![unit.id, unit.task_id, unit.claim_id, unit.scope_feature_id, unit.list_id, unit.project_root, unit.agent_id, unit.session_id, unit.session_file, unit.session_instance_id, unit.status, unit.priority, unit.note, unit.created_at, unit.dispatched_at, unit.completed_at, unit.cancelled_at])?;
    Ok(unit)
}

fn same_owner(claim: &TaskClaim, context: &WorkContextInput) -> bool {
    claim.agent_id == context.agent_id || claim.session_id == context.session_id
}

fn same_work_owner(unit: &WorkUnit, context: &WorkContextInput) -> bool {
    unit.agent_id == context.agent_id || unit.session_id == context.session_id
}

fn load_task_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    id: &str,
) -> Result<Option<Task>> {
    Ok(tx.query_row("SELECT id,list_id,project_root,display_id,feature_id,title,description,state,indexes,created_at,updated_at FROM tasks WHERE id=?1 AND list_id=?2 AND project_root=?3", params![id,p.list_id,p.project_root], row_task).optional()?)
}

fn load_tasks_tx(tx: &rusqlite::Transaction<'_>, p: &ProjectPartition) -> Result<Vec<Task>> {
    let mut stmt = tx.prepare("SELECT id,list_id,project_root,display_id,feature_id,title,description,state,indexes,created_at,updated_at FROM tasks WHERE list_id=?1 AND project_root=?2 ORDER BY display_id ASC")?;
    Ok(stmt
        .query_map(params![p.list_id, p.project_root], row_task)?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn create_task_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    title: String,
    description: Option<String>,
    state: Option<String>,
    feature_id: Option<String>,
    indexes: Option<Value>,
) -> Result<Task> {
    let now = now_ms();
    let display_id: i64 = tx.query_row(
        "SELECT COALESCE(MAX(display_id),0)+1 FROM tasks WHERE list_id=?1 AND project_root=?2",
        params![p.list_id, p.project_root],
        |r| r.get(0),
    )?;
    let task = Task {
        id: make_task_id(),
        list_id: p.list_id.clone(),
        project_root: p.project_root.clone(),
        display_id,
        feature_id,
        title,
        description,
        state: state.unwrap_or_else(|| "todo".into()),
        indexes: indexes.unwrap_or_else(|| Value::Array(vec![])),
        created_at: now,
        updated_at: now,
    };
    tx.execute("INSERT INTO tasks (id,list_id,project_root,display_id,feature_id,title,description,state,indexes,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)", params![task.id, task.list_id, task.project_root, task.display_id, task.feature_id, task.title, task.description, task.state, serde_json::to_string(&task.indexes)?, task.created_at, task.updated_at])?;
    Ok(task)
}

fn append_task_notes_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    task_id: &str,
    notes: &[NoteInput],
) -> Result<()> {
    for note in notes {
        tx.execute("INSERT INTO task_notes (id,task_id,list_id,project_root,category,content,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![make_task_note_id(), task_id, p.list_id, p.project_root, note.category, note.content, now_ms()])?;
    }
    Ok(())
}

fn append_feature_notes_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    feature_id: &str,
    notes: &[NoteInput],
) -> Result<()> {
    for note in notes {
        tx.execute("INSERT INTO feature_notes (id,feature_id,list_id,project_root,category,content,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![make_feature_note_id(), feature_id, p.list_id, p.project_root, note.category, note.content, now_ms()])?;
    }
    Ok(())
}

enum FeaturePlanNode<'a> {
    Root(&'a FeaturePlanFeature),
    Child(&'a FeaturePlanChild),
}

impl FeaturePlanNode<'_> {
    fn key(&self) -> &str {
        match self {
            Self::Root(feature) => &feature.key,
            Self::Child(feature) => &feature.key,
        }
    }

    fn title(&self) -> &str {
        match self {
            Self::Root(feature) => &feature.title,
            Self::Child(feature) => &feature.title,
        }
    }

    fn description(&self) -> Option<String> {
        match self {
            Self::Root(feature) => feature.description.clone(),
            Self::Child(feature) => feature.description.clone(),
        }
    }

    fn priority(&self) -> Option<String> {
        match self {
            Self::Root(feature) => feature.priority.clone(),
            Self::Child(feature) => feature.priority.clone(),
        }
    }

    fn tags(&self) -> Value {
        match self {
            Self::Root(feature) => serde_json::json!(feature.tags),
            Self::Child(feature) => serde_json::json!(feature.tags),
        }
    }

    fn gates(&self) -> Value {
        let gates = match self {
            Self::Root(feature) => &feature.gates,
            Self::Child(feature) => &feature.gates,
        };
        normalize_feature_plan_gates(gates)
    }

    fn children(&self) -> &[FeaturePlanChild] {
        match self {
            Self::Root(feature) => &feature.children,
            Self::Child(feature) => &feature.children,
        }
    }

    fn tasks(&self) -> &[FeaturePlanTask] {
        match self {
            Self::Root(feature) => &feature.tasks,
            Self::Child(feature) => &feature.tasks,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn create_feature_plan_node_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    node: FeaturePlanNode<'_>,
    parent_feature_id: Option<String>,
    depth: i64,
    key_map: &mut BTreeMap<String, String>,
    features: &mut Vec<Feature>,
    tasks: &mut Vec<Task>,
    dependency_count: &mut usize,
) -> Result<()> {
    let feature = create_feature_tx(
        tx,
        p,
        CreateFeature {
            title: node.title().to_owned(),
            description: node.description(),
            parent_feature_id,
            state: Some("open".into()),
            priority: node.priority(),
            tags: node.tags(),
            brief: match &node {
                FeaturePlanNode::Root(feature) => feature.brief.clone(),
                FeaturePlanNode::Child(_) => None,
            },
            acceptance: match &node {
                FeaturePlanNode::Root(feature) => {
                    normalize_feature_plan_acceptance(&feature.acceptance)
                }
                FeaturePlanNode::Child(_) => Value::Array(vec![]),
            },
            owner: match &node {
                FeaturePlanNode::Root(feature) => feature.owner.clone(),
                FeaturePlanNode::Child(_) => None,
            },
            gates: node.gates(),
            indexes: match &node {
                FeaturePlanNode::Root(feature) => serde_json::json!(feature.indexes),
                FeaturePlanNode::Child(_) => Value::Array(vec![]),
            },
        },
        depth,
    )?;
    key_map.insert(node.key().to_owned(), feature.id.clone());
    features.push(feature.clone());

    if let FeaturePlanNode::Root(root) = &node {
        append_feature_notes_tx(tx, p, &feature.id, &root.notes)?;
    }

    for child in node.children() {
        create_feature_plan_node_tx(
            tx,
            p,
            FeaturePlanNode::Child(child),
            Some(feature.id.clone()),
            depth + 1,
            key_map,
            features,
            tasks,
            dependency_count,
        )?;
    }

    for plan_task in node.tasks() {
        let task = create_task_tx(
            tx,
            p,
            plan_task.title.clone(),
            plan_task.description.clone(),
            plan_task.state.clone(),
            Some(feature.id.clone()),
            plan_task.indexes.clone(),
        )?;
        append_task_notes_tx(tx, p, &task.id, &plan_task.notes)?;
        key_map.insert(plan_task.key.clone(), task.id.clone());
        tasks.push(task);
    }

    for plan_task in node.tasks() {
        if plan_task.after.is_empty() {
            continue;
        }
        let task_id = key_map
            .get(&plan_task.key)
            .cloned()
            .ok_or_else(|| PiTaskerError::InvalidReference(plan_task.key.clone()))?;
        let dep_ids = plan_task
            .after
            .iter()
            .map(|key| match key_map.get(key) {
                Some(id) if id.starts_with("task_") => Ok(id.clone()),
                Some(_) => Err(PiTaskerError::InvalidReference(format!(
                    "feature key cannot be a task dependency: {key}"
                ))),
                None => Err(PiTaskerError::InvalidReference(key.clone())),
            })
            .collect::<Result<Vec<_>>>()?;
        set_dependencies_tx(tx, p, &task_id, &dep_ids)?;
        *dependency_count += dep_ids.len();
    }

    Ok(())
}

fn create_feature_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    input: CreateFeature,
    depth: i64,
) -> Result<Feature> {
    let now = now_ms();
    let display_id: i64 = tx.query_row(
        "SELECT COALESCE(MAX(display_id),0)+1 FROM features WHERE list_id=?1 AND project_root=?2",
        params![p.list_id, p.project_root],
        |r| r.get(0),
    )?;
    let feature = Feature {
        id: make_feature_id(),
        list_id: p.list_id.clone(),
        project_root: p.project_root.clone(),
        display_id,
        parent_feature_id: input.parent_feature_id,
        title: input.title,
        description: input.description,
        state: input.state.unwrap_or_else(|| "open".into()),
        priority: input.priority.unwrap_or_else(|| "medium".into()),
        tags: input.tags,
        brief: input.brief,
        acceptance: input.acceptance,
        owner: input.owner,
        gates: input.gates,
        indexes: input.indexes,
        depth,
        created_at: now,
        updated_at: now,
    };
    tx.execute("INSERT INTO features (id,list_id,project_root,display_id,parent_feature_id,title,description,state,priority,tags,brief,acceptance,owner,gates,indexes,depth,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)", params![feature.id, feature.list_id, feature.project_root, feature.display_id, feature.parent_feature_id, feature.title, feature.description, feature.state, feature.priority, serde_json::to_string(&feature.tags)?, feature.brief, serde_json::to_string(&feature.acceptance)?, feature.owner, serde_json::to_string(&feature.gates)?, serde_json::to_string(&feature.indexes)?, feature.depth, feature.created_at, feature.updated_at])?;
    Ok(feature)
}

fn normalize_feature_plan_acceptance(input: &[FeaturePlanAcceptance]) -> Value {
    Value::Array(
        input
            .iter()
            .map(|acceptance| {
                serde_json::json!({
                    "criterion": acceptance.criterion,
                    "met": false,
                })
            })
            .collect(),
    )
}

fn normalize_feature_plan_gates(input: &[Value]) -> Value {
    Value::Array(
        input
            .iter()
            .map(|gate| {
                let mut gate = gate.clone();
                if let Value::Object(object) = &mut gate {
                    object.insert("status".into(), Value::String("pending".into()));
                }
                gate
            })
            .collect(),
    )
}

fn validate_feature_plan_keys(root: &FeaturePlanFeature) -> Result<()> {
    let mut keys = BTreeSet::new();
    let mut visited_task_keys = BTreeSet::new();
    validate_feature_plan_root_keys(root, &mut keys, &mut visited_task_keys)
}

fn insert_plan_key(keys: &mut BTreeSet<String>, key: &str) -> Result<()> {
    if !keys.insert(key.to_owned()) {
        return Err(PiTaskerError::InvalidReference(format!(
            "duplicate plan key: {key}"
        )));
    }
    Ok(())
}

fn validate_feature_plan_root_keys(
    feature: &FeaturePlanFeature,
    keys: &mut BTreeSet<String>,
    visited_task_keys: &mut BTreeSet<String>,
) -> Result<()> {
    insert_plan_key(keys, &feature.key)?;
    for child in &feature.children {
        validate_feature_plan_child_keys(child, keys, visited_task_keys)?;
    }
    validate_feature_plan_tasks(&feature.tasks, keys, visited_task_keys)
}

fn validate_feature_plan_child_keys(
    feature: &FeaturePlanChild,
    keys: &mut BTreeSet<String>,
    visited_task_keys: &mut BTreeSet<String>,
) -> Result<()> {
    insert_plan_key(keys, &feature.key)?;
    for child in &feature.children {
        validate_feature_plan_child_keys(child, keys, visited_task_keys)?;
    }
    validate_feature_plan_tasks(&feature.tasks, keys, visited_task_keys)
}

fn validate_feature_plan_tasks(
    tasks: &[FeaturePlanTask],
    keys: &mut BTreeSet<String>,
    visited_task_keys: &mut BTreeSet<String>,
) -> Result<()> {
    for task in tasks {
        insert_plan_key(keys, &task.key)?;
    }
    for task in tasks {
        for dependency in &task.after {
            if keys.contains(dependency) && !visited_task_keys.contains(dependency) {
                return Err(PiTaskerError::InvalidReference(format!(
                    "feature key cannot be a task dependency: {dependency}"
                )));
            }
            if !visited_task_keys.contains(dependency) {
                return Err(PiTaskerError::InvalidReference(dependency.clone()));
            }
        }
    }
    for task in tasks {
        visited_task_keys.insert(task.key.clone());
    }
    Ok(())
}

fn load_task_dependencies_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    task_id: &str,
) -> Result<Vec<TaskDependency>> {
    let mut stmt = tx.prepare("SELECT id,task_id,depends_on,list_id,project_root,created_at FROM task_dependencies WHERE task_id=?1 AND list_id=?2 AND project_root=?3 ORDER BY created_at ASC")?;
    Ok(stmt
        .query_map(params![task_id, p.list_id, p.project_root], |r| {
            Ok(TaskDependency {
                id: r.get(0)?,
                task_id: r.get(1)?,
                depends_on_id: r.get(2)?,
                list_id: r.get(3)?,
                project_root: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn complete_task_work_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    task_id: &str,
    completed_at: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE work_units SET status='done', completed_at=?1 WHERE task_id=?2 AND list_id=?3 AND project_root=?4 AND status IN ('queued','active')",
        params![completed_at, task_id, p.list_id, p.project_root],
    )?;
    tx.execute(
        "UPDATE task_claims SET released_at=?1, release_reason='task_done' WHERE task_id=?2 AND list_id=?3 AND project_root=?4 AND released_at IS NULL",
        params![completed_at, task_id, p.list_id, p.project_root],
    )?;
    Ok(())
}

fn resolve_task_refs(
    refs: &[String],
    key_map: &BTreeMap<String, String>,
    snapshot_tasks: &[Task],
) -> Result<Vec<String>> {
    refs.iter()
        .map(|reference| {
            key_map
                .get(reference)
                .cloned()
                .or_else(|| {
                    resolve_task_id_from(snapshot_tasks, reference)
                        .ok()
                        .flatten()
                })
                .ok_or_else(|| PiTaskerError::InvalidReference(reference.clone()))
        })
        .collect()
}
fn load_feature_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    id: &str,
) -> Result<Option<Feature>> {
    Ok(tx.query_row("SELECT id,list_id,project_root,display_id,parent_feature_id,title,description,state,priority,tags,brief,acceptance,owner,gates,indexes,depth,created_at,updated_at FROM features WHERE id=?1 AND list_id=?2 AND project_root=?3", params![id,p.list_id,p.project_root], row_feature).optional()?)
}
fn load_feature_conn(conn: &Connection, p: &ProjectPartition, id: &str) -> Result<Option<Feature>> {
    Ok(conn.query_row("SELECT id,list_id,project_root,display_id,parent_feature_id,title,description,state,priority,tags,brief,acceptance,owner,gates,indexes,depth,created_at,updated_at FROM features WHERE id=?1 AND list_id=?2 AND project_root=?3", params![id,p.list_id,p.project_root], row_feature).optional()?)
}
fn ensure_list_meta_tx(tx: &rusqlite::Transaction<'_>, p: &ProjectPartition) -> Result<()> {
    let exists: Option<i64> = tx
        .query_row(
            "SELECT 1 FROM task_lists WHERE list_id=?1 AND project_root=?2",
            params![p.list_id, p.project_root],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        let now = now_ms();
        let name = project_name(&p.project_root);
        tx.execute("INSERT INTO task_lists (list_id, project_root, name, created_at, updated_at) VALUES (?1,?2,?3,?4,?5)", params![p.list_id,p.project_root,name,now,now])?;
    }
    Ok(())
}
fn set_dependencies_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    task_id: &str,
    depends_on: &[String],
) -> Result<()> {
    tx.execute(
        "DELETE FROM task_dependencies WHERE task_id=?1 AND list_id=?2 AND project_root=?3",
        params![task_id, p.list_id, p.project_root],
    )?;
    let now = now_ms();
    for dep in depends_on {
        let id = make_task_dependency_id(task_id, dep);
        tx.execute("INSERT INTO task_dependencies (id,task_id,depends_on,list_id,project_root,created_at) VALUES (?1,?2,?3,?4,?5,?6)", params![id,task_id,dep,p.list_id,p.project_root,now])?;
    }
    Ok(())
}

pub fn resolve_task_id_from(tasks: &[Task], input: &str) -> Result<Option<String>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.starts_with("task_") {
        return Ok(tasks.iter().find(|t| t.id == trimmed).map(|t| t.id.clone()));
    }
    let normalized = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if let Ok(n) = normalized.parse::<i64>() {
        return Ok(tasks
            .iter()
            .find(|t| t.display_id == n)
            .map(|t| t.id.clone()));
    }
    Ok(None)
}
pub fn resolve_feature_id_from(features: &[Feature], input: &str) -> Result<Option<String>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.starts_with("feat_") {
        return Ok(features
            .iter()
            .find(|f| f.id == trimmed)
            .map(|f| f.id.clone()));
    }
    let n = trimmed
        .strip_prefix('#')
        .unwrap_or(trimmed)
        .strip_prefix(['F', 'f'])
        .unwrap_or("");
    #[allow(clippy::collapsible_if)]
    if !n.is_empty() {
        if let Ok(n) = n.parse::<i64>() {
            return Ok(features
                .iter()
                .find(|f| f.display_id == n)
                .map(|f| f.id.clone()));
        }
    }
    Ok(None)
}

pub fn compute_ready_tasks(tasks: Vec<Task>, deps: Vec<TaskDependency>) -> Vec<Task> {
    let done: BTreeSet<_> = tasks
        .iter()
        .filter(|t| t.state == "done")
        .map(|t| t.id.clone())
        .collect();
    let mut by_task: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for dep in deps {
        by_task
            .entry(dep.task_id)
            .or_default()
            .push(dep.depends_on_id);
    }
    tasks
        .into_iter()
        .filter(|t| {
            t.state != "done"
                && t.state != "blocked"
                && by_task
                    .get(&t.id)
                    .is_none_or(|ds| ds.iter().all(|d| done.contains(d)))
        })
        .collect()
}

pub fn find_root_feature_id(features: &[Feature], feature_id: &str) -> Option<String> {
    let by_id: BTreeMap<_, _> = features
        .iter()
        .map(|feature| (feature.id.as_str(), feature))
        .collect();
    let mut current = by_id.get(feature_id)?;
    let mut seen = BTreeSet::new();
    while let Some(parent_id) = current.parent_feature_id.as_deref() {
        if !seen.insert(current.id.as_str()) {
            return None;
        }
        current = by_id.get(parent_id)?;
    }
    Some(current.id.clone())
}

pub fn collect_feature_subtree_ids(
    features: &[Feature],
    root_feature_id: &str,
) -> BTreeSet<String> {
    let mut children: BTreeMap<&str, Vec<&Feature>> = BTreeMap::new();
    for feature in features {
        if let Some(parent_id) = feature.parent_feature_id.as_deref() {
            children.entry(parent_id).or_default().push(feature);
        }
    }
    let mut out = BTreeSet::new();
    let mut queue = vec![root_feature_id.to_owned()];
    while let Some(id) = queue.pop() {
        if !out.insert(id.clone()) {
            continue;
        }
        if let Some(kids) = children.get(id.as_str()) {
            queue.extend(kids.iter().map(|feature| feature.id.clone()));
        }
    }
    out
}

pub fn resolve_task_scope_feature_id(task: &Task, features: &[Feature]) -> Option<String> {
    task.feature_id
        .as_deref()
        .and_then(|feature_id| find_root_feature_id(features, feature_id))
}

const REQUIRED: &[(&str, &[&str])] = &[
    (
        "task_lists",
        &[
            "list_id",
            "project_root",
            "name",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "tasks",
        &[
            "id",
            "list_id",
            "project_root",
            "display_id",
            "feature_id",
            "title",
            "description",
            "state",
            "indexes",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "task_dependencies",
        &[
            "id",
            "task_id",
            "depends_on",
            "list_id",
            "project_root",
            "created_at",
        ],
    ),
    (
        "task_notes",
        &[
            "id",
            "task_id",
            "list_id",
            "project_root",
            "category",
            "content",
            "created_at",
        ],
    ),
    (
        "features",
        &[
            "id",
            "list_id",
            "project_root",
            "display_id",
            "parent_feature_id",
            "title",
            "description",
            "state",
            "priority",
            "tags",
            "brief",
            "acceptance",
            "owner",
            "gates",
            "indexes",
            "depth",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "feature_dependencies",
        &[
            "id",
            "feature_id",
            "depends_on",
            "list_id",
            "project_root",
            "created_at",
        ],
    ),
    (
        "feature_notes",
        &[
            "id",
            "feature_id",
            "list_id",
            "project_root",
            "category",
            "content",
            "created_at",
        ],
    ),
    (
        "tasker_session_instances",
        &[
            "id",
            "list_id",
            "project_root",
            "agent_id",
            "session_id",
            "session_file",
            "pid",
            "model",
            "leaf_id_at_start",
            "current_leaf_id",
            "started_at",
            "last_seen_at",
            "ended_at",
        ],
    ),
    (
        "task_claims",
        &[
            "id",
            "task_id",
            "scope_feature_id",
            "list_id",
            "project_root",
            "agent_id",
            "session_id",
            "session_file",
            "session_instance_id",
            "pid",
            "claim_kind",
            "reason",
            "claimed_at",
            "expires_at",
            "released_at",
            "release_reason",
        ],
    ),
    (
        "work_units",
        &[
            "id",
            "task_id",
            "claim_id",
            "scope_feature_id",
            "list_id",
            "project_root",
            "agent_id",
            "session_id",
            "session_file",
            "session_instance_id",
            "status",
            "priority",
            "note",
            "created_at",
            "dispatched_at",
            "completed_at",
            "cancelled_at",
        ],
    ),
    (
        "visual_artifacts",
        &[
            "id",
            "list_id",
            "project_root",
            "task_id",
            "feature_id",
            "work_unit_id",
            "stage",
            "kind",
            "title",
            "summary",
            "path",
            "mime_type",
            "metadata",
            "created_by",
            "created_at",
            "updated_at",
        ],
    ),
];

pub fn preflight(conn: &Connection) -> Result<()> {
    for (table, cols) in REQUIRED {
        let actual = columns(conn, table)?;
        if actual.is_empty() {
            return Err(PiTaskerError::Schema(format!("missing table {table}")));
        }
        for col in *cols {
            if !actual.contains(*col) {
                return Err(PiTaskerError::Schema(format!(
                    "missing column {table}.{col}"
                )));
            }
        }
    }
    Ok(())
}
fn columns(conn: &Connection, table: &str) -> Result<BTreeSet<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    Ok(stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<BTreeSet<_>, _>>()?)
}
pub fn schema_fingerprint(conn: &Connection) -> Result<String> {
    let mut parts = Vec::new();
    for (table, _) in REQUIRED {
        let cols = columns(conn, table)?
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
        parts.push(format!("{table}:{cols}"));
    }
    let mut h = Sha1::new();
    h.update(parts.join("|").as_bytes());
    Ok(format!("{:x}", h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn temp_store() -> PiTaskerStore {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        std::mem::forget(file);
        let conn = Connection::open(&path).unwrap();
        install_schema(&conn).unwrap();
        drop(conn);
        PiTaskerStore::open(ProjectPartition::with_db_path(path, "/repo/root")).unwrap()
    }

    fn install_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(r#"
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
        "#)
    }

    fn feature_input(title: &str) -> CreateFeature {
        CreateFeature {
            title: title.into(),
            description: None,
            parent_feature_id: None,
            state: None,
            priority: None,
            tags: Value::Array(vec![]),
            brief: None,
            acceptance: Value::Array(vec![]),
            owner: None,
            gates: Value::Array(vec![]),
            indexes: Value::Array(vec![]),
        }
    }

    fn work_context(agent: &str, session: &str) -> WorkContextInput {
        WorkContextInput {
            agent_id: agent.into(),
            session_id: session.into(),
            session_instance_id: format!("sessinst_{agent}_{session}"),
            session_file: Some(format!("{session}.jsonl")),
            pid: 42,
            model: Some("test-model".into()),
            leaf_id_at_start: Some("leaf-start".into()),
            current_leaf_id: Some("leaf-current".into()),
        }
    }

    #[test]
    fn visual_artifacts_create_and_filter_like_pi() {
        let mut store = temp_store();
        let feature = store.create_feature(feature_input("Feature")).unwrap();
        let task = store
            .create_task(CreateTask {
                title: "Task".into(),
                feature_id: Some(feature.id.clone()),
                ..Default::default()
            })
            .unwrap();

        let first = store
            .create_visual_artifact(VisualArtifactCreateInput {
                task_id: Some(task.id.clone()),
                feature_id: Some(feature.id.clone()),
                work_unit_id: Some("wu_manual".into()),
                stage: Some("design".into()),
                kind: "visual-plan".into(),
                title: "Plan".into(),
                summary: "Summary".into(),
                path: "reports/plan.html".into(),
                mime_type: None,
                metadata: Some(serde_json::json!({"a": 1})),
                created_by: "agent:test".into(),
            })
            .unwrap();
        assert!(first.id.starts_with("artifact_"));
        assert_eq!(first.mime_type, "text/html");
        assert_eq!(first.metadata, serde_json::json!({"a": 1}));

        let second = store
            .create_visual_artifact(VisualArtifactCreateInput {
                task_id: None,
                feature_id: Some(feature.id.clone()),
                work_unit_id: None,
                stage: Some("validate".into()),
                kind: "evidence-pack".into(),
                title: "Evidence".into(),
                summary: "Proof".into(),
                path: "reports/evidence.html".into(),
                mime_type: Some("text/markdown".into()),
                metadata: None,
                created_by: "agent:test".into(),
            })
            .unwrap();
        assert_eq!(second.metadata, serde_json::json!({}));

        let by_task = store
            .list_visual_artifacts(VisualArtifactQueryInput {
                task_id: Some(task.id.clone()),
                limit: Some(20),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_task, vec![first.clone()]);

        let by_stage = store
            .list_visual_artifacts(VisualArtifactQueryInput {
                feature_id: Some(feature.id),
                stage: Some("validate".into()),
                kind: Some("evidence-pack".into()),
                limit: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_stage, vec![second]);
    }

    #[test]
    fn claim_task_rejects_orphan_global_and_conflicting_claims() {
        let mut store = temp_store();
        let orphan = store
            .create_task(CreateTask {
                title: "Global".into(),
                ..Default::default()
            })
            .unwrap();
        let rejected = store
            .claim_task(ClaimTaskInput {
                task_id: orphan.id,
                context: work_context("a", "s1"),
                claim_kind: None,
                reason: None,
                lease_ms: None,
                scope_feature_id: None,
            })
            .unwrap();
        assert!(!rejected.ok);
        assert_eq!(rejected.error.as_deref(), Some("missing_feature_scope"));
        assert!(rejected.claim.is_none());

        let feature = store.create_feature(feature_input("Root")).unwrap();
        let task = store
            .create_task(CreateTask {
                title: "Scoped".into(),
                feature_id: Some(feature.id.clone()),
                ..Default::default()
            })
            .unwrap();
        let first = store
            .claim_task(ClaimTaskInput {
                task_id: task.id.clone(),
                context: work_context("a", "s1"),
                claim_kind: None,
                reason: Some("mine".into()),
                lease_ms: Some(30_000),
                scope_feature_id: Some(feature.id.clone()),
            })
            .unwrap();
        assert!(first.ok);
        assert_eq!(first.scope_feature_id.as_deref(), Some(feature.id.as_str()));
        let again = store
            .claim_task(ClaimTaskInput {
                task_id: task.id.clone(),
                context: work_context("a", "s2"),
                claim_kind: None,
                reason: None,
                lease_ms: None,
                scope_feature_id: None,
            })
            .unwrap();
        assert!(again.ok);
        assert!(again.already_held);
        let conflict = store
            .claim_task(ClaimTaskInput {
                task_id: task.id,
                context: work_context("b", "other"),
                claim_kind: None,
                reason: None,
                lease_ms: None,
                scope_feature_id: None,
            })
            .unwrap();
        assert!(!conflict.ok);
        assert_eq!(conflict.error.as_deref(), Some("already_claimed"));
    }

    #[test]
    fn working_sets_are_private_and_release_cancels_owned_units() {
        let mut store = temp_store();
        let feature = store.create_feature(feature_input("Root")).unwrap();
        let task = store
            .create_task(CreateTask {
                title: "Work".into(),
                feature_id: Some(feature.id.clone()),
                ..Default::default()
            })
            .unwrap();
        let owner = work_context("agent", "session");
        let outsider = work_context("other", "else");
        let next = store
            .enqueue_next_work_unit(NextWorkUnitInput {
                context: owner.clone(),
                claim_kind: None,
                reason: None,
                lease_ms: None,
                feature_id: Some(feature.id),
                priority: Some(7),
                set_active: None,
            })
            .unwrap();
        assert!(next.ok);
        assert_eq!(next.task.as_ref().unwrap().id, task.id);
        assert_eq!(next.task.as_ref().unwrap().state, "in_progress");
        assert_eq!(
            store.get_working_set(owner.clone()).unwrap().claims.len(),
            1
        );
        assert_eq!(
            store
                .get_working_set(owner.clone())
                .unwrap()
                .work_units
                .len(),
            1
        );
        assert!(
            store
                .get_working_set(outsider.clone())
                .unwrap()
                .claims
                .is_empty()
        );
        assert!(
            store
                .get_working_set(outsider)
                .unwrap()
                .work_units
                .is_empty()
        );
        let release = store
            .release_claim(ReleaseClaimInput {
                context: owner.clone(),
                task_id: None,
                claim_id: next.claim.as_ref().map(|claim| claim.id.clone()),
                release_all: false,
                reason: None,
            })
            .unwrap();
        assert_eq!(release.count, 1);
        assert!(store.get_working_set(owner).unwrap().claims.is_empty());
        let status: String = store
            .conn
            .query_row(
                "SELECT status FROM work_units WHERE id=?1",
                params![next.work_unit.unwrap().id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "cancelled");
    }

    #[test]
    fn next_work_unit_can_leave_a_todo_task_inactive() {
        let mut store = temp_store();
        let feature = store.create_feature(feature_input("Root")).unwrap();
        let task = store
            .create_task(CreateTask {
                title: "Queued only".into(),
                feature_id: Some(feature.id.clone()),
                ..Default::default()
            })
            .unwrap();
        let next = store
            .enqueue_next_work_unit(NextWorkUnitInput {
                context: work_context("agent", "session"),
                claim_kind: None,
                reason: None,
                lease_ms: None,
                feature_id: Some(feature.id),
                priority: None,
                set_active: Some(false),
            })
            .unwrap();
        assert!(next.ok);
        assert_eq!(next.task.as_ref().unwrap().state, "todo");
        assert_eq!(
            store
                .list_tasks(None)
                .unwrap()
                .into_iter()
                .find(|candidate| candidate.id == task.id)
                .unwrap()
                .state,
            "todo"
        );
    }

    #[test]
    fn task_done_completes_work_units_and_releases_claims() {
        let mut store = temp_store();
        let feature = store.create_feature(feature_input("Root")).unwrap();
        let task = store
            .create_task(CreateTask {
                title: "Finish".into(),
                feature_id: Some(feature.id.clone()),
                ..Default::default()
            })
            .unwrap();
        let next = store
            .enqueue_next_work_unit(NextWorkUnitInput {
                context: work_context("agent", "session"),
                claim_kind: None,
                reason: None,
                lease_ms: None,
                feature_id: Some(feature.id),
                priority: None,
                set_active: None,
            })
            .unwrap();
        store
            .update_task(
                &task.id,
                UpdateTask {
                    state: Some("done".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let (work_status, completed_at): (String, Option<i64>) = store
            .conn
            .query_row(
                "SELECT status,completed_at FROM work_units WHERE id=?1",
                params![next.work_unit.unwrap().id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(work_status, "done");
        assert!(completed_at.is_some());
        let (released_at, reason): (Option<i64>, Option<String>) = store
            .conn
            .query_row(
                "SELECT released_at,release_reason FROM task_claims WHERE id=?1",
                params![next.claim.unwrap().id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(released_at.is_some());
        assert_eq!(reason.as_deref(), Some("task_done"));
    }

    #[test]
    fn derives_pi_compatible_ids() {
        assert_eq!(make_task_list_id("abc"), "list_abc");
        assert_eq!(derive_list_id("/repo/root"), "list_00e8735ec9");
        let task = make_task_id();
        assert!(task.starts_with("task_"));
        assert_eq!(task.len(), 17);
        assert_eq!(
            make_task_dependency_id("task_a", "task_b"),
            "dep_task_a_task_b"
        );
    }

    #[test]
    fn preflight_fingerprints_required_schema() {
        let store = temp_store();
        assert_eq!(store.schema_fingerprint().unwrap().len(), 40);
    }

    #[test]
    fn first_transactional_write_uses_pi_project_name() {
        let mut store = temp_store();
        store
            .create_feature(feature_input("Epic"))
            .expect("create feature");
        assert_eq!(store.list_meta().unwrap().unwrap().name, "root");
    }

    #[test]
    fn creates_updates_snapshots_and_preserves_json() {
        let mut store = temp_store();
        assert_eq!(store.ensure_list_meta().unwrap().name, "root");
        let f = store
            .create_feature(CreateFeature {
                tags: serde_json::json!(["ui"]),
                acceptance: serde_json::json!([{"criterion":"works","met":false}]),
                ..feature_input("Epic")
            })
            .unwrap();
        assert!(f.id.starts_with("feat_"));
        let t1 = store
            .create_task(CreateTask {
                title: "Setup".into(),
                state: Some("done".into()),
                indexes: Some(serde_json::json!([{"type":"file","path":"Cargo.toml"}])),
                ..Default::default()
            })
            .unwrap();
        let t2 = store
            .create_task(CreateTask {
                title: "Build".into(),
                feature_id: Some(f.id.clone()),
                depends_on: vec![t1.id.clone()],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            store
                .ready_tasks()
                .unwrap()
                .iter()
                .map(|t| t.id.clone())
                .collect::<Vec<_>>(),
            vec![t2.id.clone()]
        );
        assert_eq!(store.resolve_task_id("#2").unwrap(), Some(t2.id.clone()));
        assert_eq!(store.resolve_feature_id("F1").unwrap(), Some(f.id.clone()));
        let updated = store
            .update_task(
                &t2.id,
                UpdateTask {
                    state: Some("in_progress".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(updated.state, "in_progress");
        store
            .conn
            .execute(
                "INSERT INTO task_claims (id,task_id,list_id,project_root,agent_id,session_id,session_instance_id,pid,claim_kind,claimed_at) VALUES ('claim_test',?1,?2,?3,'agent','session','instance',1,'claim',1)",
                params![t2.id, store.partition.list_id, store.partition.project_root],
            )
            .unwrap();
        store
            .conn
            .execute(
                "INSERT INTO work_units (id,task_id,list_id,project_root,agent_id,session_id,session_instance_id,status,created_at) VALUES ('wu_test',?1,?2,?3,'agent','session','instance','active',1)",
                params![t2.id, store.partition.list_id, store.partition.project_root],
            )
            .unwrap();
        store
            .update_task(
                &t2.id,
                UpdateTask {
                    state: Some("done".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let claim_release: (Option<i64>, Option<String>) = store
            .conn
            .query_row(
                "SELECT released_at,release_reason FROM task_claims WHERE id='claim_test'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(claim_release.0.is_some());
        assert_eq!(claim_release.1.as_deref(), Some("task_done"));
        let work_unit: (String, Option<i64>) = store
            .conn
            .query_row(
                "SELECT status,completed_at FROM work_units WHERE id='wu_test'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(work_unit.0, "done");
        assert!(work_unit.1.is_some());
        let snap = store.snapshot().unwrap();
        assert_eq!(snap.tasks.len(), 2);
        assert_eq!(snap.features[0].tags, serde_json::json!(["ui"]));
        assert_eq!(
            snap.tasks[0].indexes,
            serde_json::json!([{"type":"file","path":"Cargo.toml"}])
        );
    }

    #[test]
    fn dependencies_links_notes_and_search_work_transactionally() {
        let mut store = temp_store();
        let f1 = store.create_feature(feature_input("Parent")).unwrap();
        let f2 = store
            .create_feature(CreateFeature {
                parent_feature_id: Some(f1.id.clone()),
                ..feature_input("Child")
            })
            .unwrap();
        assert_eq!(f2.depth, 1);
        let t1 = store
            .create_task(CreateTask {
                title: "Alpha".into(),
                ..Default::default()
            })
            .unwrap();
        let t2 = store
            .create_task(CreateTask {
                title: "Beta needle".into(),
                ..Default::default()
            })
            .unwrap();
        store
            .set_dependencies(&t2.id, std::slice::from_ref(&t1.id))
            .unwrap();
        assert_eq!(
            store.list_dependencies().unwrap()[0].id,
            make_task_dependency_id(&t2.id, &t1.id)
        );
        store.link_task(&t2.id, &f2.id).unwrap();
        assert_eq!(store.resolve_task_id("2").unwrap(), Some(t2.id.clone()));
        assert_eq!(store.search_tasks("needle", None).unwrap()[0].id, t2.id);
        let n = store
            .append_task_note(&t1.id, Some("context"), "note body")
            .unwrap();
        assert!(n.id.starts_with("note_"));
        let fnote = store
            .append_feature_note(&f1.id, Some("decision"), "feature note")
            .unwrap();
        assert!(fnote.id.starts_with("fnote_"));
        store.unlink_task(&t2.id).unwrap();
        assert!(
            store
                .list_tasks(None)
                .unwrap()
                .into_iter()
                .find(|t| t.id == t2.id)
                .unwrap()
                .feature_id
                .is_none()
        );
        store
            .set_feature_dependencies(&f2.id, std::slice::from_ref(&f1.id))
            .unwrap();
        assert_eq!(
            store.list_feature_dependencies().unwrap()[0].id,
            make_feature_dependency_id(&f2.id, &f1.id)
        );
    }

    #[test]
    fn batch_execute_creates_updates_dependencies_and_done_side_effects() {
        let mut store = temp_store();
        let existing = store
            .create_task(CreateTask {
                title: "Existing".into(),
                ..Default::default()
            })
            .unwrap();
        store
            .conn
            .execute(
                "INSERT INTO task_claims (id,task_id,list_id,project_root,agent_id,session_id,session_instance_id,pid,claim_kind,claimed_at) VALUES ('claim_batch',?1,?2,?3,'agent','session','instance',1,'claim',1)",
                params![existing.id, store.partition.list_id, store.partition.project_root],
            )
            .unwrap();
        store
            .conn
            .execute(
                "INSERT INTO work_units (id,task_id,list_id,project_root,agent_id,session_id,session_instance_id,status,created_at) VALUES ('wu_batch',?1,?2,?3,'agent','session','instance','active',1)",
                params![existing.id, store.partition.list_id, store.partition.project_root],
            )
            .unwrap();

        let result = store
            .batch_execute(vec![
                BatchOperation::Create {
                    key: Some("a".into()),
                    title: "A".into(),
                    description: Some("desc".into()),
                    state: None,
                    depends_on: vec![],
                    notes: vec![NoteInput {
                        content: "note".into(),
                        category: Some("context".into()),
                    }],
                    indexes: Some(serde_json::json!([{ "type": "file", "path": "a.rs" }])),
                },
                BatchOperation::Create {
                    key: Some("b".into()),
                    title: "B".into(),
                    description: None,
                    state: None,
                    depends_on: vec!["a".into(), "#1".into()],
                    notes: vec![],
                    indexes: None,
                },
                BatchOperation::Update {
                    task_id: "#1".into(),
                    title: Some("Existing done".into()),
                    description: None,
                    state: Some("done".into()),
                    depends_on: vec!["a".into()],
                    clear_dependencies: false,
                    active: None,
                },
            ])
            .unwrap();

        assert_eq!(result.created, 2);
        assert_eq!(result.updated, 1);
        assert!(result.key_map.contains_key("a"));
        let deps = store.list_dependencies().unwrap();
        assert!(
            deps.iter().any(|dep| dep.task_id == result.key_map["b"]
                && dep.depends_on_id == result.key_map["a"])
        );
        assert!(
            deps.iter()
                .any(|dep| dep.task_id == result.key_map["b"] && dep.depends_on_id == existing.id)
        );
        assert_eq!(store.list_task_notes().unwrap().len(), 1);
        let claim_release: (Option<i64>, Option<String>) = store
            .conn
            .query_row(
                "SELECT released_at,release_reason FROM task_claims WHERE id='claim_batch'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(claim_release.0.is_some());
        assert_eq!(claim_release.1.as_deref(), Some("task_done"));
        let work_unit: (String, Option<i64>) = store
            .conn
            .query_row(
                "SELECT status,completed_at FROM work_units WHERE id='wu_batch'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(work_unit.0, "done");
        assert!(work_unit.1.is_some());
    }

    #[test]
    fn batch_execute_rolls_back_on_invalid_reference() {
        let mut store = temp_store();
        let err = store
            .batch_execute(vec![BatchOperation::Create {
                key: Some("a".into()),
                title: "A".into(),
                description: None,
                state: None,
                depends_on: vec!["missing".into()],
                notes: vec![NoteInput {
                    content: "should rollback".into(),
                    category: None,
                }],
                indexes: None,
            }])
            .unwrap_err();
        assert!(matches!(err, PiTaskerError::InvalidReference(_)));
        assert!(store.list_tasks(None).unwrap().is_empty());
        assert!(store.list_task_notes().unwrap().is_empty());
        assert!(store.list_dependencies().unwrap().is_empty());
    }

    #[test]
    fn plan_import_creates_all_tasks_then_wires_dependencies() {
        let mut store = temp_store();
        let result = store
            .plan_import(vec![
                PlanTask {
                    key: "build".into(),
                    title: "Build".into(),
                    description: None,
                    state: None,
                    after: vec![],
                    notes: vec![],
                    indexes: None,
                },
                PlanTask {
                    key: "test".into(),
                    title: "Test".into(),
                    description: Some("after build".into()),
                    state: Some("todo".into()),
                    after: vec!["build".into()],
                    notes: vec![NoteInput {
                        content: "remember".into(),
                        category: Some("context".into()),
                    }],
                    indexes: Some(serde_json::json!([{ "type": "glob", "path": "tests/**" }])),
                },
            ])
            .unwrap();
        assert_eq!(result.task_count, 2);
        assert_eq!(result.dependency_count, 1);
        assert_eq!(result.dependencies[0].task_id, result.key_map["test"]);
        assert_eq!(
            result.dependencies[0].depends_on_id,
            result.key_map["build"]
        );
        assert_eq!(store.list_task_notes().unwrap().len(), 1);
    }

    #[test]
    fn plan_import_rejects_unknown_key_without_creating_tasks() {
        let mut store = temp_store();
        let err = store
            .plan_import(vec![PlanTask {
                key: "test".into(),
                title: "Test".into(),
                description: None,
                state: None,
                after: vec!["missing".into()],
                notes: vec![],
                indexes: None,
            }])
            .unwrap_err();
        assert!(matches!(err, PiTaskerError::InvalidReference(_)));
        assert!(store.list_tasks(None).unwrap().is_empty());
    }

    fn feature_plan_fixture() -> FeaturePlanInput {
        FeaturePlanInput {
            feature: FeaturePlanFeature {
                key: "root".into(),
                title: "Root feature".into(),
                description: Some("Root description".into()),
                priority: Some("high".into()),
                tags: vec!["root-tag".into()],
                brief: Some("Brief".into()),
                acceptance: vec![FeaturePlanAcceptance {
                    criterion: "Ship it".into(),
                    met: true,
                }],
                owner: Some("agent".into()),
                gates: vec![serde_json::json!({"label":"Review","status":"done"})],
                indexes: vec![serde_json::json!({"type":"file","path":"README.md"})],
                notes: vec![NoteInput {
                    content: "Root note".into(),
                    category: Some("context".into()),
                }],
                children: vec![FeaturePlanChild {
                    key: "child".into(),
                    title: "Child feature".into(),
                    description: None,
                    priority: None,
                    tags: vec!["child-tag".into()],
                    gates: vec![serde_json::json!({"label":"Child gate"})],
                    children: vec![FeaturePlanChild {
                        key: "grandchild".into(),
                        title: "Grandchild feature".into(),
                        description: None,
                        priority: None,
                        tags: vec![],
                        gates: vec![],
                        children: vec![],
                        tasks: vec![FeaturePlanTask {
                            key: "setup".into(),
                            title: "Setup".into(),
                            description: None,
                            state: Some("done".into()),
                            after: vec![],
                            notes: vec![NoteInput {
                                content: "Task note".into(),
                                category: None,
                            }],
                            indexes: None,
                        }],
                    }],
                    tasks: vec![FeaturePlanTask {
                        key: "build".into(),
                        title: "Build".into(),
                        description: None,
                        state: None,
                        after: vec!["setup".into()],
                        notes: vec![],
                        indexes: Some(serde_json::json!([{"type":"file","path":"src/lib.rs"}])),
                    }],
                }],
                tasks: vec![FeaturePlanTask {
                    key: "finish".into(),
                    title: "Finish".into(),
                    description: None,
                    state: None,
                    after: vec!["setup".into(), "build".into()],
                    notes: vec![],
                    indexes: None,
                }],
            },
        }
    }

    #[test]
    fn feature_plan_import_creates_depth_first_tree_tasks_notes_and_dependencies() {
        let mut store = temp_store();
        let result = store.feature_plan_import(feature_plan_fixture()).unwrap();

        assert_eq!(result.feature_count, 3);
        assert_eq!(result.task_count, 3);
        assert_eq!(result.dependency_count, 3);
        assert_eq!(result.feature.title, "Root feature");
        assert_eq!(result.child_features[0].title, "Child feature");
        assert_eq!(result.child_features[1].title, "Grandchild feature");
        assert_eq!(
            result
                .tasks
                .iter()
                .map(|task| task.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Setup", "Build", "Finish"]
        );
        assert!(result.key_map["root"].starts_with("feat_"));
        assert!(result.key_map["setup"].starts_with("task_"));

        let snapshot = store.snapshot().unwrap();
        assert_eq!(snapshot.features.len(), 3);
        assert_eq!(snapshot.tasks.len(), 3);
        assert_eq!(snapshot.dependencies.len(), 3);
        assert_eq!(snapshot.task_notes.len(), 1);
        assert_eq!(snapshot.feature_notes.len(), 1);
        assert_eq!(
            snapshot.features[0].acceptance,
            serde_json::json!([{"criterion":"Ship it","met":false}])
        );
        assert_eq!(
            snapshot.features[0].gates,
            serde_json::json!([{"label":"Review","status":"pending"}])
        );
        assert_eq!(
            snapshot.features[1].gates,
            serde_json::json!([{"label":"Child gate","status":"pending"}])
        );
        assert_eq!(snapshot.features[2].depth, 2);
        assert_eq!(
            snapshot.tasks[1].feature_id,
            Some(snapshot.features[1].id.clone())
        );
    }

    #[test]
    fn feature_plan_import_rejects_duplicate_global_keys() {
        let mut store = temp_store();
        let mut plan = feature_plan_fixture();
        plan.feature.children[0].tasks[0].key = "setup".into();
        let err = store.feature_plan_import(plan).unwrap_err();
        assert!(
            matches!(err, PiTaskerError::InvalidReference(message) if message.contains("duplicate plan key: setup"))
        );
        assert!(store.snapshot().unwrap().features.is_empty());
    }

    #[test]
    fn feature_plan_import_rejects_invalid_dependency_reference() {
        let mut store = temp_store();
        let mut plan = feature_plan_fixture();
        plan.feature.tasks[0].after.push("missing".into());
        let err = store.feature_plan_import(plan).unwrap_err();
        assert!(matches!(err, PiTaskerError::InvalidReference(key) if key == "missing"));
        assert!(store.snapshot().unwrap().tasks.is_empty());
    }

    #[test]
    fn feature_plan_import_rejects_feature_dependency_targets() {
        let mut store = temp_store();
        let mut plan = feature_plan_fixture();
        plan.feature.tasks[0].after.push("root".into());
        let err = store.feature_plan_import(plan).unwrap_err();
        assert!(
            matches!(err, PiTaskerError::InvalidReference(message) if message.contains("feature key cannot be a task dependency: root"))
        );
        assert!(store.snapshot().unwrap().features.is_empty());
    }

    #[test]
    fn feature_plan_import_rolls_back_when_late_dependency_wiring_fails() {
        let mut store = temp_store();
        let mut plan = feature_plan_fixture();
        plan.feature.children[0].tasks[0].after = vec!["root".into()];
        assert!(validate_feature_plan_keys(&plan.feature).is_err());

        let tx = store.conn.transaction().unwrap();
        ensure_list_meta_tx(&tx, &store.partition).unwrap();
        let mut key_map = BTreeMap::new();
        let mut features = Vec::new();
        let mut tasks = Vec::new();
        let mut dependency_count = 0;
        let err = create_feature_plan_node_tx(
            &tx,
            &store.partition,
            FeaturePlanNode::Root(&plan.feature),
            None,
            0,
            &mut key_map,
            &mut features,
            &mut tasks,
            &mut dependency_count,
        )
        .unwrap_err();
        assert!(
            matches!(err, PiTaskerError::InvalidReference(message) if message.contains("feature key cannot be a task dependency: root"))
        );
        drop(tx);

        let snapshot = store.snapshot().unwrap();
        assert!(snapshot.features.is_empty());
        assert!(snapshot.tasks.is_empty());
        assert!(snapshot.dependencies.is_empty());
    }

    #[test]
    fn feature_gates_round_trip_typed_resolvers_and_legacy_manual_gates() {
        let mut store = temp_store();
        let mut input = feature_input("gated");
        input.gates = serde_json::json!([
            {"type":"legacy-review"},
            {"label":"unit tests","resolver":{"_tag":"command","run":"cargo test","cwd":"crates/x","timeout":123,"env":{"RUST_LOG":"debug"}},"status":"pending"},
            {"label":"script","resolver":{"_tag":"script","path":"scripts/check.sh","timeout":456},"status":"pending"},
            {"label":"tool","resolver":{"_tag":"tool","tool":"tasker","args":{"action":"status"}},"status":"passed"},
            {"label":"agent","resolver":{"_tag":"agent","agent":"reviewer","task":"review","model":"sonnet"},"status":"pending"}
        ]);
        let feature = store.create_feature(input).unwrap();

        let gates = store.feature_gates(&feature.id).unwrap();
        assert_eq!(gates[0].label, "legacy-review");
        assert_eq!(gates[0].resolver, None);
        assert_eq!(gates[0].status, "pending");
        assert!(matches!(
            gates[1].resolver,
            Some(GateResolver::Command { .. })
        ));
        assert!(matches!(
            gates[2].resolver,
            Some(GateResolver::Script { .. })
        ));
        assert!(matches!(gates[3].resolver, Some(GateResolver::Tool { .. })));
        assert!(matches!(
            gates[4].resolver,
            Some(GateResolver::Agent { .. })
        ));
        assert_eq!(
            store.pending_executable_gate_indexes(&feature.id).unwrap(),
            vec![1, 2, 4]
        );
    }

    #[test]
    fn resolve_feature_gate_preserves_resolver_and_updates_json_atomically() {
        let mut store = temp_store();
        let mut input = feature_input("manual gates");
        input.gates = serde_json::json!([
            {"label":"review","resolver":null,"status":"pending"},
            {"label":"tests","resolver":{"_tag":"command","run":"true"},"status":"pending"}
        ]);
        let feature = store.create_feature(input).unwrap();

        let gates = store
            .resolve_feature_gate(
                &feature.id,
                0,
                ResolveFeatureGate {
                    status: "passed".into(),
                    resolved_by: Some("human".into()),
                    note: Some("looks good".into()),
                },
            )
            .unwrap();
        assert_eq!(gates[0].status, "passed");
        assert_eq!(gates[0].resolved_by.as_deref(), Some("human"));
        assert!(gates[0].resolved_at.is_some());
        assert_eq!(gates[0].note.as_deref(), Some("looks good"));
        assert!(matches!(
            store.feature_gate(&feature.id, 1).unwrap().resolver,
            Some(GateResolver::Command { .. })
        ));

        let before = store.feature_gates(&feature.id).unwrap();
        let err = store
            .resolve_feature_gate(
                &feature.id,
                99,
                ResolveFeatureGate {
                    status: "failed".into(),
                    resolved_by: None,
                    note: None,
                },
            )
            .unwrap_err();
        assert!(matches!(err, PiTaskerError::InvalidReference(_)));
        assert_eq!(store.feature_gates(&feature.id).unwrap(), before);
    }

    #[test]
    fn apply_feature_gate_check_rejects_manual_and_resolved_gates_without_side_effects() {
        let mut store = temp_store();
        let mut input = feature_input("check guards");
        input.gates = serde_json::json!([
            {"label":"manual","resolver":null,"status":"pending"},
            {"label":"done","resolver":{"_tag":"command","run":"true"},"status":"passed"}
        ]);
        let feature = store.create_feature(input).unwrap();
        let check = FeatureGateCheckResult {
            status: "passed".into(),
            note: "ok".into(),
            full_log: "log".into(),
            exit_code: 0,
            duration_ms: 1,
        };

        assert!(matches!(
            store.apply_feature_gate_check(&feature.id, 0, check.clone()),
            Err(PiTaskerError::InvalidReference(_))
        ));
        assert!(matches!(
            store.apply_feature_gate_check(&feature.id, 1, check),
            Err(PiTaskerError::InvalidReference(_))
        ));
        assert!(store.list_feature_notes().unwrap().is_empty());
        assert_eq!(
            store.feature_gates(&feature.id).unwrap()[0].status,
            "pending"
        );
    }

    #[test]
    fn apply_feature_gate_checks_caps_summary_archives_evidence_and_honors_modes() {
        let mut store = temp_store();
        let mut input = feature_input("check all");
        input.gates = serde_json::json!([
            {"label":"first","resolver":{"_tag":"command","run":"true"},"status":"pending"},
            {"label":"second","resolver":{"_tag":"script","path":"safe.sh"},"status":"pending"},
            {"label":"third","resolver":{"_tag":"agent","agent":"a","task":"t"},"status":"pending"}
        ]);
        let feature = store.create_feature(input).unwrap();
        let long_note = "n".repeat(2_500);
        let long_log = "l".repeat(12_000);

        let applied = store
            .apply_feature_gate_checks(
                &feature.id,
                vec![
                    (
                        0,
                        FeatureGateCheckResult {
                            status: "passed".into(),
                            note: long_note,
                            full_log: long_log,
                            exit_code: 0,
                            duration_ms: 7,
                        },
                    ),
                    (
                        1,
                        FeatureGateCheckResult {
                            status: "failed".into(),
                            note: "bad".into(),
                            full_log: "failure".into(),
                            exit_code: 1,
                            duration_ms: 8,
                        },
                    ),
                    (
                        2,
                        FeatureGateCheckResult {
                            status: "passed".into(),
                            note: "would be skipped".into(),
                            full_log: "skip".into(),
                            exit_code: 0,
                            duration_ms: 9,
                        },
                    ),
                ],
                FeatureGateCheckMode::FailFast,
            )
            .unwrap();

        assert_eq!(applied.len(), 2);
        let gates = store.feature_gates(&feature.id).unwrap();
        assert_eq!(gates[0].note.as_ref().unwrap().chars().count(), 2_000);
        assert_eq!(gates[0].resolved_by.as_deref(), Some("resolver:command"));
        assert_eq!(gates[1].status, "failed");
        assert_eq!(gates[2].status, "pending");
        let notes = store.list_feature_notes().unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].category.as_deref(), Some("ref"));
        assert!(
            notes[0]
                .content
                .starts_with("[gate:0] first\n\nExit: 0 | Duration: 7ms")
        );
        assert_eq!(notes[0].content.chars().count(), 10_000);

        let applied = store
            .apply_feature_gate_checks(
                &feature.id,
                vec![(
                    2,
                    FeatureGateCheckResult {
                        status: "passed".into(),
                        note: "ok".into(),
                        full_log: String::new(),
                        exit_code: 0,
                        duration_ms: 3,
                    },
                )],
                FeatureGateCheckMode::CheckAll,
            )
            .unwrap();
        assert_eq!(applied.len(), 1);
        assert!(applied[0].evidence_note.is_none());
        assert_eq!(
            store.feature_gates(&feature.id).unwrap()[2].status,
            "passed"
        );
    }
}
