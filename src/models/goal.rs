use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::style;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use crate::db::atomic_write;
use crate::output::Render;

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
    total_tokens: i64,
    prompt_tokens: i64,
    completion_tokens: i64,
    elapsed_ms: i64,
    task_count: i64,
    tasks_completed: i64,
    tasks_failed: i64,
}

impl Metrics {
    pub fn new(
        total_tokens: i64,
        prompt_tokens: i64,
        completion_tokens: i64,
        elapsed_ms: i64,
        task_count: i64,
        tasks_completed: i64,
        tasks_failed: i64,
    ) -> Self {
        Self {
            total_tokens,
            prompt_tokens,
            completion_tokens,
            elapsed_ms,
            task_count,
            tasks_completed,
            tasks_failed,
        }
    }

    pub fn total_tokens(&self) -> i64 {
        self.total_tokens
    }

    pub fn prompt_tokens(&self) -> i64 {
        self.prompt_tokens
    }

    pub fn completion_tokens(&self) -> i64 {
        self.completion_tokens
    }

    pub fn elapsed_ms(&self) -> i64 {
        self.elapsed_ms
    }

    pub fn task_count(&self) -> i64 {
        self.task_count
    }

    pub fn tasks_completed(&self) -> i64 {
        self.tasks_completed
    }

    pub fn tasks_failed(&self) -> i64 {
        self.tasks_failed
    }
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
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<String>,
    description: String,
    state: GoalState,
    created_at: Timestamp,
    updated_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed_at: Option<Timestamp>,
    metrics: Metrics,
}

impl Goal {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        parent_id: Option<String>,
        description: String,
        state: GoalState,
        created_at: Timestamp,
        updated_at: Timestamp,
        completed_at: Option<Timestamp>,
        metrics: Metrics,
    ) -> Self {
        Self {
            id,
            parent_id,
            description,
            state,
            created_at,
            updated_at,
            completed_at,
            metrics,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn state(&self) -> GoalState {
        self.state
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }

    pub fn completed_at(&self) -> Option<Timestamp> {
        self.completed_at
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn touch(&mut self) {
        self.updated_at = Timestamp::now();
    }

    pub fn mark_in_progress(&mut self) {
        self.state = GoalState::InProgress;
        self.updated_at = Timestamp::now();
    }

    pub fn mark_completed(&mut self) {
        self.state = GoalState::Completed;
        let now = Timestamp::now();
        self.updated_at = now;
        self.completed_at = Some(now);
    }

    pub fn mark_failed(&mut self) {
        self.state = GoalState::Failed;
        self.updated_at = Timestamp::now();
    }

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
        writeln!(w, "  {}", &self.description)?;
        Ok(())
    }
}
