use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteSynchronous {
    Normal,
    Full,
}

impl SqliteSynchronous {
    pub(crate) const fn pragma_value(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Full => "FULL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreConfig {
    pub busy_timeout: Duration,
    pub synchronous: SqliteSynchronous,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            busy_timeout: Duration::from_secs(5),
            synchronous: SqliteSynchronous::Normal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteConnectionState {
    pub foreign_keys: bool,
    pub journal_mode: String,
    pub busy_timeout_ms: u64,
    pub synchronous: i64,
    pub schema_version: u32,
}
