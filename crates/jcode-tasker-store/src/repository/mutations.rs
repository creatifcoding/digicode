use std::str::FromStr;

use chrono::Utc;
use jcode_tasker_types::{
    AddFeatureDependency, AddTaskDependency, ChangeKind, ChangeSummary, CreateFeature,
    CreateProject, CreateTask, Feature, FeatureAlias, FeatureDependency, FeatureId, FeatureState,
    OutboxEventId, Project, ProjectEvent, ProjectId, ProjectRevision, SetFeatureState,
    SetTaskState, Task, TaskDependency, TaskId, TaskState, TaskerError,
};
use tokio_rusqlite::rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use super::{MutationResult, TaskerStore, rows};
use crate::{StoreError, StoreResult};

impl TaskerStore {
    pub async fn create_project(
        &self,
        command: CreateProject,
    ) -> StoreResult<MutationResult<Project>> {
        self.call(move |connection| {
            let name = required_text("name", command.name, 200)?;
            let canonical_root = optional_text(command.canonical_root);
            let project_id = command.id.unwrap_or_default();
            let now = Utc::now();
            let timestamp = now.to_rfc3339();
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let existing: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
                [project_id.to_string()],
                |row| row.get(0),
            )?;
            if existing {
                return Err(TaskerError::Conflict {
                    message: format!("project already exists: {project_id}"),
                }
                .into());
            }
            transaction.execute(
                "INSERT INTO projects (id, name, canonical_root, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![project_id.to_string(), name, canonical_root, timestamp],
            )?;
            transaction.execute(
                "INSERT INTO project_revisions (project_id, revision, updated_at)
                 VALUES (?1, 0, ?2)",
                params![project_id.to_string(), timestamp],
            )?;
            if let Some(root) = canonical_root.as_deref() {
                transaction.execute(
                    "INSERT INTO project_roots (project_id, root, is_canonical, created_at)
                     VALUES (?1, ?2, 1, ?3)",
                    params![project_id.to_string(), root, timestamp],
                )?;
            }
            let change = ChangeSummary {
                kind: ChangeKind::ProjectCreated,
                feature_id: None,
                task_id: None,
                description: format!("created project {project_id}"),
            };
            let (revision, event_id) =
                finish_mutation(&transaction, project_id, change, &timestamp)?;
            transaction.commit()?;
            Ok(MutationResult {
                value: Project {
                    id: project_id,
                    name,
                    canonical_root,
                    revision,
                    created_at: now,
                    updated_at: now,
                },
                revision,
                event_id,
            })
        })
        .await
    }

    pub async fn create_feature(
        &self,
        command: CreateFeature,
    ) -> StoreResult<MutationResult<Feature>> {
        self.call(move |connection| {
            let title = required_text("title", command.title, 500)?;
            let feature_id = command.id.unwrap_or_default();
            let now = Utc::now();
            let timestamp = now.to_rfc3339();
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            check_revision(&transaction, command.project_id, command.expected_revision)?;
            if let Some(parent_id) = command.parent_id {
                ensure_feature(&transaction, command.project_id, parent_id)?;
            }
            ensure_new_id(&transaction, "features", &feature_id.to_string(), "feature")?;
            let alias = allocate_alias(&transaction, command.project_id, "next_feature_alias")?;
            transaction.execute(
                "INSERT INTO features
                    (id, project_id, alias, parent_id, title, description, state, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open', ?7, ?7)",
                params![
                    feature_id.to_string(),
                    command.project_id.to_string(),
                    alias,
                    command.parent_id.map(|id| id.to_string()),
                    title,
                    command.description,
                    timestamp,
                ],
            )?;
            let change = ChangeSummary {
                kind: ChangeKind::FeatureCreated,
                feature_id: Some(feature_id),
                task_id: None,
                description: format!("created feature {feature_id}"),
            };
            let (revision, event_id) =
                finish_mutation(&transaction, command.project_id, change, &timestamp)?;
            transaction.commit()?;
            Ok(MutationResult {
                value: Feature {
                    id: feature_id,
                    project_id: command.project_id,
                    alias: FeatureAlias(alias_to_u64(alias, "feature alias")?),
                    parent_id: command.parent_id,
                    title,
                    description: command.description,
                    state: FeatureState::Open,
                    created_at: now,
                    updated_at: now,
                },
                revision,
                event_id,
            })
        })
        .await
    }

    pub async fn create_task(&self, command: CreateTask) -> StoreResult<MutationResult<Task>> {
        self.call(move |connection| {
            let title = required_text("title", command.title, 500)?;
            let task_id = command.id.unwrap_or_default();
            let now = Utc::now();
            let timestamp = now.to_rfc3339();
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            check_revision(&transaction, command.project_id, command.expected_revision)?;
            ensure_feature(&transaction, command.project_id, command.feature_id)?;
            ensure_new_id(&transaction, "tasks", &task_id.to_string(), "task")?;
            let alias = allocate_alias(&transaction, command.project_id, "next_task_alias")?;
            transaction.execute(
                "INSERT INTO tasks
                    (id, project_id, feature_id, alias, title, description, state, priority, rank,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'todo', ?7, ?8, ?9, ?9)",
                params![
                    task_id.to_string(),
                    command.project_id.to_string(),
                    command.feature_id.to_string(),
                    alias,
                    title,
                    command.description,
                    command.priority.sort_value(),
                    command.rank,
                    timestamp,
                ],
            )?;
            let change = ChangeSummary {
                kind: ChangeKind::TaskCreated,
                feature_id: Some(command.feature_id),
                task_id: Some(task_id),
                description: format!("created task {task_id}"),
            };
            let (revision, event_id) =
                finish_mutation(&transaction, command.project_id, change, &timestamp)?;
            transaction.commit()?;
            Ok(MutationResult {
                value: Task {
                    id: task_id,
                    project_id: command.project_id,
                    feature_id: command.feature_id,
                    alias: jcode_tasker_types::TaskAlias(alias_to_u64(alias, "task alias")?),
                    title,
                    description: command.description,
                    state: TaskState::Todo,
                    priority: command.priority,
                    rank: command.rank,
                    created_at: now,
                    updated_at: now,
                },
                revision,
                event_id,
            })
        })
        .await
    }

    pub async fn add_task_dependency(
        &self,
        command: AddTaskDependency,
    ) -> StoreResult<MutationResult<TaskDependency>> {
        self.call(move |connection| {
            let now = Utc::now();
            let timestamp = now.to_rfc3339();
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            check_revision(&transaction, command.project_id, command.expected_revision)?;
            ensure_task(&transaction, command.project_id, command.task_id)?;
            ensure_task(&transaction, command.project_id, command.depends_on_task_id)?;
            reject_task_cycle(&transaction, &command)?;
            ensure_new_dependency(
                &transaction,
                "task_dependencies",
                "task_id",
                "depends_on_task_id",
                command.project_id,
                command.task_id,
                command.depends_on_task_id,
            )?;
            transaction.execute(
                "INSERT INTO task_dependencies
                    (project_id, task_id, depends_on_task_id, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    command.project_id.to_string(),
                    command.task_id.to_string(),
                    command.depends_on_task_id.to_string(),
                    timestamp,
                ],
            )?;
            let change = ChangeSummary {
                kind: ChangeKind::TaskDependencyAdded,
                feature_id: None,
                task_id: Some(command.task_id),
                description: format!(
                    "task {} depends on {}",
                    command.task_id, command.depends_on_task_id
                ),
            };
            let (revision, event_id) =
                finish_mutation(&transaction, command.project_id, change, &timestamp)?;
            transaction.commit()?;
            Ok(MutationResult {
                value: TaskDependency {
                    project_id: command.project_id,
                    task_id: command.task_id,
                    depends_on_task_id: command.depends_on_task_id,
                    created_at: now,
                },
                revision,
                event_id,
            })
        })
        .await
    }

    pub async fn add_feature_dependency(
        &self,
        command: AddFeatureDependency,
    ) -> StoreResult<MutationResult<FeatureDependency>> {
        self.call(move |connection| {
            let now = Utc::now();
            let timestamp = now.to_rfc3339();
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            check_revision(&transaction, command.project_id, command.expected_revision)?;
            ensure_feature(&transaction, command.project_id, command.feature_id)?;
            ensure_feature(
                &transaction,
                command.project_id,
                command.depends_on_feature_id,
            )?;
            reject_feature_cycle(&transaction, &command)?;
            ensure_new_dependency(
                &transaction,
                "feature_dependencies",
                "feature_id",
                "depends_on_feature_id",
                command.project_id,
                command.feature_id,
                command.depends_on_feature_id,
            )?;
            transaction.execute(
                "INSERT INTO feature_dependencies
                    (project_id, feature_id, depends_on_feature_id, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    command.project_id.to_string(),
                    command.feature_id.to_string(),
                    command.depends_on_feature_id.to_string(),
                    timestamp,
                ],
            )?;
            let change = ChangeSummary {
                kind: ChangeKind::FeatureDependencyAdded,
                feature_id: Some(command.feature_id),
                task_id: None,
                description: format!(
                    "feature {} depends on {}",
                    command.feature_id, command.depends_on_feature_id
                ),
            };
            let (revision, event_id) =
                finish_mutation(&transaction, command.project_id, change, &timestamp)?;
            transaction.commit()?;
            Ok(MutationResult {
                value: FeatureDependency {
                    project_id: command.project_id,
                    feature_id: command.feature_id,
                    depends_on_feature_id: command.depends_on_feature_id,
                    created_at: now,
                },
                revision,
                event_id,
            })
        })
        .await
    }

    pub async fn set_task_state(&self, command: SetTaskState) -> StoreResult<MutationResult<Task>> {
        self.call(move |connection| {
            let timestamp = Utc::now().to_rfc3339();
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            check_revision(&transaction, command.project_id, command.expected_revision)?;
            let current = task_state(&transaction, command.project_id, command.task_id)?;
            current.transition_to(command.state)?;
            transaction.execute(
                "UPDATE tasks SET state = ?1, updated_at = ?2
                 WHERE project_id = ?3 AND id = ?4",
                params![
                    command.state.as_str(),
                    timestamp,
                    command.project_id.to_string(),
                    command.task_id.to_string(),
                ],
            )?;
            let change = ChangeSummary {
                kind: ChangeKind::TaskStateChanged,
                feature_id: None,
                task_id: Some(command.task_id),
                description: format!(
                    "task {} changed from {} to {}",
                    command.task_id,
                    current.as_str(),
                    command.state.as_str()
                ),
            };
            let (revision, event_id) =
                finish_mutation(&transaction, command.project_id, change, &timestamp)?;
            let task = task_by_id(&transaction, command.project_id, command.task_id)?;
            transaction.commit()?;
            Ok(MutationResult {
                value: task,
                revision,
                event_id,
            })
        })
        .await
    }

    pub async fn set_feature_state(
        &self,
        command: SetFeatureState,
    ) -> StoreResult<MutationResult<Feature>> {
        self.call(move |connection| {
            let timestamp = Utc::now().to_rfc3339();
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            check_revision(&transaction, command.project_id, command.expected_revision)?;
            let current = feature_state(&transaction, command.project_id, command.feature_id)?;
            current.transition_to(command.state)?;
            transaction.execute(
                "UPDATE features SET state = ?1, updated_at = ?2
                 WHERE project_id = ?3 AND id = ?4",
                params![
                    command.state.as_str(),
                    timestamp,
                    command.project_id.to_string(),
                    command.feature_id.to_string(),
                ],
            )?;
            let change = ChangeSummary {
                kind: ChangeKind::FeatureStateChanged,
                feature_id: Some(command.feature_id),
                task_id: None,
                description: format!(
                    "feature {} changed from {} to {}",
                    command.feature_id,
                    current.as_str(),
                    command.state.as_str()
                ),
            };
            let (revision, event_id) =
                finish_mutation(&transaction, command.project_id, change, &timestamp)?;
            let feature = feature_by_id(&transaction, command.project_id, command.feature_id)?;
            transaction.commit()?;
            Ok(MutationResult {
                value: feature,
                revision,
                event_id,
            })
        })
        .await
    }
}

