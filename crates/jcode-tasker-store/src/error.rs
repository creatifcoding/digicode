use std::fmt;

use jcode_tasker_types::TaskerError;
use tokio_rusqlite::{Error as TokioRusqliteError, rusqlite};

#[derive(Debug)]
pub enum StoreError {
    ConnectionClosed,
    Database(rusqlite::Error),
    Json(serde_json::Error),
    Domain(TaskerError),
    InvalidTimestamp(String),
}

pub type StoreResult<T> = Result<T, StoreError>;

impl StoreError {
    pub(crate) fn from_call_error(error: TokioRusqliteError<Self>) -> Self {
        match error {
            TokioRusqliteError::ConnectionClosed => Self::ConnectionClosed,
            TokioRusqliteError::Close((_, error)) => Self::Database(error),
            TokioRusqliteError::Error(error) => error,
            _ => Self::ConnectionClosed,
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConnectionClosed => formatter.write_str("tasker SQLite connection is closed"),
            Self::Database(error) => write!(formatter, "tasker SQLite error: {error}"),
            Self::Json(error) => write!(formatter, "tasker JSON error: {error}"),
            Self::Domain(error) => error.fmt(formatter),
            Self::InvalidTimestamp(value) => write!(formatter, "invalid stored timestamp: {value}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Domain(error) => Some(error),
            Self::ConnectionClosed | Self::InvalidTimestamp(_) => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<TaskerError> for StoreError {
    fn from(error: TaskerError) -> Self {
        Self::Domain(error)
    }
}
