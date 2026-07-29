use std::str::FromStr;

use chrono::{DateTime, Utc};
use jcode_tasker_types::{
    Feature, FeatureAlias, FeatureState, Project, ProjectRevision, Task, TaskAlias, TaskPriority,
    TaskState, TaskerError,
};

use crate::{StoreError, StoreResult};

pub(crate) type ProjectRow = (String, String, Option<String>, i64, String, String);
pub(crate) type FeatureRow = (
    String,
    String,
    i64,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
);
pub(crate) type TaskRow = (
    String,
    String,
    String,
    i64,
    String,
    String,
    String,
    i64,
    i64,
    String,
    String,
);

pub(crate) fn project_from_row(row: ProjectRow) -> StoreResult<Project> {
    Ok(Project {
        id: parse_id(&row.0)?,
        name: row.1,
        canonical_root: row.2,
        revision: ProjectRevision(u64::try_from(row.3).map_err(|_| invalid_number("revision"))?),
        created_at: parse_timestamp(row.4)?,
        updated_at: parse_timestamp(row.5)?,
    })
}

pub(crate) fn feature_from_row(row: FeatureRow) -> StoreResult<Feature> {
    Ok(Feature {
        id: parse_id(&row.0)?,
        project_id: parse_id(&row.1)?,
        alias: FeatureAlias(u64::try_from(row.2).map_err(|_| invalid_number("feature alias"))?),
        parent_id: row.3.as_deref().map(parse_id).transpose()?,
        title: row.4,
        description: row.5,
        state: FeatureState::from_str(&row.6)?,
        created_at: parse_timestamp(row.7)?,
        updated_at: parse_timestamp(row.8)?,
    })
}

pub(crate) fn task_from_row(row: TaskRow) -> StoreResult<Task> {
    Ok(Task {
        id: parse_id(&row.0)?,
        project_id: parse_id(&row.1)?,
        feature_id: parse_id(&row.2)?,
        alias: TaskAlias(u64::try_from(row.3).map_err(|_| invalid_number("task alias"))?),
        title: row.4,
        description: row.5,
        state: TaskState::from_str(&row.6)?,
        priority: TaskPriority::from_sort_value(row.7).ok_or_else(|| invalid_number("priority"))?,
        rank: row.8,
        created_at: parse_timestamp(row.9)?,
        updated_at: parse_timestamp(row.10)?,
    })
}

pub(crate) fn parse_id<T>(value: &str) -> StoreResult<T>
where
    T: FromStr<Err = TaskerError>,
{
    value.parse().map_err(StoreError::Domain)
}

pub(crate) fn parse_timestamp(value: String) -> StoreResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| StoreError::InvalidTimestamp(value))
}

fn invalid_number(field: &str) -> StoreError {
    TaskerError::Conflict {
        message: format!("invalid stored {field}"),
    }
    .into()
}