fn required_text(field: &str, value: String, max_len: usize) -> StoreResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(TaskerError::InvalidInput {
            field: field.to_string(),
            message: "must not be empty".to_string(),
        }
        .into());
    }
    if value.chars().count() > max_len {
        return Err(TaskerError::InvalidInput {
            field: field.to_string(),
            message: format!("must not exceed {max_len} characters"),
        }
        .into());
    }
    Ok(value)
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn check_revision(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    expected: Option<ProjectRevision>,
) -> StoreResult<ProjectRevision> {
    let actual: Option<i64> = transaction
        .query_row(
            "SELECT revision FROM project_revisions WHERE project_id = ?1",
            [project_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    let actual = actual.ok_or_else(|| not_found("project", project_id))?;
    let actual = ProjectRevision(alias_to_u64(actual, "project revision")?);
    if let Some(expected) = expected.filter(|expected| *expected != actual) {
        return Err(TaskerError::RevisionConflict { expected, actual }.into());
    }
    Ok(actual)
}

fn allocate_alias(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    column: &str,
) -> StoreResult<i64> {
    let sql =
        format!("UPDATE projects SET {column} = {column} + 1 WHERE id = ?1 RETURNING {column} - 1");
    transaction
        .query_row(&sql, [project_id.to_string()], |row| row.get(0))
        .map_err(StoreError::from)
}

fn ensure_new_id(
    transaction: &Transaction<'_>,
    table: &str,
    id: &str,
    entity: &str,
) -> StoreResult<()> {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)");
    let exists: bool = transaction.query_row(&sql, [id], |row| row.get(0))?;
    if exists {
        return Err(TaskerError::Conflict {
            message: format!("{entity} already exists: {id}"),
        }
        .into());
    }
    Ok(())
}

fn ensure_feature(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    feature_id: FeatureId,
) -> StoreResult<()> {
    ensure_entity(transaction, "features", project_id, feature_id, "feature")
}

fn ensure_task(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    task_id: TaskId,
) -> StoreResult<()> {
    ensure_entity(transaction, "tasks", project_id, task_id, "task")
}

fn ensure_entity<T: std::fmt::Display>(
    transaction: &Transaction<'_>,
    table: &str,
    project_id: ProjectId,
    id: T,
    entity: &str,
) -> StoreResult<()> {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE project_id = ?1 AND id = ?2)");
    let id = id.to_string();
    let exists: bool =
        transaction.query_row(&sql, params![project_id.to_string(), id], |row| row.get(0))?;
    if !exists {
        return Err(TaskerError::NotFound {
            entity: entity.to_string(),
            id,
        }
        .into());
    }
    Ok(())
}

