mod mutations;
mod queries;
mod rows;

use std::path::Path;

use jcode_tasker_types::{OutboxEventId, ProjectRevision};
use tokio_rusqlite::{Connection, rusqlite};

use crate::{
    SqliteConnectionState, StoreConfig, StoreError, StoreResult, migrations::configure_and_migrate,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationResult<T> {
    pub value: T,
    pub revision: ProjectRevision,
    pub event_id: OutboxEventId,
}

#[derive(Clone)]
pub struct TaskerStore {
    connection: Connection,
}

impl TaskerStore {
    pub async fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        Self::open_with_config(path, StoreConfig::default()).await
    }

    pub async fn open_with_config(
        path: impl AsRef<Path>,
        config: StoreConfig,
    ) -> StoreResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                StoreError::Domain(jcode_tasker_types::TaskerError::Conflict {
                    message: format!("failed to create tasker database directory: {error}"),
                })
            })?;
        }
        let connection = Connection::open(path).await.map_err(StoreError::Database)?;
        Self::initialize(connection, config).await
    }

    pub async fn open_in_memory() -> StoreResult<Self> {
        let connection = Connection::open_in_memory()
            .await
            .map_err(StoreError::Database)?;
        Self::initialize(connection, StoreConfig::default()).await
    }

    async fn initialize(connection: Connection, config: StoreConfig) -> StoreResult<Self> {
        connection
            .call(move |connection| configure_and_migrate(connection, config))
            .await
            .map_err(StoreError::from_call_error)?;
        Ok(Self { connection })
    }

    pub async fn connection_state(&self) -> StoreResult<SqliteConnectionState> {
        self.call(|connection| {
            let foreign_keys: bool =
                connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
            let journal_mode: String =
                connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
            let busy_timeout_ms: u64 =
                connection.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
            let synchronous: i64 =
                connection.pragma_query_value(None, "synchronous", |row| row.get(0))?;
            let schema_version: u32 =
                connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
            Ok(SqliteConnectionState {
                foreign_keys,
                journal_mode,
                busy_timeout_ms,
                synchronous,
                schema_version,
            })
        })
        .await
    }

    async fn call<F, R>(&self, operation: F) -> StoreResult<R>
    where
        F: FnOnce(&mut rusqlite::Connection) -> StoreResult<R> + Send + 'static,
        R: Send + 'static,
    {
        self.connection
            .call(operation)
            .await
            .map_err(StoreError::from_call_error)
    }
}
