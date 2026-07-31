use std::path::Path;

use chrono::{DateTime, Utc};
use jcode_metatool_types::{
    MetaToolError, ObjectId, ObjectKey, PutObject, SearchHit, StoredObject, validate_name,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio_rusqlite::{Connection, rusqlite};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Domain(#[from] MetaToolError),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("database worker error: {0}")]
    Worker(String),
    #[error("stored JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("stored timestamp is invalid: {0}")]
    Timestamp(String),
}

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Clone)]
pub struct MetaToolStore {
    connection: Connection,
}

impl MetaToolStore {
    pub async fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|error| MetaToolError::Validation {
                field: "database_path".to_owned(),
                message: error.to_string(),
            })?;
        }
        let connection = Connection::open(path).await.map_err(StoreError::Database)?;
        Self::initialize(connection).await
    }

    pub async fn open_in_memory() -> StoreResult<Self> {
        let connection = Connection::open_in_memory()
            .await
            .map_err(StoreError::Database)?;
        Self::initialize(connection).await
    }

    async fn initialize(connection: Connection) -> StoreResult<Self> {
        connection
            .call(configure_and_migrate)
            .await
            .map_err(StoreError::from_call_error)?;
        Ok(Self { connection })
    }

    pub async fn put(&self, input: PutObject) -> StoreResult<StoredObject> {
        validate_object_key(&input.workspace_id, &input.collection, &input.key)?;
        let value_json = serde_json::to_string(&input.value)?;
        let tags = normalize_tags(input.tags)?;
        let tags_json = serde_json::to_string(&tags)?;
        let search_text = searchable_text(&input.value, &tags);
        let content_hash = hex_digest(format!("{value_json}\n{tags_json}").as_bytes());
        let now = Utc::now();

        self.call(move |connection| {
            let transaction = connection.transaction()?;
            let existing = query_object(
                &transaction,
                &input.workspace_id,
                &input.collection,
                &input.key,
            )?;
            let object = match existing {
                Some(existing) => {
                    if let Some(expected) = input.expected_revision
                        && expected != existing.revision
                    {
                        return Err(StoreError::Domain(MetaToolError::RevisionConflict {
                            actual_revision: existing.revision,
                        }));
                    }
                    let revision = existing.revision + 1;
                    transaction.execute(
                        "UPDATE mt_objects SET value_json=?1, tags_json=?2, search_text=?3, revision=?4, content_hash=?5, updated_at=?6 WHERE id=?7",
                        rusqlite::params![value_json, tags_json, search_text, revision, content_hash, now.to_rfc3339(), existing.id.as_str()],
                    )?;
                    StoredObject { value: input.value, tags, revision, content_hash, updated_at: now, ..existing }
                }
                None => {
                    if input.expected_revision.is_some() {
                        return Err(StoreError::Domain(MetaToolError::NotFound));
                    }
                    let object = StoredObject {
                        id: ObjectId::new(), workspace_id: input.workspace_id, collection: input.collection,
                        key: input.key, value: input.value, tags, revision: 1, content_hash,
                        created_at: now, updated_at: now,
                    };
                    transaction.execute(
                        "INSERT INTO mt_objects (id,workspace_id,collection_name,object_key,value_json,tags_json,search_text,revision,content_hash,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                        rusqlite::params![object.id.as_str(), object.workspace_id, object.collection, object.key, value_json, tags_json, search_text, object.revision, object.content_hash, now.to_rfc3339(), now.to_rfc3339()],
                    )?;
                    object
                }
            };
            transaction.commit()?;
            Ok(object)
        }).await
    }

    pub async fn get(&self, key: ObjectKey) -> StoreResult<Option<StoredObject>> {
        validate_object_key(&key.workspace_id, &key.collection, &key.key)?;
        self.call(move |connection| {
            query_object(connection, &key.workspace_id, &key.collection, &key.key)
        })
        .await
    }

    pub async fn delete(
        &self,
        key: ObjectKey,
        expected_revision: Option<u64>,
    ) -> StoreResult<bool> {
        validate_object_key(&key.workspace_id, &key.collection, &key.key)?;
        self.call(move |connection| {
            let transaction = connection.transaction()?;
            let existing =
                query_object(&transaction, &key.workspace_id, &key.collection, &key.key)?;
            let Some(existing) = existing else {
                return Ok(false);
            };
            if let Some(expected) = expected_revision
                && expected != existing.revision
            {
                return Err(StoreError::Domain(MetaToolError::RevisionConflict {
                    actual_revision: existing.revision,
                }));
            }
            transaction.execute("DELETE FROM mt_objects WHERE id=?1", [existing.id.as_str()])?;
            transaction.commit()?;
            Ok(true)
        })
        .await
    }

    pub async fn search(
        &self,
        workspace_id: String,
        query: String,
        limit: usize,
    ) -> StoreResult<Vec<SearchHit>> {
        validate_name("workspace_id", &workspace_id)?;
        let limit = limit.clamp(1, 100) as i64;
        self.call(move |connection| {
            let mut statement = connection.prepare(
                "SELECT o.id,o.workspace_id,o.collection_name,o.object_key,o.value_json,o.tags_json,o.revision,o.content_hash,o.created_at,o.updated_at,bm25(mt_objects_fts) FROM mt_objects_fts JOIN mt_objects o ON o.rowid=mt_objects_fts.rowid WHERE mt_objects_fts MATCH ?1 AND o.workspace_id=?2 ORDER BY bm25(mt_objects_fts),o.collection_name,o.object_key LIMIT ?3",
            )?;
            let rows = statement.query_map(rusqlite::params![query, workspace_id, limit], |row| {
                Ok((row_to_object(row)?, row.get::<_, f64>(10)?))
            })?;
            rows.map(|row| row.map(|(object, rank)| SearchHit { object, rank }).map_err(StoreError::from))
                .collect()
        }).await
    }

    pub async fn list(
        &self,
        workspace_id: String,
        collection: Option<String>,
        limit: usize,
    ) -> StoreResult<Vec<StoredObject>> {
        validate_name("workspace_id", &workspace_id)?;
        if let Some(collection) = &collection {
            validate_name("collection", collection)?;
        }
        let limit = limit.clamp(1, 500) as i64;
        self.call(move |connection| {
            let sql = if collection.is_some() {
                "SELECT id,workspace_id,collection_name,object_key,value_json,tags_json,revision,content_hash,created_at,updated_at FROM mt_objects WHERE workspace_id=?1 AND collection_name=?2 ORDER BY collection_name,object_key LIMIT ?3"
            } else {
                "SELECT id,workspace_id,collection_name,object_key,value_json,tags_json,revision,content_hash,created_at,updated_at FROM mt_objects WHERE workspace_id=?1 ORDER BY collection_name,object_key LIMIT ?2"
            };
            let mut statement = connection.prepare(sql)?;
            if let Some(collection) = collection {
                statement
                    .query_map(
                        rusqlite::params![workspace_id, collection, limit],
                        row_to_object,
                    )?
                    .map(|row| row.map_err(StoreError::from))
                    .collect()
            } else {
                statement
                    .query_map(rusqlite::params![workspace_id, limit], row_to_object)?
                    .map(|row| row.map_err(StoreError::from))
                    .collect()
            }
        }).await
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

impl StoreError {
    fn from_call_error(error: tokio_rusqlite::Error<Self>) -> Self {
        match error {
            tokio_rusqlite::Error::Error(error) => error,
            tokio_rusqlite::Error::Close((_, error)) => Self::Database(error),
            error => Self::Worker(error.to_string()),
        }
    }
}

fn configure_and_migrate(connection: &mut rusqlite::Connection) -> StoreResult<()> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS mt_objects (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            collection_name TEXT NOT NULL,
            object_key TEXT NOT NULL,
            value_json TEXT NOT NULL,
            tags_json TEXT NOT NULL,
            search_text TEXT NOT NULL,
            revision INTEGER NOT NULL CHECK (revision > 0),
            content_hash TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(workspace_id, collection_name, object_key)
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS mt_objects_fts USING fts5(search_text, content='mt_objects', content_rowid='rowid');
        CREATE TRIGGER IF NOT EXISTS mt_objects_ai AFTER INSERT ON mt_objects BEGIN
          INSERT INTO mt_objects_fts(rowid,search_text) VALUES (new.rowid,new.search_text);
        END;
        CREATE TRIGGER IF NOT EXISTS mt_objects_ad AFTER DELETE ON mt_objects BEGIN
          INSERT INTO mt_objects_fts(mt_objects_fts,rowid,search_text) VALUES('delete',old.rowid,old.search_text);
        END;
        CREATE TRIGGER IF NOT EXISTS mt_objects_au AFTER UPDATE ON mt_objects BEGIN
          INSERT INTO mt_objects_fts(mt_objects_fts,rowid,search_text) VALUES('delete',old.rowid,old.search_text);
          INSERT INTO mt_objects_fts(rowid,search_text) VALUES(new.rowid,new.search_text);
        END;",
    )?;
    connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn validate_object_key(workspace_id: &str, collection: &str, key: &str) -> StoreResult<()> {
    validate_name("workspace_id", workspace_id)?;
    validate_name("collection", collection)?;
    validate_name("key", key)?;
    Ok(())
}

fn normalize_tags(mut tags: Vec<String>) -> StoreResult<Vec<String>> {
    for tag in &tags {
        validate_name("tag", tag)?;
    }
    tags.sort();
    tags.dedup();
    if tags.len() > 64 {
        return Err(MetaToolError::Validation {
            field: "tags".to_owned(),
            message: "at most 64 tags are allowed".to_owned(),
        }
        .into());
    }
    Ok(tags)
}

fn searchable_text(value: &Value, tags: &[String]) -> String {
    format!("{} {}", value, tags.join(" "))
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn query_object(
    connection: &rusqlite::Connection,
    workspace_id: &str,
    collection: &str,
    key: &str,
) -> StoreResult<Option<StoredObject>> {
    connection
        .query_row(
            "SELECT id,workspace_id,collection_name,object_key,value_json,tags_json,revision,content_hash,created_at,updated_at FROM mt_objects WHERE workspace_id=?1 AND collection_name=?2 AND object_key=?3",
            rusqlite::params![workspace_id, collection, key],
            row_to_object,
        )
        .optional()
        .map_err(StoreError::from)
}

fn row_to_object(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredObject> {
    let parse_time = |index| {
        let raw: String = row.get(index)?;
        DateTime::parse_from_rfc3339(&raw)
            .map(|time| time.with_timezone(&Utc))
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
    };
    let raw_id: String = row.get(0)?;
    let id = ObjectId::parse(raw_id).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(StoredObject {
        id,
        workspace_id: row.get(1)?,
        collection: row.get(2)?,
        key: row.get(3)?,
        value: parse_json_column(row, 4)?,
        tags: parse_json_column(row, 5)?,
        revision: row.get(6)?,
        content_hash: row.get(7)?,
        created_at: parse_time(8)?,
        updated_at: parse_time(9)?,
    })
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let raw: String = row.get(index)?;
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn put(value: Value, expected_revision: Option<u64>) -> PutObject {
        PutObject {
            workspace_id: "workspace_1".to_owned(),
            collection: "notes".to_owned(),
            key: "auth".to_owned(),
            value,
            tags: vec!["security".to_owned()],
            expected_revision,
        }
    }

    #[tokio::test]
    async fn put_get_revision_and_search_are_transactional() {
        let store = MetaToolStore::open_in_memory().await.unwrap();
        let first = store
            .put(put(json!({"text":"capability broker"}), None))
            .await
            .unwrap();
        assert_eq!(first.revision, 1);
        let second = store
            .put(put(json!({"text":"secure capability broker"}), Some(1)))
            .await
            .unwrap();
        assert_eq!(second.revision, 2);
        let found = store
            .search("workspace_1".to_owned(), "secure".to_owned(), 10)
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].object.revision, 2);
        let conflict = store
            .put(put(json!({"text":"stale"}), Some(1)))
            .await
            .unwrap_err();
        assert!(matches!(
            conflict,
            StoreError::Domain(MetaToolError::RevisionConflict { actual_revision: 2 })
        ));
        let current = store
            .get(ObjectKey {
                workspace_id: "workspace_1".to_owned(),
                collection: "notes".to_owned(),
                key: "auth".to_owned(),
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current.value, json!({"text":"secure capability broker"}));
    }

    #[tokio::test]
    async fn workspace_partitioning_and_reopen_are_durable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metatool.sqlite3");
        let store = MetaToolStore::open(&path).await.unwrap();
        store
            .put(put(json!({"text":"durable"}), None))
            .await
            .unwrap();
        drop(store);
        let reopened = MetaToolStore::open(&path).await.unwrap();
        assert_eq!(
            reopened
                .list("workspace_1".to_owned(), None, 10)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            reopened
                .list("workspace_2".to_owned(), None, 10)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