fn ensure_new_dependency<A: std::fmt::Display, B: std::fmt::Display>(
    transaction: &Transaction<'_>,
    table: &str,
    source_column: &str,
    target_column: &str,
    project_id: ProjectId,
    source: A,
    target: B,
) -> StoreResult<()> {
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM {table}
         WHERE project_id = ?1 AND {source_column} = ?2 AND {target_column} = ?3)"
    );
    let exists: bool = transaction.query_row(
        &sql,
        params![
            project_id.to_string(),
            source.to_string(),
            target.to_string()
        ],
        |row| row.get(0),
    )?;
    if exists {
        return Err(TaskerError::Conflict {
            message: format!("dependency already exists: {source} -> {target}"),
        }
        .into());
    }
    Ok(())
}

fn reject_task_cycle(
    transaction: &Transaction<'_>,
    command: &AddTaskDependency,
) -> StoreResult<()> {
    reject_cycle(
        transaction,
        "task_dependencies",
        "task_id",
        "depends_on_task_id",
        "task",
        command.project_id,
        command.task_id.to_string(),
        command.depends_on_task_id.to_string(),
    )
}

fn reject_feature_cycle(
    transaction: &Transaction<'_>,
    command: &AddFeatureDependency,
) -> StoreResult<()> {
    reject_cycle(
        transaction,
        "feature_dependencies",
        "feature_id",
        "depends_on_feature_id",
        "feature",
        command.project_id,
        command.feature_id.to_string(),
        command.depends_on_feature_id.to_string(),
    )
}

