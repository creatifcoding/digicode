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
        let name = Path::new(&self.partition.project_root)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or(&self.partition.project_root);
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
fn json_or_empty_array(raw: Option<String>) -> Value {
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| Value::Array(vec![]))
}

fn load_task_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    id: &str,
) -> Result<Option<Task>> {
    Ok(tx.query_row("SELECT id,list_id,project_root,display_id,feature_id,title,description,state,indexes,created_at,updated_at FROM tasks WHERE id=?1 AND list_id=?2 AND project_root=?3", params![id,p.list_id,p.project_root], row_task).optional()?)
}
fn load_feature_tx(
    tx: &rusqlite::Transaction<'_>,
    p: &ProjectPartition,
    id: &str,
) -> Result<Option<Feature>> {
    Ok(tx.query_row("SELECT id,list_id,project_root,display_id,parent_feature_id,title,description,state,priority,tags,brief,acceptance,owner,gates,indexes,depth,created_at,updated_at FROM features WHERE id=?1 AND list_id=?2 AND project_root=?3", params![id,p.list_id,p.project_root], row_feature).optional()?)
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
        tx.execute("INSERT INTO task_lists (list_id, project_root, name, created_at, updated_at) VALUES (?1,?2,?3,?4,?5)", params![p.list_id,p.project_root,p.project_root,now,now])?;
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
    if !n.is_empty()
        && let Ok(n) = n.parse::<i64>()
    {
        return Ok(features
            .iter()
            .find(|f| f.display_id == n)
            .map(|f| f.id.clone()));
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
}
