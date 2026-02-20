use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use crate::db::atomic_write;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, AsRefStr, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "snake_case")]
pub enum GoalState {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metrics {
    pub total_tokens: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub elapsed_ms: i64,
    pub task_count: i64,
    pub tasks_completed: i64,
    pub tasks_failed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub description: String,
    pub state: GoalState,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    pub metrics: Metrics,
}

impl Goal {
    pub fn file_path(&self, base: &Path) -> PathBuf {
        base.join(&self.id).join("goal.toml")
    }

    pub fn write_file(&self, base: &Path) -> Result<()> {
        let path = self.file_path(base);
        let content = toml::to_string(self).context("Failed to serialize goal")?;
        atomic_write(&path, content.as_bytes())
    }
}
