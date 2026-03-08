use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::style;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use super::{Comment, Contract, Outcome};
use crate::db::atomic_write;
use crate::output::Render;

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, AsRefStr, EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[derive(Default, clap::ValueEnum)]
pub enum Priority {
    P0,
    P1,
    #[default]
    P2,
    P3,
}

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
    tokens: i64,
    elapsed_ms: i64,
    retry_count: i64,
}

impl TaskMetrics {
    pub fn new(tokens: i64, elapsed_ms: i64, retry_count: i64) -> Self {
        Self {
            tokens,
            elapsed_ms,
            retry_count,
        }
    }

    pub fn tokens(&self) -> i64 {
        self.tokens
    }

    pub fn elapsed_ms(&self) -> i64 {
        self.elapsed_ms
    }

    pub fn retry_count(&self) -> i64 {
        self.retry_count
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    id: String,
    goal_id: String,
    description: String,
    #[serde(default)]
    priority: Priority,
    #[serde(skip_serializing_if = "Option::is_none")]
    contract: Option<Contract>,
    state: TaskState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blocked_by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    started_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Outcome>,
    created_at: Timestamp,
    updated_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed_at: Option<Timestamp>,
    metrics: TaskMetrics,
    #[serde(default)]
    comments: Vec<Comment>,
}

impl Task {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        goal_id: String,
        description: String,
        priority: Priority,
        contract: Option<Contract>,
        state: TaskState,
        blocked_by: Vec<String>,
        created_at: Timestamp,
        updated_at: Timestamp,
    ) -> Self {
        Self {
            id,
            goal_id,
            description,
            priority,
            contract,
            state,
            blocked_by,
            assignee: None,
            started_at: None,
            result: None,
            created_at,
            updated_at,
            completed_at: None,
            metrics: TaskMetrics::default(),
            comments: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_metrics(mut self, metrics: TaskMetrics) -> Self {
        self.metrics = metrics;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn goal_id(&self) -> &str {
        &self.goal_id
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn priority(&self) -> Priority {
        self.priority
    }

    pub fn contract(&self) -> Option<&Contract> {
        self.contract.as_ref()
    }

    pub fn state(&self) -> TaskState {
        self.state
    }

    pub fn blocked_by(&self) -> &[String] {
        &self.blocked_by
    }

    pub fn result(&self) -> Option<&Outcome> {
        self.result.as_ref()
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

    pub fn metrics(&self) -> &TaskMetrics {
        &self.metrics
    }

    pub fn comments(&self) -> &[Comment] {
        &self.comments
    }

    pub fn set_description(&mut self, description: String) {
        self.description = description;
        self.updated_at = Timestamp::now();
    }

    pub fn set_priority(&mut self, priority: Priority) {
        self.priority = priority;
        self.updated_at = Timestamp::now();
    }

    pub fn set_contract(&mut self, contract: Contract) {
        self.contract = Some(contract);
        self.updated_at = Timestamp::now();
    }

    pub fn set_blocked_by(&mut self, blocked_by: Vec<String>) {
        self.blocked_by = blocked_by;
        self.updated_at = Timestamp::now();
    }

    pub fn assignee(&self) -> Option<&str> {
        self.assignee.as_deref()
    }

    pub fn started_at(&self) -> Option<Timestamp> {
        self.started_at
    }

    pub fn set_assignee(&mut self, assignee: Option<String>) {
        self.assignee = assignee;
        self.updated_at = Timestamp::now();
    }

    pub fn release(&mut self) -> bool {
        if self.assignee.is_none() {
            return false;
        }
        self.assignee = None;
        self.started_at = None;
        self.state = TaskState::Pending;
        self.updated_at = Timestamp::now();
        true
    }

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
        let now = Timestamp::now();
        self.updated_at = now;
        if to == TaskState::InProgress {
            self.started_at = Some(now);
        }
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
        let now = Timestamp::now();
        self.updated_at = now;
        self.started_at = Some(now);
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

impl Render for Task {
    fn render(&self, w: &mut dyn Write) -> Result<()> {
        writeln!(
            w,
            "{} [{}] [{}]",
            style(&self.id).cyan().bold(),
            style(self.state.as_ref()).yellow(),
            self.priority.as_ref(),
        )?;
        writeln!(w, "  {}", &self.description)?;

        if let Some(assignee) = &self.assignee {
            writeln!(w, "  Assignee: {assignee}")?;
        }

        match self.contract {
            Some(ref contract) => {
                writeln!(w, "  Contract:")?;
                writeln!(w, "    Receives: {}", contract.receives())?;
                writeln!(w, "    Produces: {}", contract.produces())?;
                writeln!(w, "    Verify:   {}", contract.verify())?;
            }
            None => {
                writeln!(w, "  Contract: {}", style("(not set)").dim())?;
            }
        }

        if !self.blocked_by.is_empty() {
            writeln!(w, "  Blocked by: {}", self.blocked_by.join(", "))?;
        }

        if let Some(result) = &self.result {
            writeln!(w, "  Result: {}", result.summary())?;
            if !result.artifacts().is_empty() {
                writeln!(w, "  Artifacts: {}", result.artifacts().join(", "))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Render;
    use rstest::{fixture, rstest};

    #[fixture]
    fn task() -> Task {
        let now = Timestamp::now();
        Task {
            id: "t_abc123".to_string(),
            goal_id: "g_xyz789".to_string(),
            description: "test task".to_string(),
            priority: Priority::default(),
            contract: None,
            state: TaskState::Pending,
            blocked_by: Vec::new(),
            assignee: None,
            started_at: None,
            result: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
            metrics: TaskMetrics::default(),
            comments: Vec::new(),
        }
    }

    fn render_to_string(task: &Task) -> String {
        let mut buf = Vec::new();
        task.render(&mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    // -- transition --

    // transition() only succeeds when the task's current state matches `from`.
    // Cases where initial == from should succeed; mismatches should leave the
    // task unchanged with its original updated_at timestamp.
    #[rstest]
    #[case::matching_pending(TaskState::Pending, TaskState::Pending, TaskState::InProgress, true)]
    #[case::matching_in_progress(
        TaskState::InProgress,
        TaskState::InProgress,
        TaskState::Completed,
        true
    )]
    #[case::mismatch_completed(
        TaskState::Completed,
        TaskState::Pending,
        TaskState::InProgress,
        false
    )]
    #[case::mismatch_failed(TaskState::Failed, TaskState::Pending, TaskState::InProgress, false)]
    fn transition_checks_current_state(
        mut task: Task,
        #[case] initial: TaskState,
        #[case] from: TaskState,
        #[case] to: TaskState,
        #[case] expected: bool,
    ) {
        task.state = initial;
        let before = task.updated_at;
        let result = task.transition(from, to);
        assert_eq!(result, expected);
        if expected {
            assert_eq!(task.state, to);
            assert!(task.updated_at >= before);
        } else {
            assert_eq!(task.state, initial);
            assert_eq!(task.updated_at, before);
        }
    }

    // -- transition_from_any --

    // transition_from_any() accepts a list of valid source states.
    // Only states in the list should transition; others are rejected.
    #[rstest]
    #[case::in_progress_matches(TaskState::InProgress, true)]
    #[case::verifying_matches(TaskState::Verifying, true)]
    #[case::pending_rejected(TaskState::Pending, false)]
    #[case::completed_rejected(TaskState::Completed, false)]
    fn transition_from_any_matches_list(
        mut task: Task,
        #[case] current: TaskState,
        #[case] expected: bool,
    ) {
        task.state = current;
        let result = task.transition_from_any(
            &[TaskState::InProgress, TaskState::Verifying],
            TaskState::Failed,
        );
        assert_eq!(result, expected);
        if expected {
            assert_eq!(task.state, TaskState::Failed);
        } else {
            assert_eq!(task.state, current);
        }
    }

    // -- complete --

    // Completing an InProgress task should set state, result, metrics,
    // completed_at, and updated_at all in one shot.
    #[rstest]
    fn complete_sets_all_fields(mut task: Task) {
        task.state = TaskState::InProgress;
        let outcome = Outcome::new("done".to_string(), vec!["file.txt".to_string()]);
        let metrics = TaskMetrics::new(100, 5000, 1);

        assert!(task.complete(outcome, metrics));
        assert_eq!(task.state, TaskState::Completed);
        assert!(task.completed_at.is_some());
        assert_eq!(task.result.as_ref().unwrap().summary(), "done");
        assert_eq!(task.metrics.tokens, 100);
        assert_eq!(task.metrics.retry_count, 1);
    }

    // complete() is only valid from InProgress. Every other state should
    // be rejected, leaving the task untouched.
    #[rstest]
    #[case::from_pending(TaskState::Pending)]
    #[case::from_blocked(TaskState::Blocked)]
    #[case::from_completed(TaskState::Completed)]
    #[case::from_failed(TaskState::Failed)]
    fn complete_rejects_non_in_progress(mut task: Task, #[case] state: TaskState) {
        task.state = state;
        let outcome = Outcome::new("done".to_string(), Vec::new());
        assert!(!task.complete(outcome, TaskMetrics::default()));
        assert_eq!(task.state, state);
        assert!(task.completed_at.is_none());
    }

    // -- retry --

    // Retrying a failed task should move it back to InProgress and
    // bump the retry counter.
    #[rstest]
    fn retry_increments_and_transitions(mut task: Task) {
        task.state = TaskState::Failed;
        task.metrics.retry_count = 2;
        assert!(task.retry());
        assert_eq!(task.state, TaskState::InProgress);
        assert_eq!(task.metrics.retry_count, 3);
    }

    // retry() is only valid from Failed. Every other state should be rejected.
    #[rstest]
    #[case::from_pending(TaskState::Pending)]
    #[case::from_in_progress(TaskState::InProgress)]
    #[case::from_completed(TaskState::Completed)]
    #[case::from_blocked(TaskState::Blocked)]
    fn retry_rejects_non_failed(mut task: Task, #[case] state: TaskState) {
        task.state = state;
        assert!(!task.retry());
        assert_eq!(task.state, state);
    }

    // -- unblock --

    // Unblocking sets the task to Pending unconditionally and bumps updated_at.
    #[rstest]
    fn unblock_sets_pending(mut task: Task) {
        task.state = TaskState::Blocked;
        let before = task.updated_at;
        task.unblock();
        assert_eq!(task.state, TaskState::Pending);
        assert!(task.updated_at >= before);
    }

    // -- add_comment --

    // Adding a comment should append to the list and bump updated_at.
    #[rstest]
    fn add_comment_appends_and_updates_timestamp(mut task: Task) {
        let before = task.updated_at;
        let comment = Comment::new("c_1".to_string(), "hello".to_string(), Timestamp::now());
        task.add_comment(comment);

        assert_eq!(task.comments.len(), 1);
        assert_eq!(task.comments[0].text(), "hello");
        assert!(task.updated_at >= before);
    }

    // -- file_path --

    // Task files live at {base}/{goal_id}/{task_id}.toml.
    #[rstest]
    fn file_path_is_correct(task: Task) {
        let path = task.file_path(Path::new("/tmp/.radial"));
        assert_eq!(path, PathBuf::from("/tmp/.radial/g_xyz789/t_abc123.toml"));
    }

    // -- render --

    // The summary render should show the description and indicate
    // missing contract with "(not set)".
    #[rstest]
    fn render_includes_description(task: Task) {
        let output = render_to_string(&task);
        assert!(output.contains("test task"));
        assert!(output.contains("(not set)"));
    }

    // When a contract is present, all three fields should appear.
    #[rstest]
    fn render_includes_contract_fields(mut task: Task) {
        task.contract = Some(Contract::new(
            "input data".to_string(),
            "output data".to_string(),
            "check output".to_string(),
        ));
        let output = render_to_string(&task);
        assert!(output.contains("input data"));
        assert!(output.contains("output data"));
        assert!(output.contains("check output"));
    }

    // Blocked tasks should show which task IDs they're waiting on.
    #[rstest]
    fn render_includes_blocked_by(mut task: Task) {
        task.state = TaskState::Blocked;
        task.blocked_by = vec!["t_other".to_string()];
        let output = render_to_string(&task);
        assert!(output.contains("Blocked by: t_other"));
    }

    // Completed tasks should show the result summary and artifact list.
    #[rstest]
    fn render_includes_result(mut task: Task) {
        task.state = TaskState::Completed;
        task.result = Some(Outcome::new(
            "all good".to_string(),
            vec!["out.txt".to_string()],
        ));
        let output = render_to_string(&task);
        assert!(output.contains("all good"));
        assert!(output.contains("out.txt"));
    }

    // -- assignee --

    // Setting an assignee should store the value and bump updated_at.
    #[rstest]
    fn set_assignee_stores_value(mut task: Task) {
        let before = task.updated_at;
        task.set_assignee(Some("agent-1".to_string()));
        assert_eq!(task.assignee(), Some("agent-1"));
        assert!(task.updated_at >= before);
    }

    // Clearing an assignee should set it to None.
    #[rstest]
    fn set_assignee_clears_value(mut task: Task) {
        task.assignee = Some("agent-1".to_string());
        task.set_assignee(None);
        assert_eq!(task.assignee(), None);
    }

    // -- release --

    // Releasing an assigned in-progress task should clear assignee and set Pending.
    #[rstest]
    fn release_clears_assignee_and_sets_pending(mut task: Task) {
        task.state = TaskState::InProgress;
        task.assignee = Some("agent-1".to_string());
        let before = task.updated_at;
        assert!(task.release());
        assert_eq!(task.state, TaskState::Pending);
        assert_eq!(task.assignee(), None);
        assert!(task.updated_at >= before);
    }

    // Releasing a failed task with an assignee should also work.
    #[rstest]
    fn release_works_from_failed(mut task: Task) {
        task.state = TaskState::Failed;
        task.assignee = Some("agent-1".to_string());
        assert!(task.release());
        assert_eq!(task.state, TaskState::Pending);
        assert_eq!(task.assignee(), None);
    }

    // Releasing a task with no assignee should fail.
    #[rstest]
    fn release_rejects_unassigned(mut task: Task) {
        task.state = TaskState::InProgress;
        assert!(!task.release());
        assert_eq!(task.state, TaskState::InProgress);
    }

    // Render should include assignee when set.
    #[rstest]
    fn render_includes_assignee(mut task: Task) {
        task.assignee = Some("agent-1".to_string());
        let output = render_to_string(&task);
        assert!(output.contains("Assignee: agent-1"));
    }

    // -- started_at --

    // Transitioning to InProgress should set started_at.
    #[rstest]
    fn transition_to_in_progress_sets_started_at(mut task: Task) {
        assert!(task.started_at.is_none());
        task.transition(TaskState::Pending, TaskState::InProgress);
        assert!(task.started_at.is_some());
    }

    // Transitioning to a non-InProgress state should not set started_at.
    #[rstest]
    fn transition_to_other_state_does_not_set_started_at(mut task: Task) {
        task.state = TaskState::InProgress;
        task.transition(TaskState::InProgress, TaskState::Completed);
        assert!(task.started_at.is_none());
    }

    // Retrying a failed task should set started_at.
    #[rstest]
    fn retry_sets_started_at(mut task: Task) {
        task.state = TaskState::Failed;
        assert!(task.started_at.is_none());
        task.retry();
        assert!(task.started_at.is_some());
    }

    // Releasing a task should clear started_at.
    #[rstest]
    fn release_clears_started_at(mut task: Task) {
        task.state = TaskState::InProgress;
        task.assignee = Some("agent-1".to_string());
        task.started_at = Some(Timestamp::now());
        task.release();
        assert!(task.started_at.is_none());
    }

    // Completing a task should preserve started_at.
    #[rstest]
    fn complete_preserves_started_at(mut task: Task) {
        task.state = TaskState::InProgress;
        let ts = Timestamp::now();
        task.started_at = Some(ts);
        let outcome = Outcome::new("done".to_string(), Vec::new());
        task.complete(outcome, TaskMetrics::default());
        assert_eq!(task.started_at, Some(ts));
    }
}
