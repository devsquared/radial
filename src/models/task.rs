use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use super::{Comment, Contract, Outcome};
use crate::id::{GoalId, TaskId};

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, AsRefStr, EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[derive(Default, clap::ValueEnum)]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum TaskState {
    Pending,
    Blocked,
    InProgress,
    Verifying,
    Completed,
    Failed,
    Cancelled,
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
    id: TaskId,
    goal_id: GoalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seq: Option<u32>,
    #[serde(
        rename = "ref",
        skip_deserializing,
        skip_serializing_if = "Option::is_none"
    )]
    display_ref_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<TaskId>,
    description: String,
    #[serde(default)]
    priority: Priority,
    #[serde(skip_serializing_if = "Option::is_none")]
    contract: Option<Contract>,
    state: TaskState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blocked_by: Vec<TaskId>,
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
    #[serde(default)]
    compacted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

impl Task {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: TaskId,
        goal_id: GoalId,
        seq: Option<u32>,
        parent_id: Option<TaskId>,
        description: String,
        priority: Priority,
        contract: Option<Contract>,
        state: TaskState,
        blocked_by: Vec<TaskId>,
        created_at: Timestamp,
        updated_at: Timestamp,
    ) -> Self {
        Self {
            id,
            goal_id,
            seq,
            display_ref_field: None, // Will be computed after loading
            parent_id,
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
            compacted: false,
            summary: None,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_metrics(mut self, metrics: TaskMetrics) -> Self {
        self.metrics = metrics;
        self
    }

    pub fn id(&self) -> &TaskId {
        &self.id
    }

    pub fn goal_id(&self) -> &GoalId {
        &self.goal_id
    }

    pub fn seq(&self) -> Option<u32> {
        self.seq
    }

    /// Returns the display reference for this task (e.g., "g1.2").
    /// Returns None if either this task's seq or the goal's seq is not assigned.
    pub fn display_ref(&self, goal_seq: u32) -> Option<String> {
        // If cached ref is available, use it; otherwise compute it
        self.display_ref_field
            .clone()
            .or_else(|| self.seq.map(|s| format!("g{goal_seq}.{s}")))
    }

    /// Compute and set the `display_ref_field` based on the `seq` and `goal_seq`.
    /// Called after deserialization to populate the computed field.
    pub(crate) fn compute_display_ref(&mut self, goal_seq: u32) {
        self.display_ref_field = self.seq.map(|s| format!("g{goal_seq}.{s}"));
    }

    pub fn parent_id(&self) -> Option<&TaskId> {
        self.parent_id.as_ref()
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

    pub fn blocked_by(&self) -> &[TaskId] {
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

    pub fn compacted(&self) -> bool {
        self.compacted
    }

    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// Compact this task by replacing heavy fields with a summary.
    /// Only valid for Completed or Failed tasks that aren't already compacted.
    pub(crate) fn compact(&mut self, summary: String) -> bool {
        if self.compacted {
            return false;
        }
        if self.state != TaskState::Completed && self.state != TaskState::Failed {
            return false;
        }
        self.compacted = true;
        self.summary = Some(summary);
        self.description = "[compacted]".to_string();
        self.contract = None;
        self.comments = Vec::new();
        self.updated_at = Timestamp::now();
        true
    }

    /// Set state directly for parent tasks whose state is derived from subtasks.
    pub(crate) fn set_derived_state(&mut self, state: TaskState) {
        self.state = state;
        let now = Timestamp::now();
        self.updated_at = now;
        if state == TaskState::Completed && self.completed_at.is_none() {
            self.completed_at = Some(now);
        }
    }

    pub(crate) fn set_description(&mut self, description: String) {
        self.description = description;
        self.updated_at = Timestamp::now();
    }

    pub(crate) fn set_priority(&mut self, priority: Priority) {
        self.priority = priority;
        self.updated_at = Timestamp::now();
    }

    pub(crate) fn set_contract(&mut self, contract: Contract) {
        self.contract = Some(contract);
        self.updated_at = Timestamp::now();
    }

    pub(crate) fn set_blocked_by(&mut self, blocked_by: Vec<TaskId>) {
        self.blocked_by = blocked_by;
        self.updated_at = Timestamp::now();
    }

    pub fn assignee(&self) -> Option<&str> {
        self.assignee.as_deref()
    }

    pub fn started_at(&self) -> Option<Timestamp> {
        self.started_at
    }

    pub(crate) fn set_assignee(&mut self, assignee: Option<String>) {
        self.assignee = assignee;
        self.updated_at = Timestamp::now();
    }

    pub(crate) fn release(&mut self) -> bool {
        if self.assignee.is_none() {
            return false;
        }
        self.assignee = None;
        self.started_at = None;
        self.state = TaskState::Pending;
        self.updated_at = Timestamp::now();
        true
    }

    pub(crate) fn transition_from_any(&mut self, from: &[TaskState], to: TaskState) -> bool {
        if !from.contains(&self.state) {
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

    pub(crate) fn complete(&mut self, outcome: Outcome, metrics: TaskMetrics) -> bool {
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

    pub(crate) fn retry(&mut self) -> bool {
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

    pub(crate) fn cancel(&mut self) -> bool {
        // Cancelled is terminal - already cancelled tasks cannot be re-cancelled
        if self.state == TaskState::Cancelled {
            return false;
        }
        // Completed work is history - cannot cancel completed tasks
        if self.state == TaskState::Completed {
            return false;
        }
        // All other states (Pending, Blocked, InProgress, Verifying, Failed) can transition to Cancelled
        self.state = TaskState::Cancelled;
        self.updated_at = Timestamp::now();
        true
    }

    pub(crate) fn unblock(&mut self) {
        self.state = TaskState::Pending;
        self.updated_at = Timestamp::now();
    }

    pub(crate) fn add_comment(&mut self, comment: Comment) {
        self.comments.push(comment);
        self.updated_at = Timestamp::now();
    }

    pub(crate) fn set_result(&mut self, outcome: Outcome) {
        self.result = Some(outcome);
        self.updated_at = Timestamp::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{GoalId, TaskId};
    use rstest::{fixture, rstest};

    #[fixture]
    fn task() -> Task {
        let now = Timestamp::now();
        Task {
            id: TaskId::new_unchecked("t_abc123".to_string()),
            goal_id: GoalId::new_unchecked("g_xyz789".to_string()),
            seq: None,
            display_ref_field: None,
            parent_id: None,
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
            compacted: false,
            summary: None,
        }
    }

    // -- transition_from_any --

    // transition_from_any() only succeeds when the task's current state is in
    // `from`. Cases where initial is in `from` should succeed; mismatches
    // should leave the task unchanged with its original updated_at timestamp.
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
    fn transition_from_any_checks_current_state(
        mut task: Task,
        #[case] initial: TaskState,
        #[case] from: TaskState,
        #[case] to: TaskState,
        #[case] expected: bool,
    ) {
        task.state = initial;
        let before = task.updated_at;
        let result = task.transition_from_any(&[from], to);
        assert_eq!(result, expected);
        if expected {
            assert_eq!(task.state, to);
            assert!(task.updated_at >= before);
        } else {
            assert_eq!(task.state, initial);
            assert_eq!(task.updated_at, before);
        }
    }

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

    // -- cancel --

    // Cancelling from valid states (Pending, Blocked, InProgress, Verifying, Failed)
    // should transition to Cancelled and update timestamp.
    #[rstest]
    #[case::from_pending(TaskState::Pending)]
    #[case::from_blocked(TaskState::Blocked)]
    #[case::from_in_progress(TaskState::InProgress)]
    #[case::from_verifying(TaskState::Verifying)]
    #[case::from_failed(TaskState::Failed)]
    fn cancel_succeeds_from_valid_states(mut task: Task, #[case] state: TaskState) {
        task.state = state;
        let before = task.updated_at;
        assert!(task.cancel());
        assert_eq!(task.state, TaskState::Cancelled);
        assert!(task.updated_at >= before);
    }

    // cancel() rejects Completed tasks (completed work is history).
    #[rstest]
    fn cancel_rejects_completed(mut task: Task) {
        task.state = TaskState::Completed;
        assert!(!task.cancel());
        assert_eq!(task.state, TaskState::Completed);
    }

    // cancel() rejects already-Cancelled tasks (idempotency check).
    #[rstest]
    fn cancel_rejects_already_cancelled(mut task: Task) {
        task.state = TaskState::Cancelled;
        let before = task.updated_at;
        assert!(!task.cancel());
        assert_eq!(task.state, TaskState::Cancelled);
        assert_eq!(task.updated_at, before); // No timestamp change
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

    // -- started_at --

    // Transitioning to InProgress should set started_at.
    #[rstest]
    fn transition_to_in_progress_sets_started_at(mut task: Task) {
        assert!(task.started_at.is_none());
        task.transition_from_any(&[TaskState::Pending], TaskState::InProgress);
        assert!(task.started_at.is_some());
    }

    // Transitioning to a non-InProgress state should not set started_at.
    #[rstest]
    fn transition_to_other_state_does_not_set_started_at(mut task: Task) {
        task.state = TaskState::InProgress;
        task.transition_from_any(&[TaskState::InProgress], TaskState::Completed);
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

    // -- compact --

    // Compacting a completed task should set compacted=true, store the summary,
    // replace description, clear contract and comments, and bump updated_at.
    #[rstest]
    fn compact_completed_task(mut task: Task) {
        task.state = TaskState::Completed;
        task.contract = Some(Contract::new(
            "input".to_string(),
            "output".to_string(),
            "verify".to_string(),
        ));
        task.comments = vec![Comment::new(
            "c1".to_string(),
            "a comment".to_string(),
            Timestamp::now(),
        )];
        let before = task.updated_at;

        assert!(task.compact("Summarized the task.".to_string()));
        assert!(task.compacted);
        assert_eq!(task.summary.as_deref(), Some("Summarized the task."));
        assert_eq!(task.description, "[compacted]");
        assert!(task.contract.is_none());
        assert!(task.comments.is_empty());
        assert!(task.updated_at >= before);
    }

    // Compacting a failed task should also work.
    #[rstest]
    fn compact_failed_task(mut task: Task) {
        task.state = TaskState::Failed;
        assert!(task.compact("Failed task summary.".to_string()));
        assert!(task.compacted);
        assert_eq!(task.summary.as_deref(), Some("Failed task summary."));
    }

    // Compacting a pending task should be rejected.
    #[rstest]
    #[case::pending(TaskState::Pending)]
    #[case::blocked(TaskState::Blocked)]
    #[case::in_progress(TaskState::InProgress)]
    fn compact_rejects_active_states(mut task: Task, #[case] state: TaskState) {
        task.state = state;
        assert!(!task.compact("nope".to_string()));
        assert!(!task.compacted);
        assert!(task.summary.is_none());
    }

    // Compacting an already-compacted task should be rejected.
    #[rstest]
    fn compact_rejects_already_compacted(mut task: Task) {
        task.state = TaskState::Completed;
        assert!(task.compact("first".to_string()));
        assert!(!task.compact("second".to_string()));
        assert_eq!(task.summary.as_deref(), Some("first"));
    }

    #[test]
    fn display_ref_with_seq() {
        let mut task = task();
        task.seq = Some(3);
        assert_eq!(task.display_ref(5), Some("g5.3".to_string()));
    }

    #[test]
    fn display_ref_without_seq() {
        let task = task();
        assert_eq!(task.display_ref(5), None);
    }
}
