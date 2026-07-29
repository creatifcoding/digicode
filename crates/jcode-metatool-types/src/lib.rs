use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(format!("{}{}", $prefix, Uuid::now_v7()))
            }

            pub fn parse(value: impl Into<String>) -> Result<Self, MetaToolError> {
                let value = value.into();
                let Some(uuid) = value.strip_prefix($prefix) else {
                    return Err(MetaToolError::InvalidId { value });
                };
                Uuid::parse_str(uuid).map_err(|_| MetaToolError::InvalidId {
                    value: value.clone(),
                })?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

typed_id!(ObjectId, "mto_");
typed_id!(ExecutionId, "mte_");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionProfile {
    Pure,
    WorkspaceRead,
    WorkspaceMutate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredObject {
    pub id: ObjectId,
    pub workspace_id: String,
    pub collection: String,
    pub key: String,
    pub value: Value,
    pub tags: Vec<String>,
    pub revision: u64,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PutObject {
    pub workspace_id: String,
    pub collection: String,
    pub key: String,
    pub value: Value,
    #[serde(default)]
    pub tags: Vec<String>,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectKey {
    pub workspace_id: String,
    pub collection: String,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub object: StoredObject,
    pub rank: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionLimits {
    pub cpu_time_ms: u64,
    pub wall_time_ms: u64,
    pub heap_mb: u32,
    pub max_output_bytes: usize,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            cpu_time_ms: 250,
            wall_time_ms: 1_000,
            heap_mb: 32,
            max_output_bytes: 64 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub id: ExecutionId,
    pub source: String,
    pub inputs: Value,
    pub profile: ExecutionProfile,
    pub limits: ExecutionLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub id: ExecutionId,
    pub outcome: ExecutionOutcome,
    pub value: Option<Value>,
    pub output: String,
    pub duration_ms: u64,
    pub termination_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionOutcome {
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum MetaToolError {
    #[error("invalid MetaTool identifier: {value}")]
    InvalidId { value: String },
    #[error("invalid {field}: {message}")]
    Validation { field: String, message: String },
    #[error("object already exists at revision {actual_revision}")]
    RevisionConflict { actual_revision: u64 },
    #[error("object not found")]
    NotFound,
    #[error("runtime unavailable: {message}")]
    RuntimeUnavailable { message: String },
    #[error("execution failed: {message}")]
    Execution { message: String },
}

pub fn validate_name(field: &str, value: &str) -> Result<(), MetaToolError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(MetaToolError::Validation {
            field: field.to_owned(),
            message: "must be 1-128 ASCII letters, digits, '.', '_', or '-'".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_prefixed_uuid_v7_values() {
        let id = ObjectId::new();
        assert!(id.as_str().starts_with("mto_"));
        assert_eq!(ObjectId::parse(id.to_string()).unwrap(), id);
        assert!(ExecutionId::parse(id.to_string()).is_err());
    }

    #[test]
    fn names_are_bounded_and_path_free() {
        assert!(validate_name("collection", "research.notes").is_ok());
        assert!(validate_name("collection", "../secrets").is_err());
        assert!(validate_name("collection", "").is_err());
    }
}
