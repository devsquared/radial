use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use crate::id::GoalId;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, AsRefStr, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "snake_case")]
#[non_exhaustive]
pub enum GoalState {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Metrics {
    total_tokens: i64,
    prompt_tokens: i64,
    completion_tokens: i64,
    elapsed_ms: i64,
    task_count: i64,
    tasks_completed: i64,
    tasks_failed: i64,
    tasks_cancelled: i64,
}

impl Metrics {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        total_tokens: i64,
        prompt_tokens: i64,
        completion_tokens: i64,
        elapsed_ms: i64,
        task_count: i64,
        tasks_completed: i64,
        tasks_failed: i64,
        tasks_cancelled: i64,
    ) -> Self {
        Self {
            total_tokens,
            prompt_tokens,
            completion_tokens,
            elapsed_ms,
            task_count,
            tasks_completed,
            tasks_failed,
            tasks_cancelled,
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

    pub fn tasks_cancelled(&self) -> i64 {
        self.tasks_cancelled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    id: GoalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seq: Option<u32>,
    #[serde(
        rename = "ref",
        skip_deserializing,
        skip_serializing_if = "Option::is_none"
    )]
    display_ref_field: Option<String>,
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
    pub(crate) fn new(
        id: GoalId,
        seq: Option<u32>,
        description: String,
        state: GoalState,
        created_at: Timestamp,
        updated_at: Timestamp,
        completed_at: Option<Timestamp>,
        metrics: Metrics,
    ) -> Self {
        let display_ref_field = seq.map(|s| format!("g{s}"));
        Self {
            id,
            seq,
            display_ref_field,
            description,
            state,
            created_at,
            updated_at,
            completed_at,
            metrics,
        }
    }

    pub fn id(&self) -> &GoalId {
        &self.id
    }

    pub fn seq(&self) -> Option<u32> {
        self.seq
    }

    /// Returns the display reference for this goal (e.g., "g1").
    /// Returns None if seq is not assigned (legacy records).
    pub fn display_ref(&self) -> Option<String> {
        self.display_ref_field.clone()
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

    pub(crate) fn set_metrics(&mut self, metrics: Metrics) {
        self.metrics = metrics;
    }

    pub(crate) fn set_description(&mut self, description: String) {
        self.description = description;
        self.updated_at = Timestamp::now();
    }

    pub(crate) fn touch(&mut self) {
        self.updated_at = Timestamp::now();
    }

    /// Compute and set the `display_ref_field` based on the `seq`.
    /// Called after deserialization to populate the computed field.
    pub(crate) fn compute_display_ref(&mut self) {
        self.display_ref_field = self.seq.map(|s| format!("g{s}"));
    }

    pub(crate) fn mark_in_progress(&mut self) {
        self.state = GoalState::InProgress;
        self.updated_at = Timestamp::now();
    }

    pub(crate) fn mark_completed(&mut self) {
        self.state = GoalState::Completed;
        let now = Timestamp::now();
        self.updated_at = now;
        self.completed_at = Some(now);
    }

    pub(crate) fn mark_failed(&mut self) {
        self.state = GoalState::Failed;
        self.updated_at = Timestamp::now();
    }

    pub(crate) fn mark_cancelled(&mut self) {
        self.state = GoalState::Cancelled;
        self.updated_at = Timestamp::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_ref_with_seq() {
        let goal = Goal::new(
            GoalId::new_unchecked("test123".to_string()),
            Some(5),
            "Test goal".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        assert_eq!(goal.display_ref(), Some("g5".to_string()));
    }

    #[test]
    fn display_ref_without_seq() {
        let goal = Goal::new(
            GoalId::new_unchecked("test123".to_string()),
            None,
            "Test goal".to_string(),
            GoalState::Pending,
            Timestamp::now(),
            Timestamp::now(),
            None,
            Metrics::default(),
        );
        assert_eq!(goal.display_ref(), None);
    }

    // Goal files written before a counter existed omit its key entirely. Parsing
    // must fall back to the default rather than failing, because the database
    // loads every goal up front and one stale file would take down all commands.
    #[test]
    fn metrics_parses_without_tasks_cancelled() {
        let legacy = "
total_tokens = 10
prompt_tokens = 4
completion_tokens = 6
elapsed_ms = 250
task_count = 3
tasks_completed = 2
tasks_failed = 1
";
        let metrics: Metrics = toml::from_str(legacy).expect("legacy metrics should parse");
        assert_eq!(metrics.tasks_cancelled(), 0);
        assert_eq!(metrics.task_count(), 3);
        assert_eq!(metrics.tasks_completed(), 2);
        assert_eq!(metrics.tasks_failed(), 1);
    }

    #[test]
    fn goal_parses_with_legacy_metrics_table() {
        let legacy = r#"
id = "abc12345"
description = "legacy goal"
state = "inprogress"
created_at = "2026-04-05T19:03:40.353107Z"
updated_at = "2026-04-05T19:03:40.353107Z"

[metrics]
total_tokens = 0
prompt_tokens = 0
completion_tokens = 0
elapsed_ms = 0
task_count = 0
tasks_completed = 0
tasks_failed = 0
"#;
        let goal: Goal = toml::from_str(legacy).expect("legacy goal should parse");
        assert_eq!(goal.description(), "legacy goal");
        assert_eq!(goal.metrics().tasks_cancelled(), 0);
    }
}
