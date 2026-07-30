use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub key: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub id: String,
    pub artifact_id: String,
    pub number: i64,
    pub source_digest: String,
    pub rendered_digest: String,
    pub source_path: PathBuf,
    pub rendered_path: PathBuf,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub artifact_id: String,
    pub revision_id: Option<String>,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateType {
    Primary,
    Alternate,
    Experimental,
}

impl fmt::Display for CandidateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CandidateType::Primary => "primary",
            CandidateType::Alternate => "alternate",
            CandidateType::Experimental => "experimental",
        })
    }
}

impl CandidateType {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "primary" => Ok(Self::Primary),
            "alternate" => Ok(Self::Alternate),
            "experimental" => Ok(Self::Experimental),
            other => Err(ArtifactStoreError::InvalidEnum(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    Proposed,
    Trial,
    Ratified,
    Deprecated,
    Superseded,
}

impl fmt::Display for CandidateStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CandidateStatus::Proposed => "proposed",
            CandidateStatus::Trial => "trial",
            CandidateStatus::Ratified => "ratified",
            CandidateStatus::Deprecated => "deprecated",
            CandidateStatus::Superseded => "superseded",
        })
    }
}

impl CandidateStatus {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "proposed" => Ok(Self::Proposed),
            "trial" => Ok(Self::Trial),
            "ratified" => Ok(Self::Ratified),
            "deprecated" => Ok(Self::Deprecated),
            "superseded" => Ok(Self::Superseded),
            other => Err(ArtifactStoreError::InvalidEnum(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub artifact_id: String,
    pub revision_id: String,
    pub candidate_type: CandidateType,
    pub status: CandidateStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("time parse error: {0}")]
    Time(#[from] chrono::ParseError),
    #[error("invalid enum value: {0}")]
    InvalidEnum(String),
    #[error("candidate transition from {from:?} to {to:?} is not allowed")]
    InvalidCandidateTransition {
        from: CandidateStatus,
        to: CandidateStatus,
    },
}

pub type Result<T> = std::result::Result<T, ArtifactStoreError>;

pub struct ArtifactStore {
    conn: Connection,
    asset_root: PathBuf,
}

impl ArtifactStore {
    pub fn open_migrate(
        database_path: impl AsRef<Path>,
        asset_root: impl AsRef<Path>,
    ) -> Result<Self> {
        let asset_root = asset_root.as_ref().to_path_buf();
        fs::create_dir_all(&asset_root)?;
        if let Some(parent) = database_path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(database_path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn, asset_root })
    }

    pub fn create_artifact(
        &self,
        key: impl AsRef<str>,
        title: impl AsRef<str>,
    ) -> Result<Artifact> {
        let artifact = Artifact {
            id: Uuid::new_v4().to_string(),
            key: key.as_ref().to_owned(),
            title: title.as_ref().to_owned(),
            created_at: Utc::now(),
        };
        self.conn.execute(
            "INSERT INTO artifacts (id, key, title, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                artifact.id,
                artifact.key,
                artifact.title,
                artifact.created_at.to_rfc3339()
            ],
        )?;
        Ok(artifact)
    }