#[allow(clippy::too_many_arguments)]
fn reject_cycle(
    transaction: &Transaction<'_>,
    table: &str,
    source_column: &str,
    target_column: &str,
    entity: &str,
    project_id: ProjectId,
    source: String,
    target: String,
) -> StoreResult<()> {
    if source == target {
        return Err(TaskerError::DependencyCycle {
            entity: entity.to_string(),
            cycle: vec![source.clone(), source],
        }
        .into());
    }
    let sql = format!(
        "WITH RECURSIVE reach(node, path) AS (
            SELECT {target_column}, ?2 || '|' || {target_column}
            FROM {table}
            WHERE project_id = ?1 AND {source_column} = ?2
            UNION ALL
            SELECT dependency.{target_column}, reach.path || '|' || dependency.{target_column}
            FROM {table} dependency
            JOIN reach ON dependency.{source_column} = reach.node
            WHERE dependency.project_id = ?1
              AND instr('|' || reach.path || '|', '|' || dependency.{target_column} || '|') = 0
        )
        SELECT path FROM reach WHERE node = ?3 LIMIT 1"
    );
    let path: Option<String> = transaction
        .query_row(
            &sql,
            params![project_id.to_string(), target, source],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(path) = path {
        let mut cycle = vec![source];
        cycle.extend(path.split('|').map(str::to_string));
        return Err(TaskerError::DependencyCycle {
            entity: entity.to_string(),
            cycle,
        }
        .into());
    }
    Ok(())
}

