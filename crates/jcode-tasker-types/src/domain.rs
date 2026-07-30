use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use uuid::Uuid;

use crate::TaskerError;

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, concat!($prefix, "{}"), self.0)
            }
        }

        impl FromStr for $name {
            type Err = TaskerError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let raw = value
                    .strip_prefix($prefix)
                    .ok_or_else(|| TaskerError::InvalidId {
                        expected_prefix: $prefix.to_string(),
                        value: value.to_string(),
                    })?;
                Uuid::parse_str(raw)
                    .map(Self)
                    .map_err(|_| TaskerError::InvalidId {
                        expected_prefix: $prefix.to_string(),
                        value: value.to_string(),
                    })
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(de::Error::custom)
            }
        }
    };
}

typed_id!(ProjectId, "proj_");
typed_id!(FeatureId, "feat_");
typed_id!(TaskId, "task_");
typed_id!(OutboxEventId, "evt_");
typed_id!(CandidateSetId, "cset_");
typed_id!(CandidateId, "cand_");
typed_id!(AdjudicationRoundId, "adj_");
typed_id!(BallotId, "ballot_");
typed_id!(PromotionIntentId, "promote_");

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectRevision(pub u64);

impl ProjectRevision {
    pub const ZERO: Self = Self(0);

    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for ProjectRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeatureAlias(pub u64);

impl fmt::Display for FeatureAlias {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "#F{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskAlias(pub u64);

impl fmt::Display for TaskAlias {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "#{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureState {
    #[default]
    Open,
    Active,
    Closed,
    Archived,
}

impl FeatureState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Active => "active",
            Self::Closed => "closed",
            Self::Archived => "archived",
        }
    }

    pub const fn permits_work(self) -> bool {
        matches!(self, Self::Open | Self::Active)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (Self::Open, Self::Active | Self::Closed | Self::Archived)
                    | (Self::Active, Self::Open | Self::Closed | Self::Archived)
                    | (Self::Closed, Self::Open | Self::Archived)
                    | (Self::Archived, Self::Open)
            )
    }

    pub fn transition_to(self, next: Self) -> Result<Self, TaskerError> {
        self.can_transition_to(next)
            .then_some(next)
            .ok_or_else(|| TaskerError::invalid_feature_transition(self, next))
    }
}

impl FromStr for FeatureState {
    type Err = TaskerError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "open" => Ok(Self::Open),
            "active" => Ok(Self::Active),
            "closed" => Ok(Self::Closed),
            "archived" => Ok(Self::Archived),
            _ => Err(TaskerError::InvalidInput {
                field: "feature_state".to_string(),
                message: value.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    #[default]
    Todo,
    InProgress,
    Blocked,
    Done,
    Cancelled,
}

impl TaskState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }

    pub const fn satisfies_dependency(self) -> bool {
        matches!(self, Self::Done)
    }

    pub const fn permits_execution(self) -> bool {
        matches!(self, Self::Todo)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (
                    Self::Todo,
                    Self::InProgress | Self::Blocked | Self::Cancelled
                ) | (
                    Self::InProgress,
                    Self::Blocked | Self::Done | Self::Cancelled
                ) | (
                    Self::Blocked,
                    Self::Todo | Self::InProgress | Self::Cancelled
                ) | (Self::Done | Self::Cancelled, Self::Todo)
            )
    }

    pub fn transition_to(self, next: Self) -> Result<Self, TaskerError> {
        self.can_transition_to(next)
            .then_some(next)
            .ok_or_else(|| TaskerError::invalid_task_transition(self, next))
    }
}

impl FromStr for TaskState {
    type Err = TaskerError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "todo" => Ok(Self::Todo),
            "in_progress" => Ok(Self::InProgress),
            "blocked" => Ok(Self::Blocked),
            "done" => Ok(Self::Done),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(TaskerError::InvalidInput {
                field: "task_state".to_string(),
                message: value.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

impl TaskPriority {
    pub const fn sort_value(self) -> i64 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
            Self::High => 2,
            Self::Critical => 3,
        }
    }

    pub const fn from_sort_value(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Low),
            1 => Some(Self::Normal),
            2 => Some(Self::High),
            3 => Some(Self::Critical),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub canonical_root: Option<String>,
    pub revision: ProjectRevision,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feature {
    pub id: FeatureId,
    pub project_id: ProjectId,
    pub alias: FeatureAlias,
    pub parent_id: Option<FeatureId>,
    pub title: String,
    pub description: String,
    pub state: FeatureState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub project_id: ProjectId,
    pub feature_id: FeatureId,
    pub alias: TaskAlias,
    pub title: String,
    pub description: String,
    pub state: TaskState,
    pub priority: TaskPriority,
    pub rank: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDependency {
    pub project_id: ProjectId,
    pub task_id: TaskId,
    pub depends_on_task_id: TaskId,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureDependency {
    pub project_id: ProjectId,
    pub feature_id: FeatureId,
    pub depends_on_feature_id: FeatureId,
    pub created_at: DateTime<Utc>,
}
