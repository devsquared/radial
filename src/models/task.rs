use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use super::{Comment, Contract, Outcome};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, AsRefStr, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "snake_case")]
pub enum TaskState {
    Pending,
    Blocked,
    InProgress,
    Verifying,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskMetrics {
    pub tokens: i64,
    pub elapsed_ms: i64,
    pub retry_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub goal_id: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<Contract>,
    pub state: TaskState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Outcome>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    pub metrics: TaskMetrics,
    #[serde(default)]
    pub comments: Vec<Comment>,
}
