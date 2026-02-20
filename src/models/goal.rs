use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::style;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use crate::db::atomic_write;
use crate::output::{Render, write_field};

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

impl Render for Metrics {
    fn render(&self, w: &mut dyn Write) -> Result<()> {
        writeln!(
            w,
            "  Tasks: {} total, {} completed, {} failed",
            self.task_count, self.tasks_completed, self.tasks_failed
        )?;
        writeln!(w, "  Tokens: {}", self.total_tokens)?;
        writeln!(w, "  Elapsed: {}ms", self.elapsed_ms)?;
        Ok(())
    }
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

impl Render for Goal {
    fn render(&self, w: &mut dyn Write) -> Result<()> {
        writeln!(
            w,
            "{} [{}]",
            style(&self.id).cyan().bold(),
            style(self.state.as_ref()).yellow()
        )?;
        write_field(w, "  ", "Description", &self.description)?;
        Ok(())
    }
}
