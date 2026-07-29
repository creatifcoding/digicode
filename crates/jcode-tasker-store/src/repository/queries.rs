use std::str::FromStr;

use chrono::Utc;
use jcode_tasker_types::{
    DurableOutboxEvent, Feature, FeatureId, FeatureState, INITIAL_READINESS_POLICY_VERSION,
    Project, ProjectId, ProjectRevision, ProjectSnapshot, ReadinessExplanation, ReadyTask, Task,
    TaskId, TaskState, TaskerError,
};
use tokio_rusqlite::rusqlite::{Connection, OptionalExtension, params};

use super::{TaskerStore, rows};
use crate::{StoreResult, error::StoreError};

impl TaskerStore {
    pub async fn project_by_root(&self, root: impl Into<String>) -> StoreResult<Option<Project>> {
        let root = root.into();
        self.call(move |connection| {
            let row = connection
                .query_row(
                    "SELECT p.id, p.name, p.canonical_root, r.revision, p.created_at, p.updated_at
                     FROM project_roots roots
                     JOIN projects p ON p.id = roots.project_id
                     JOIN project_revisions r ON r.project_id = p.id
                     WHERE roots.root = ?1
                     ORDER BY roots.is_canonical DESC, p.created_at ASC
                     LIMIT 1",
                    [root],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .optional()?;
            row.map(rows::project_from_row).transpose()
        })
        .await
    }

    pub async fn get_project(&self, project_id: ProjectId) -> StoreResult<Project> {
        self.call(move |connection| project_by_id(connection, project_id))
            .await
    }

    pub async fn get_feature(
        &self,
        project_id: ProjectId,
        feature_id: FeatureId,
    ) -> StoreResult<Feature> {
        self.call(move |connection| feature_by_id(connection, project_id, feature_id))
            .await
    }

    pub async fn resolve_feature(
        &self,
        project_id: ProjectId,
        reference: impl Into<String>,
    ) -> StoreResult<Feature> {
        let reference = reference.into();
        self.call(move |connection| {
            if reference.starts_with("feat_") {
                return feature_by_id(connection, project_id, FeatureId::from_str(&reference)?);
            }
            let alias = reference
                .strip_prefix("#F")
                .ok_or_else(|| {
                    invalid_reference("feature", &reference, "feat_<uuid> or #F<number>")
                })?
                .parse::<u64>()
                .map_err(|_| {
                    invalid_reference("feature", &reference, "feat_<uuid> or #F<number>")
                })?;
            feature_by_alias(connection, project_id, alias)
        })
        .await
    }

    pub async fn get_task(&self, project_id: ProjectId, task_id: TaskId) -> StoreResult<Task> {
        self.call(move |connection| task_by_id(connection, project_id, task_id))
            .await
    }

    pub async fn resolve_task(
        &self,
        project_id: ProjectId,
        reference: impl Into<String>,
    ) -> StoreResult<Task> {
        let reference = reference.into();
        self.call(move |connection| {
            if reference.starts_with("task_") {
                return task_by_id(connection, project_id, TaskId::from_str(&reference)?);
            }
            let alias = reference
                .strip_prefix('#')
                .ok_or_else(|| invalid_reference("task", &reference, "task_<uuid> or #<number>"))?
                .parse::<u64>()
                .map_err(|_| invalid_reference("task", &reference, "task_<uuid> or #<number>"))?;
            task_by_alias(connection, project_id, alias)
        })
        .await
    }

    pub async fn project_revision(&self, project_id: ProjectId) -> StoreResult<ProjectRevision> {
        self.call(move |connection| revision_by_project(connection, project_id))
            .await
    }

    pub async fn task_dependencies(
        &self,
        project_id: ProjectId,
        task_id: TaskId,
    ) -> StoreResult<Vec<TaskId>> {
        self.call(move |connection| task_dependencies(connection, project_id, task_id))
            .await
    }

    pub async fn task_readiness(
        &self,
        project_id: ProjectId,
        task_id: TaskId,
    ) -> StoreResult<ReadinessExplanation> {
        self.call(move |connection| readiness_by_task(connection, project_id, task_id))
            .await
    }

    pub async fn list_ready(&self, project_id: ProjectId) -> StoreResult<Vec<ReadyTask>> {
        self.call(move |connection| ready_tasks(connection, project_id))
            .await
    }

    pub async fn snapshot(&self, project_id: ProjectId) -> StoreResult<ProjectSnapshot> {
        self.call(move |connection| {
            let project = project_by_id(connection, project_id)?;
            let revision = project.revision;
            let features = all_features(connection, project_id)?;
            let tasks = all_tasks(connection, project_id)?;
            let ready_tasks = ready_tasks(connection, project_id)?;
            Ok(ProjectSnapshot {
                project,
                revision,
                features,
                tasks,
                ready_tasks,
                policy_version: INITIAL_READINESS_POLICY_VERSION,
            })
        })
        .await
    }

    pub async fn pending_outbox(
        &self,
        project_id: Option<ProjectId>,
        limit: usize,
    ) -> StoreResult<Vec<DurableOutboxEvent>> {
        let limit = limit.clamp(1, 1_000);
        self.call(move |connection| {
            let project_id = project_id.map(|id| id.to_string());
            let mut statement = connection.prepare(
                "SELECT payload_json, dispatched_at
                 FROM outbox_events
                 WHERE dispatched_at IS NULL AND (?1 IS NULL OR project_id = ?1)
                 ORDER BY project_id, revision
                 LIMIT ?2",
            )?;
            let rows = statement
                .query_map(params![project_id, limit as i64], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(payload, dispatched_at)| {
                    Ok(DurableOutboxEvent {
                        event: serde_json::from_str(&payload)?,
                        dispatched_at: dispatched_at.map(rows::parse_timestamp).transpose()?,
                    })
                })
                .collect()
        })
        .await
    }

    pub async fn mark_outbox_dispatched(
        &self,
        event_id: jcode_tasker_types::OutboxEventId,
    ) -> StoreResult<bool> {
        self.call(move |connection| {
            let changed = connection.execute(
                "UPDATE outbox_events SET dispatched_at = ?1
                 WHERE id = ?2 AND dispatched_at IS NULL",
                params![Utc::now().to_rfc3339(), event_id.to_string()],
            )?;
            Ok(changed == 1)
        })
        .await
    }
}

fn project_by_id(connection: &Connection, project_id: ProjectId) -> StoreResult<Project> {
    let row = connection
        .query_row(
            "SELECT p.id, p.name, p.canonical_root, r.revision, p.created_at, p.updated_at
             FROM projects p
             JOIN project_revisions r ON r.project_id = p.id
             WHERE p.id = ?1",
            [project_id.to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| not_found("project", project_id))?;
    rows::project_from_row(row)
}

fn feature_by_id(
    connection: &Connection,
    project_id: ProjectId,
    feature_id: FeatureId,
) -> StoreResult<Feature> {
    let row = connection
        .query_row(
            "SELECT id, project_id, alias, parent_id, title, description, state, created_at,
                    updated_at
             FROM features WHERE project_id = ?1 AND id = ?2",
            params![project_id.to_string(), feature_id.to_string()],
            feature_row,
        )
        .optional()?
        .ok_or_else(|| not_found("feature", feature_id))?;
    rows::feature_from_row(row)
}

fn task_by_id(
    connection: &Connection,
    project_id: ProjectId,
    task_id: TaskId,
) -> StoreResult<Task> {
    let row = connection
        .query_row(
            "SELECT id, project_id, feature_id, alias, title, description, state, priority, rank,
                    created_at, updated_at
             FROM tasks WHERE project_id = ?1 AND id = ?2",
            params![project_id.to_string(), task_id.to_string()],
            task_row,
        )
        .optional()?
        .ok_or_else(|| not_found("task", task_id))?;
    rows::task_from_row(row)
}

fn feature_by_alias(
    connection: &Connection,
    project_id: ProjectId,
    alias: u64,
) -> StoreResult<Feature> {
    let alias = i64::try_from(alias)
        .map_err(|_| invalid_reference("feature", &alias.to_string(), "#F<number>"))?;
    let row = connection
        .query_row(
            "SELECT id, project_id, alias, parent_id, title, description, state, created_at,
                    updated_at
             FROM features WHERE project_id = ?1 AND alias = ?2",
            params![project_id.to_string(), alias],
            feature_row,
        )
        .optional()?
        .ok_or_else(|| TaskerError::NotFound {
            entity: "feature".to_string(),
            id: format!("#F{alias}"),
        })?;
    rows::feature_from_row(row)
}

fn task_by_alias(connection: &Connection, project_id: ProjectId, alias: u64) -> StoreResult<Task> {
    let alias = i64::try_from(alias)
        .map_err(|_| invalid_reference("task", &alias.to_string(), "#<number>"))?;
    let row = connection
        .query_row(
            "SELECT id, project_id, feature_id, alias, title, description, state, priority, rank,
                    created_at, updated_at
             FROM tasks WHERE project_id = ?1 AND alias = ?2",
            params![project_id.to_string(), alias],
            task_row,
        )
        .optional()?
        .ok_or_else(|| TaskerError::NotFound {
            entity: "task".to_string(),
            id: format!("#{alias}"),
        })?;
    rows::task_from_row(row)
}

fn invalid_reference(entity: &str, value: &str, expected: &str) -> StoreError {
    TaskerError::InvalidInput {
        field: format!("{entity}_reference"),
        message: format!("expected {expected}, received {value}"),
    }
    .into()
}

fn revision_by_project(
    connection: &Connection,
    project_id: ProjectId,
) -> StoreResult<ProjectRevision> {
    let revision: Option<i64> = connection
        .query_row(
            "SELECT revision FROM project_revisions WHERE project_id = ?1",
            [project_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    let revision = revision.ok_or_else(|| not_found("project", project_id))?;
    Ok(ProjectRevision(u64::try_from(revision).map_err(|_| {
        TaskerError::Conflict {
            message: "invalid stored project revision".to_string(),
        }
    })?))
}

fn all_features(connection: &Connection, project_id: ProjectId) -> StoreResult<Vec<Feature>> {
    let mut statement = connection.prepare(
        "SELECT id, project_id, alias, parent_id, title, description, state, created_at, updated_at
         FROM features WHERE project_id = ?1 ORDER BY alias, id",
    )?;
    let raw = statement
        .query_map([project_id.to_string()], feature_row)?
        .collect::<Result<Vec<_>, _>>()?;
    raw.into_iter().map(rows::feature_from_row).collect()
}

fn all_tasks(connection: &Connection, project_id: ProjectId) -> StoreResult<Vec<Task>> {
    let mut statement = connection.prepare(
        "SELECT id, project_id, feature_id, alias, title, description, state, priority, rank,
                created_at, updated_at
         FROM tasks
         WHERE project_id = ?1
         ORDER BY priority DESC, rank, alias, id",
    )?;
    let raw = statement
        .query_map([project_id.to_string()], task_row)?
        .collect::<Result<Vec<_>, _>>()?;
    raw.into_iter().map(rows::task_from_row).collect()
}

fn task_dependencies(
    connection: &Connection,
    project_id: ProjectId,
    task_id: TaskId,
) -> StoreResult<Vec<TaskId>> {
    task_by_id(connection, project_id, task_id)?;
    let mut statement = connection.prepare(
        "SELECT depends_on_task_id FROM task_dependencies
         WHERE project_id = ?1 AND task_id = ?2 ORDER BY depends_on_task_id",
    )?;
    let ids = statement
        .query_map(
            params![project_id.to_string(), task_id.to_string()],
            |row| row.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    ids.into_iter().map(|id| rows::parse_id(&id)).collect()
}

fn ready_tasks(connection: &Connection, project_id: ProjectId) -> StoreResult<Vec<ReadyTask>> {
    project_by_id(connection, project_id)?;
    let mut ready = Vec::new();
    for task in all_tasks(connection, project_id)? {
        let explanation = readiness_for_task(connection, &task)?;
        if explanation.ready {
            ready.push(ReadyTask { task, explanation });
        }
    }
    Ok(ready)
}

fn readiness_by_task(
    connection: &Connection,
    project_id: ProjectId,
    task_id: TaskId,
) -> StoreResult<ReadinessExplanation> {
    let task = task_by_id(connection, project_id, task_id)?;
    readiness_for_task(connection, &task)
}

fn readiness_for_task(connection: &Connection, task: &Task) -> StoreResult<ReadinessExplanation> {
    let revision = revision_by_project(connection, task.project_id)?;
    let mut restrictions = Vec::new();
    if !task.state.permits_execution() {
        restrictions.push(format!(
            "task state {} does not permit execution",
            task.state.as_str()
        ));
    }

    let mut statement = connection.prepare(
        "WITH RECURSIVE ancestors(id, state, parent_id) AS (
            SELECT id, state, parent_id FROM features
            WHERE project_id = ?1 AND id = ?2
            UNION ALL
            SELECT feature.id, feature.state, feature.parent_id
            FROM features feature
            JOIN ancestors ON feature.id = ancestors.parent_id
            WHERE feature.project_id = ?1
         )
         SELECT id, state FROM ancestors",
    )?;
    let feature_states = statement
        .query_map(
            params![task.project_id.to_string(), task.feature_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    if feature_states.is_empty() {
        return Err(not_found("feature", task.feature_id));
    }
    for (feature_id, state) in feature_states {
        let state = FeatureState::from_str(&state)?;
        if !state.permits_work() {
            restrictions.push(format!("feature {feature_id} is {}", state.as_str()));
        }
    }

    let mut statement = connection.prepare(
        "SELECT dependency.id, dependency.state
         FROM task_dependencies edge
         JOIN tasks dependency
           ON dependency.project_id = edge.project_id
          AND dependency.id = edge.depends_on_task_id
         WHERE edge.project_id = ?1 AND edge.task_id = ?2
         ORDER BY dependency.id",
    )?;
    let dependencies = statement
        .query_map(
            params![task.project_id.to_string(), task.id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut satisfied_dependencies = Vec::new();
    let mut unsatisfied_dependencies = Vec::new();
    for (id, state) in dependencies {
        let id = rows::parse_id(&id)?;
        let state = TaskState::from_str(&state)?;
        if state.satisfies_dependency() {
            satisfied_dependencies.push(id);
        } else {
            unsatisfied_dependencies.push(id);
        }
    }
    let ready = restrictions.is_empty() && unsatisfied_dependencies.is_empty();
    Ok(ReadinessExplanation {
        task_id: task.id,
        ready,
        satisfied_dependencies,
        unsatisfied_dependencies,
        restrictions,
        revision,
        policy_version: INITIAL_READINESS_POLICY_VERSION,
    })
}

fn feature_row(
    row: &tokio_rusqlite::rusqlite::Row<'_>,
) -> tokio_rusqlite::rusqlite::Result<rows::FeatureRow> {
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
}

fn task_row(
    row: &tokio_rusqlite::rusqlite::Row<'_>,
) -> tokio_rusqlite::rusqlite::Result<rows::TaskRow> {
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
}

fn not_found(entity: &str, id: impl std::fmt::Display) -> StoreError {
    TaskerError::NotFound {
        entity: entity.to_string(),
        id: id.to_string(),
    }
    .into()
}
