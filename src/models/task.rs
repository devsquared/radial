use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use super::{Comment, Contract, Outcome};
use crate::db::atomic_write;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, AsRefStr, EnumString)]
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

impl Task {
    pub fn file_path(&self, base: &Path) -> PathBuf {
        base.join(&self.goal_id).join(format!("{}.toml", self.id))
    }

    pub fn write_file(&self, base: &Path) -> Result<()> {
        let path = self.file_path(base);
        let content = toml::to_string(self).context("Failed to serialize task")?;
        atomic_write(&path, content.as_bytes())
    }

    pub fn transition(&mut self, from: TaskState, to: TaskState) -> bool {
        if self.state != from {
            return false;
        }
        self.state = to;
        self.updated_at = Timestamp::now();
        true
    }

    pub fn transition_from_any(&mut self, from: &[TaskState], to: TaskState) -> bool {
        if !from.contains(&self.state) {
            return false;
        }
        self.state = to;
        self.updated_at = Timestamp::now();
        true
    }

    pub fn complete(&mut self, outcome: Outcome, metrics: TaskMetrics) -> bool {
        if self.state != TaskState::InProgress {
            return false;
        }
        self.state = TaskState::Completed;
        self.result = Some(outcome);
        self.metrics = metrics;
        let now = Timestamp::now();
        self.updated_at = now;
        self.completed_at = Some(now);
        true
    }

    pub fn retry(&mut self) -> bool {
        if self.state != TaskState::Failed {
            return false;
        }
        self.state = TaskState::InProgress;
        self.metrics.retry_count += 1;
        self.updated_at = Timestamp::now();
        true
    }

    pub fn unblock(&mut self) {
        self.state = TaskState::Pending;
        self.updated_at = Timestamp::now();
    }

    pub fn add_comment(&mut self, comment: Comment) {
        self.comments.push(comment);
        self.updated_at = Timestamp::now();
    }
}