fn task_state(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    task_id: TaskId,
) -> StoreResult<TaskState> {
    let state: Option<String> = transaction
        .query_row(
            "SELECT state FROM tasks WHERE project_id = ?1 AND id = ?2",
            params![project_id.to_string(), task_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    TaskState::from_str(&state.ok_or_else(|| not_found("task", task_id))?).map_err(Into::into)
}

fn feature_state(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    feature_id: FeatureId,
) -> StoreResult<FeatureState> {
    let state: Option<String> = transaction
        .query_row(
            "SELECT state FROM features WHERE project_id = ?1 AND id = ?2",
            params![project_id.to_string(), feature_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    FeatureState::from_str(&state.ok_or_else(|| not_found("feature", feature_id))?)
        .map_err(Into::into)
}

fn task_by_id(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    task_id: TaskId,
) -> StoreResult<Task> {
    let row = transaction
        .query_row(
            "SELECT id, project_id, feature_id, alias, title, description, state, priority, rank,
                    created_at, updated_at
             FROM tasks WHERE project_id = ?1 AND id = ?2",
            params![project_id.to_string(), task_id.to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| not_found("task", task_id))?;
    rows::task_from_row(row)
}

fn feature_by_id(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    feature_id: FeatureId,
) -> StoreResult<Feature> {
    let row = transaction
        .query_row(
            "SELECT id, project_id, alias, parent_id, title, description, state, created_at,
                    updated_at
             FROM features WHERE project_id = ?1 AND id = ?2",
            params![project_id.to_string(), feature_id.to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| not_found("feature", feature_id))?;
    rows::feature_from_row(row)
}

fn finish_mutation(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    change: ChangeSummary,
    timestamp: &str,
) -> StoreResult<(ProjectRevision, OutboxEventId)> {
    let revision: i64 = transaction.query_row(
        "UPDATE project_revisions
         SET revision = revision + 1, updated_at = ?1
         WHERE project_id = ?2
         RETURNING revision",
        params![timestamp, project_id.to_string()],
        |row| row.get(0),
    )?;
    transaction.execute(
        "UPDATE projects SET updated_at = ?1 WHERE id = ?2",
        params![timestamp, project_id.to_string()],
    )?;
    let revision = ProjectRevision(alias_to_u64(revision, "project revision")?);
    let event_id = OutboxEventId::new();
    let event = ProjectEvent {
        id: event_id,
        project_id,
        revision,
        change,
        created_at: chrono::DateTime::parse_from_rfc3339(timestamp)
            .map_err(|_| StoreError::InvalidTimestamp(timestamp.to_string()))?
            .with_timezone(&Utc),
    };
    transaction.execute(
        "INSERT INTO outbox_events
            (id, project_id, revision, event_type, payload_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            event_id.to_string(),
            project_id.to_string(),
            i64::try_from(revision.0).map_err(|_| TaskerError::Conflict {
                message: "project revision exceeds SQLite integer range".to_string(),
            })?,
            event.change.kind.as_str(),
            serde_json::to_string(&event)?,
            timestamp,
        ],
    )?;
    Ok((revision, event_id))
}

fn alias_to_u64(value: i64, field: &str) -> StoreResult<u64> {
    u64::try_from(value).map_err(|_| {
        TaskerError::Conflict {
            message: format!("invalid stored {field}"),
        }
        .into()
    })
}

fn not_found(entity: &str, id: impl std::fmt::Display) -> StoreError {
    TaskerError::NotFound {
        entity: entity.to_string(),
        id: id.to_string(),
    }
    .into()
}