    pub fn get_artifact(&self, artifact_id: impl AsRef<str>) -> Result<Option<Artifact>> {
        self.conn
            .query_row(
                "SELECT id, key, title, created_at FROM artifacts WHERE id = ?1",
                params![artifact_id.as_ref()],
                artifact_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_artifacts(&self) -> Result<Vec<Artifact>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, key, title, created_at FROM artifacts ORDER BY created_at, id")?;
        let rows = stmt.query_map([], artifact_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn add_revision(
        &self,
        artifact_id: impl AsRef<str>,
        source_bytes: &[u8],
        rendered_bytes: &[u8],
    ) -> Result<Revision> {
        let artifact_id = artifact_id.as_ref();
        let number = self.conn.query_row(
            "SELECT COALESCE(MAX(number), 0) + 1 FROM revisions WHERE artifact_id = ?1",
            params![artifact_id],
            |row| row.get::<_, i64>(0),
        )?;
        let id = Uuid::new_v4().to_string();
        let source_digest = digest_hex(source_bytes);
        let rendered_digest = digest_hex(rendered_bytes);
        let source_rel = revision_asset_path(artifact_id, number, "source", &source_digest);
        let rendered_rel = revision_asset_path(artifact_id, number, "rendered", &rendered_digest);
        atomic_write(&self.asset_root.join(&source_rel), source_bytes)?;
        atomic_write(&self.asset_root.join(&rendered_rel), rendered_bytes)?;
        let revision = Revision {
            id,
            artifact_id: artifact_id.to_owned(),
            number,
            source_digest,
            rendered_digest,
            source_path: source_rel,
            rendered_path: rendered_rel,
            created_at: Utc::now(),
        };
        self.conn.execute(
            "INSERT INTO revisions (id, artifact_id, number, source_digest, rendered_digest, source_path, rendered_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![revision.id, revision.artifact_id, revision.number, revision.source_digest, revision.rendered_digest, path_to_db(&revision.source_path), path_to_db(&revision.rendered_path), revision.created_at.to_rfc3339()],
        )?;
        Ok(revision)
    }

    pub fn get_revision(&self, revision_id: impl AsRef<str>) -> Result<Option<Revision>> {
        self.conn
            .query_row(
                "SELECT id, artifact_id, number, source_digest, rendered_digest, source_path, rendered_path, created_at FROM revisions WHERE id = ?1",
                params![revision_id.as_ref()],
                revision_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_revisions(&self, artifact_id: impl AsRef<str>) -> Result<Vec<Revision>> {
        let mut stmt = self.conn.prepare("SELECT id, artifact_id, number, source_digest, rendered_digest, source_path, rendered_path, created_at FROM revisions WHERE artifact_id = ?1 ORDER BY number")?;
        let rows = stmt.query_map(params![artifact_id.as_ref()], revision_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn read_source_bytes(&self, revision: &Revision) -> Result<Vec<u8>> {
        fs::read(self.asset_root.join(&revision.source_path)).map_err(Into::into)
    }

    pub fn read_rendered_bytes(&self, revision: &Revision) -> Result<Vec<u8>> {
        fs::read(self.asset_root.join(&revision.rendered_path)).map_err(Into::into)
    }

    pub fn add_annotation(
        &self,
        artifact_id: impl AsRef<str>,
        revision_id: Option<&str>,
        body: impl AsRef<str>,
    ) -> Result<Annotation> {
        let annotation = Annotation {
            id: Uuid::new_v4().to_string(),
            artifact_id: artifact_id.as_ref().to_owned(),
            revision_id: revision_id.map(ToOwned::to_owned),
            body: body.as_ref().to_owned(),
            created_at: Utc::now(),
        };
        self.conn.execute(
            "INSERT INTO annotations (id, artifact_id, revision_id, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![annotation.id, annotation.artifact_id, annotation.revision_id, annotation.body, annotation.created_at.to_rfc3339()],
        )?;
        Ok(annotation)
    }

    pub fn list_annotations(&self, artifact_id: impl AsRef<str>) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn.prepare("SELECT id, artifact_id, revision_id, body, created_at FROM annotations WHERE artifact_id = ?1 ORDER BY created_at, id")?;
        let rows = stmt.query_map(params![artifact_id.as_ref()], annotation_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn register_candidate(
        &self,
        revision_id: impl AsRef<str>,
        candidate_type: CandidateType,
        status: CandidateStatus,
    ) -> Result<Candidate> {
        let revision = self
            .get_revision(revision_id.as_ref())?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let now = Utc::now();
        let candidate = Candidate {
            id: Uuid::new_v4().to_string(),
            artifact_id: revision.artifact_id,
            revision_id: revision_id.as_ref().to_owned(),
            candidate_type,
            status,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            "INSERT INTO candidates (id, artifact_id, revision_id, candidate_type, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![candidate.id, candidate.artifact_id, candidate.revision_id, candidate.candidate_type.to_string(), candidate.status.to_string(), candidate.created_at.to_rfc3339(), candidate.updated_at.to_rfc3339()],
        )?;
        Ok(candidate)
    }

    pub fn update_candidate_status(
        &self,
        candidate_id: impl AsRef<str>,
        status: CandidateStatus,
    ) -> Result<Candidate> {
        let candidate = self
            .get_candidate(candidate_id.as_ref())?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        if !is_allowed_transition(candidate.status, status) {
            return Err(ArtifactStoreError::InvalidCandidateTransition {
                from: candidate.status,
                to: status,
            });
        }
        let updated_at = Utc::now();
        self.conn.execute(
            "UPDATE candidates SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                status.to_string(),
                updated_at.to_rfc3339(),
                candidate_id.as_ref()
            ],
        )?;
        Ok(Candidate {
            status,
            updated_at,
            ..candidate
        })
    }

    pub fn get_candidate(&self, candidate_id: impl AsRef<str>) -> Result<Option<Candidate>> {
        self.conn
            .query_row(
                "SELECT id, artifact_id, revision_id, candidate_type, status, created_at, updated_at FROM candidates WHERE id = ?1",
                params![candidate_id.as_ref()],
                candidate_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_candidates(&self, artifact_id: impl AsRef<str>) -> Result<Vec<Candidate>> {
        let mut stmt = self.conn.prepare("SELECT id, artifact_id, revision_id, candidate_type, status, created_at, updated_at FROM candidates WHERE artifact_id = ?1 ORDER BY created_at, id")?;
        let rows = stmt.query_map(params![artifact_id.as_ref()], candidate_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS artifacts (
  id TEXT PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS revisions (
  id TEXT PRIMARY KEY,
  artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
  number INTEGER NOT NULL,
  source_digest TEXT NOT NULL,
  rendered_digest TEXT NOT NULL,
  source_path TEXT NOT NULL,
  rendered_path TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE (artifact_id, number)
);
CREATE TRIGGER IF NOT EXISTS revisions_no_update
BEFORE UPDATE ON revisions
BEGIN
  SELECT RAISE(ABORT, 'revisions are immutable');
END;
CREATE TRIGGER IF NOT EXISTS revisions_no_delete
BEFORE DELETE ON revisions
BEGIN
  SELECT RAISE(ABORT, 'revisions are immutable');
END;
CREATE TABLE IF NOT EXISTS annotations (
  id TEXT PRIMARY KEY,
  artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
  revision_id TEXT REFERENCES revisions(id),
  body TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS candidates (
  id TEXT PRIMARY KEY,
  artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
  revision_id TEXT NOT NULL REFERENCES revisions(id),
  candidate_type TEXT NOT NULL CHECK(candidate_type IN ('primary','alternate','experimental')),
  status TEXT NOT NULL CHECK(status IN ('proposed','trial','ratified','deprecated','superseded')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
"#;

fn artifact_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Artifact> {
    let created_at: String = row.get(3)?;
    Ok(Artifact {
        id: row.get(0)?,
        key: row.get(1)?,
        title: row.get(2)?,
        created_at: parse_time_sql(&created_at)?,
    })
}

fn revision_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Revision> {
    let created_at: String = row.get(7)?;
    let source_path: String = row.get(5)?;
    let rendered_path: String = row.get(6)?;
    Ok(Revision {
        id: row.get(0)?,
        artifact_id: row.get(1)?,
        number: row.get(2)?,
        source_digest: row.get(3)?,
        rendered_digest: row.get(4)?,
        source_path: PathBuf::from(source_path),
        rendered_path: PathBuf::from(rendered_path),
        created_at: parse_time_sql(&created_at)?,
    })
}

fn annotation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Annotation> {
    let created_at: String = row.get(4)?;
    Ok(Annotation {
        id: row.get(0)?,
        artifact_id: row.get(1)?,
        revision_id: row.get(2)?,
        body: row.get(3)?,
        created_at: parse_time_sql(&created_at)?,
    })
}

fn candidate_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Candidate> {
    let candidate_type: String = row.get(3)?;
    let status: String = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;
    Ok(Candidate {
        id: row.get(0)?,
        artifact_id: row.get(1)?,
        revision_id: row.get(2)?,
        candidate_type: CandidateType::parse(&candidate_type).map_err(to_sql_error)?,
        status: CandidateStatus::parse(&status).map_err(to_sql_error)?,
        created_at: parse_time_sql(&created_at)?,
        updated_at: parse_time_sql(&updated_at)?,
    })
}

fn parse_time_sql(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(to_sql_error)
}

fn to_sql_error<E>(error: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

fn digest_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn revision_asset_path(artifact_id: &str, number: i64, kind: &str, digest: &str) -> PathBuf {
    PathBuf::from("artifacts")
        .join(artifact_id)
        .join(format!("r{number:08}-{kind}-{digest}.blob"))
}

fn path_to_db(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn is_allowed_transition(from: CandidateStatus, to: CandidateStatus) -> bool {
    use CandidateStatus::*;
    matches!(
        (from, to),
        (
            Proposed,
            Proposed | Trial | Ratified | Deprecated | Superseded
        ) | (Trial, Trial | Ratified | Deprecated | Superseded)
            | (Ratified, Ratified | Deprecated | Superseded)
            | (Deprecated, Deprecated | Superseded)
            | (Superseded, Superseded)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> (tempfile::TempDir, ArtifactStore) {
        let dir = tempdir().unwrap();
        let db = dir.path().join("store.sqlite3");
        let assets = dir.path().join("assets");
        let store = ArtifactStore::open_migrate(db, assets).unwrap();
        (dir, store)
    }

    #[test]
    fn revision_history_is_append_only_and_assets_are_digested() {
        let (_dir, store) = store();
        let artifact = store.create_artifact("deck", "Deck").unwrap();
        let first = store
            .add_revision(&artifact.id, b"source one", b"rendered one")
            .unwrap();
        let second = store
            .add_revision(&artifact.id, b"source two", b"rendered two")
            .unwrap();

        assert_eq!(first.number, 1);
        assert_eq!(second.number, 2);
        assert_eq!(first.source_digest, digest_hex(b"source one"));
        assert_eq!(store.read_source_bytes(&first).unwrap(), b"source one");
        assert_eq!(store.read_rendered_bytes(&second).unwrap(), b"rendered two");
        assert_eq!(
            store.list_revisions(&artifact.id).unwrap(),
            vec![first, second]
        );
    }

    #[test]
    fn revisions_are_immutable_at_database_boundary() {
        let (_dir, store) = store();
        let artifact = store.create_artifact("brief", "Brief").unwrap();
        let revision = store
            .add_revision(&artifact.id, b"source", b"rendered")
            .unwrap();

        let update = store.conn.execute(
            "UPDATE revisions SET number = 99 WHERE id = ?1",
            params![revision.id],
        );
        assert!(
            update
                .unwrap_err()
                .to_string()
                .contains("revisions are immutable")
        );
        let delete = store
            .conn
            .execute("DELETE FROM revisions WHERE id = ?1", params![revision.id]);
        assert!(
            delete
                .unwrap_err()
                .to_string()
                .contains("revisions are immutable")
        );
        assert_eq!(store.get_revision(&revision.id).unwrap().unwrap().number, 1);
    }

    #[test]
    fn annotations_attach_to_artifact_and_revision() {
        let (_dir, store) = store();
        let artifact = store.create_artifact("audit", "Audit").unwrap();
        let revision = store
            .add_revision(&artifact.id, b"source", b"rendered")
            .unwrap();
        let artifact_note = store
            .add_annotation(&artifact.id, None, "artifact scope")
            .unwrap();
        let revision_note = store
            .add_annotation(&artifact.id, Some(&revision.id), "revision scope")
            .unwrap();

        let notes = store.list_annotations(&artifact.id).unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].id, artifact_note.id);
        assert_eq!(notes[0].revision_id, None);
        assert_eq!(notes[1].id, revision_note.id);
        assert_eq!(notes[1].revision_id.as_deref(), Some(revision.id.as_str()));
    }

    #[test]
    fn candidates_follow_allowed_lifecycle_transitions() {
        let (_dir, store) = store();
        let artifact = store.create_artifact("proposal", "Proposal").unwrap();
        let revision = store
            .add_revision(&artifact.id, b"source", b"rendered")
            .unwrap();
        let candidate = store
            .register_candidate(
                &revision.id,
                CandidateType::Primary,
                CandidateStatus::Proposed,
            )
            .unwrap();
        assert_eq!(candidate.status, CandidateStatus::Proposed);

        let trial = store
            .update_candidate_status(&candidate.id, CandidateStatus::Trial)
            .unwrap();
        assert_eq!(trial.status, CandidateStatus::Trial);
        let ratified = store
            .update_candidate_status(&candidate.id, CandidateStatus::Ratified)
            .unwrap();
        assert_eq!(ratified.status, CandidateStatus::Ratified);
        let superseded = store
            .update_candidate_status(&candidate.id, CandidateStatus::Superseded)
            .unwrap();
        assert_eq!(superseded.status, CandidateStatus::Superseded);
        let invalid = store
            .update_candidate_status(&candidate.id, CandidateStatus::Trial)
            .unwrap_err();
        assert!(matches!(
            invalid,
            ArtifactStoreError::InvalidCandidateTransition {
                from: CandidateStatus::Superseded,
                to: CandidateStatus::Trial
            }
        ));

        let candidates = store.list_candidates(&artifact.id).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].status, CandidateStatus::Superseded);
    }
}
