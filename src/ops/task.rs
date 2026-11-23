use std::collections::{BTreeMap, HashSet, VecDeque};

use anyhow::{Result, anyhow};
use jiff::{SignedDuration, Timestamp};
use serde::Serialize;

use crate::db::Database;
use crate::helpers::find_similar_id;
use crate::id::{GoalId, TaskId, generate_id};
use crate::models::{
    Comment, Contract, GoalState, Outcome, Priority, Task, TaskMetrics, TaskState,
};

/// Result of completing a task, including any unblocked tasks.
#[derive(Debug, Serialize)]
pub struct CompleteResult {
    /// The completed task.
    pub task: Task,
    /// IDs of tasks that became unblocked as a result.
    pub unblocked_task_ids: Vec<TaskId>,
}

/// Result of cancelling a task, including unblocked and cascaded tasks.
#[derive(Debug, Serialize)]
pub struct CancelResult {
    /// The cancelled task.
    pub task: Task,
    /// IDs of tasks that became unblocked as a result.
    pub unblocked_task_ids: Vec<TaskId>,
    /// IDs of downstream tasks also cancelled, if cascading was requested.
    pub cascaded_task_ids: Vec<TaskId>,
}

fn task_not_found_err(task_id: &TaskId, db: &Database) -> anyhow::Error {
    let all_task_ids: Vec<&str> = db
        .list_goals()
        .iter()
        .flat_map(|goal| {
            db.list_tasks(goal.id())
                .into_iter()
                .map(|t| t.id().as_ref())
        })
        .collect();

    if let Some(suggestion) = find_similar_id(task_id.as_ref(), &all_task_ids) {
        anyhow!("Task not found: {task_id}\nDid you mean: {suggestion}")
    } else {
        anyhow!("Task not found: {task_id}")
    }
}

/// Builds the "has subtasks" error for an op that refuses to act on a
/// parent directly, naming every subtask (and its state) so the caller can
/// decide whether to resolve them individually or, if `suggest_force` is
/// set, pass `--force` to run `verb` on the parent anyway.
fn subtasks_block_err(
    task_id: &TaskId,
    verb: &str,
    suggest_force: bool,
    db: &Database,
) -> anyhow::Error {
    let subtasks = db.list_subtasks(task_id);
    let ids: Vec<String> = subtasks
        .iter()
        .map(|t| format!("{} [{}]", t.id(), t.state().as_ref()))
        .collect();

    let force_hint = if suggest_force {
        format!(
            " Or pass --force to run '{verb}' on {task_id} directly anyway (recorded as a comment on it)."
        )
    } else {
        String::new()
    };
    anyhow!(
        "Task {task_id} has {} subtask(s): {}.\nRun '{verb}' on them individually instead.{force_hint}",
        subtasks.len(),
        ids.join(", "),
    )
}

/// Builds the comment text recording that `verb` was run on a parent task
/// directly via `--force` despite open subtasks.
fn force_override_comment_text(task_id: &TaskId, verb: &str, db: &Database) -> String {
    let total = db.list_subtasks(task_id).len();
    let open = db
        .list_subtasks(task_id)
        .iter()
        .filter(|t| !matches!(t.state(), TaskState::Completed | TaskState::Cancelled))
        .count();
    format!(
        "--force override: ran '{verb}' on this task directly despite {open} open subtask(s) ({total} total)."
    )
}

/// Records a `--force` override comment on a parent task, so it's visible
/// in its history.
///
/// Note: this sets the parent's state explicitly, but a later state change
/// on any of its subtasks triggers `sync_parent_state`, which recomputes the
/// parent's state from its subtasks and can silently overwrite this one.
fn record_force_override(task_id: &TaskId, verb: &str, db: &mut Database) -> Result<()> {
    let text = force_override_comment_text(task_id, verb, db);
    let task = db
        .get_task_mut(task_id)
        .ok_or_else(|| anyhow!("Task not found: {task_id}"))?;
    task.add_comment(Comment::new(generate_id(), text, Timestamp::now()));
    Ok(())
}

/// Create a new task under a goal.
///
/// Validates the parent task (if any) and the `blocked_by` IDs (if any)
/// against the goal's existing tasks, builds a contract from the given
/// `receives`/`produces`/`verify` strings if any are set, and starts the
/// task `Pending` or `Blocked` depending on whether its blockers are already
/// complete.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn create(
    goal_id: &GoalId,
    description: String,
    priority: Priority,
    parent_id: Option<TaskId>,
    receives: Option<String>,
    produces: Option<String>,
    verify: Option<String>,
    blocked_by: Option<Vec<TaskId>>,
    tags: BTreeMap<String, String>,
    db: &mut Database,
) -> Result<Task> {
    let goal = db.get_goal(goal_id);

    if goal.is_none() {
        let all_goals = db.list_goals();
        let goal_ids: Vec<&str> = all_goals.iter().map(|g| g.id().as_ref()).collect();

        return if let Some(suggestion) = find_similar_id(goal_id.as_ref(), &goal_ids) {
            Err(anyhow!(
                "Goal not found: {goal_id}\nDid you mean: {suggestion}"
            ))
        } else {
            Err(anyhow!("Goal not found: {goal_id}"))
        };
    }

    let goal = goal.unwrap();
    let goal_id_owned = goal.id().clone();

    // Validate parent task if provided
    if let Some(ref pid) = parent_id {
        let parent = db
            .get_task(pid)
            .ok_or_else(|| anyhow!("Parent task not found: {pid}"))?;

        if parent.goal_id() != &goal_id_owned {
            return Err(anyhow!("Parent task {pid} belongs to a different goal"));
        }

        if parent.parent_id().is_some() {
            return Err(anyhow!(
                "Cannot create a subtask of a subtask: {pid} is already a subtask"
            ));
        }

        let parent_state = parent.state();
        if parent_state == TaskState::Completed || parent_state == TaskState::Failed {
            return Err(anyhow!(
                "Cannot add subtasks to a {} task",
                parent_state.as_ref()
            ));
        }
    }

    // Validate blocked_by task IDs exist
    if let Some(ref task_ids) = blocked_by {
        let all_tasks = db.list_tasks(&goal_id_owned);
        let existing_task_ids: Vec<&str> = all_tasks.iter().map(|t| t.id().as_ref()).collect();

        for task_id in task_ids {
            if !existing_task_ids.contains(&task_id.as_ref()) {
                return if let Some(suggestion) =
                    find_similar_id(task_id.as_ref(), &existing_task_ids)
                {
                    Err(anyhow!(
                        "Task not found in blocked-by list: {task_id}\nDid you mean: {suggestion}"
                    ))
                } else {
                    Err(anyhow!(
                        "Task not found in blocked-by list: {task_id}\nTask must exist in the same goal."
                    ))
                };
            }
        }
    }

    // Build contract if any contract fields are provided
    let contract = if receives.is_some() || produces.is_some() || verify.is_some() {
        Some(Contract::new(
            receives.unwrap_or_default(),
            produces.unwrap_or_default(),
            verify.unwrap_or_default(),
        ))
    } else {
        None
    };

    // Filter out already-completed blockers so a task created after its
    // blockers finish starts as pending rather than permanently blocked.
    let all_goal_tasks = db.list_tasks(&goal_id_owned);
    let blocked_by_ids: Vec<TaskId> = blocked_by
        .unwrap_or_default()
        .into_iter()
        .filter(|id| {
            all_goal_tasks
                .iter()
                .find(|t| t.id() == id)
                .is_none_or(|t| t.state() != TaskState::Completed)
        })
        .collect();
    let state = if blocked_by_ids.is_empty() {
        TaskState::Pending
    } else {
        TaskState::Blocked
    };
    let now = Timestamp::now();
    let seq = db.next_task_seq(&goal_id_owned);
    let mut task = Task::new(
        TaskId::new(),
        goal_id_owned.clone(),
        Some(seq),
        parent_id.clone(),
        description,
        priority,
        contract,
        state,
        blocked_by_ids,
        now,
        now,
    );
    task.merge_tags(tags);

    db.create_task(task.clone())?;

    // Sync parent state now that it has a new subtask
    if let Some(ref pid) = parent_id {
        db.sync_parent_state(pid)?;
    }

    // Update the goal
    let goal = db.get_goal_mut(&goal_id_owned).unwrap();
    goal.touch();
    db.refresh_goal_metrics(&goal_id_owned)?;

    Ok(task)
}

/// List a goal's tasks, optionally filtered by priority, assignee, and/or
/// tags, sorted by priority.
pub fn list(
    goal_id: &GoalId,
    priority: Option<&Priority>,
    assignee: Option<&str>,
    tag_filter: &BTreeMap<String, String>,
    db: &Database,
) -> Result<Vec<Task>> {
    db.get_goal(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    let mut tasks: Vec<Task> = db
        .list_tasks(goal_id)
        .into_iter()
        .filter(|t| priority.is_none_or(|p| t.priority() == *p))
        .filter(|t| assignee.is_none_or(|a| t.assignee() == Some(a)))
        .filter(|t| tags_matching(t, tag_filter))
        .cloned()
        .collect();
    tasks.sort_by_key(Task::priority);
    Ok(tasks)
}

/// Upsert tags onto a task, overwriting any keys already present and
/// leaving the rest of its tag set untouched.
pub fn set_tags(
    task_id: &TaskId,
    tags: BTreeMap<String, String>,
    db: &mut Database,
) -> Result<Task> {
    if db.get_task(task_id).is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = db.get_task_mut(task_id).unwrap();
    task.merge_tags(tags);
    let task = task.clone();
    db.save_task(&task)?;
    Ok(task)
}

/// Whether a task's tags satisfy every key-value pair in `filter` (an empty
/// filter matches everything).
pub fn tags_matching(task: &Task, filter: &BTreeMap<String, String>) -> bool {
    filter.iter().all(|(k, v)| task.tags().get(k) == Some(v))
}

/// Start a task: assign it and transition it to `InProgress`.
///
/// Fails if the task has subtasks, has no contract, or is not
/// `Pending`/`Blocked` (unless `force` overrides an outstanding blocker — its
/// own, or one inherited from an unfinished parent). Also transitions the
/// parent goal to `InProgress` on its first task start, and syncs the
/// parent task's derived state so a parent reflects `InProgress` as soon as
/// any of its subtasks does.
pub fn start(task_id: &TaskId, assignee: &str, force: bool, db: &mut Database) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) {
        return Err(subtasks_block_err(task_id, "start", false, db));
    }

    if task.contract().is_none() {
        return Err(anyhow!(
            "Task has no contract. Set a contract before starting.\nUse: radial edit task {} --receives \"...\" --produces \"...\" --verify \"...\"",
            task.id()
        ));
    }

    let effective = db.effective_blockers(task);
    if !force && !effective.is_empty() {
        let own: Vec<&str> = effective
            .iter()
            .filter(|id| task.blocked_by().contains(id))
            .map(AsRef::as_ref)
            .collect();
        let inherited: Vec<&str> = effective
            .iter()
            .filter(|id| !task.blocked_by().contains(id))
            .map(AsRef::as_ref)
            .collect();

        let own_line = if own.is_empty() {
            String::new()
        } else {
            format!("Task is blocked by: {}\n", own.join(", "))
        };
        let inherited_line = if inherited.is_empty() {
            String::new()
        } else {
            let pid = task.parent_id().map(AsRef::as_ref).unwrap_or_default();
            format!(
                "Task inherits unfinished blocker(s) from parent {pid}: {}\n",
                inherited.join(", ")
            )
        };
        let msg = format!(
            "{own_line}{inherited_line}Complete those tasks first, or use --force to override."
        );
        return Err(anyhow!(msg));
    }

    if task.state() != TaskState::Pending && task.state() != TaskState::Blocked {
        return Err(anyhow!(
            "Task must be in 'pending' state to start. Current state: {}",
            task.state().as_ref()
        ));
    }

    let goal_id = task.goal_id().to_owned();
    let parent_id = task.parent_id().cloned();

    let task = db.get_task_mut(task_id).unwrap();
    let started = task.transition_from_any(
        &[TaskState::Pending, TaskState::Blocked],
        TaskState::InProgress,
    );
    if !started {
        return Err(anyhow!(
            "Failed to start task: another process may have already started it"
        ));
    }
    task.set_assignee(Some(assignee.to_owned()));
    let task = task.clone();
    db.save_task(&task)?;

    if let Some(ref pid) = parent_id {
        db.sync_parent_state(pid)?;
    }

    // Transition the goal to in_progress on first task start.
    let goal = db.get_goal_mut(&goal_id).unwrap();
    if goal.state() == GoalState::Pending {
        goal.mark_in_progress();
        let goal = goal.clone();
        db.save_goal(&goal)?;
    }

    Ok(db.get_task(task_id).unwrap().clone())
}

/// Collect IDs of all in-progress tasks across all goals.
fn collect_in_progress_task_ids(db: &Database) -> Vec<TaskId> {
    db.list_goals()
        .iter()
        .flat_map(|goal| db.list_tasks(goal.id()))
        .filter(|t| t.state() == TaskState::InProgress)
        .map(|t| t.id().to_owned())
        .collect()
}

/// Release all in-progress tasks whose start time exceeds the given duration.
///
/// Falls back to `updated_at` for tasks without a `started_at` timestamp
/// (i.e. tasks started before this feature was added).
pub fn release_stale(threshold: SignedDuration, db: &mut Database) -> Result<Vec<Task>> {
    use std::collections::HashSet;

    let cutoff = Timestamp::now().checked_sub(threshold)?;
    let task_ids = collect_in_progress_task_ids(db);

    let mut stale_ids = Vec::new();
    for id in &task_ids {
        let task = db.get_task(id).unwrap();
        let started = task.started_at().unwrap_or_else(|| task.updated_at());
        if started <= cutoff {
            stale_ids.push(id.clone());
        }
    }

    let mut parent_ids: HashSet<TaskId> = HashSet::new();
    let mut released = Vec::new();
    for id in &stale_ids {
        let task = db.get_task_mut(id).unwrap();
        if let Some(pid) = task.parent_id().cloned() {
            parent_ids.insert(pid);
        }
        task.release();
        let task = task.clone();
        db.save_task(&task)?;
        released.push(task);
    }

    for pid in &parent_ids {
        db.sync_parent_state(pid)?;
    }

    Ok(released)
}

/// Release every in-progress task regardless of how long it has been running.
pub fn release_all_in_progress(db: &mut Database) -> Result<Vec<Task>> {
    use std::collections::HashSet;

    let task_ids = collect_in_progress_task_ids(db);

    let mut parent_ids: HashSet<TaskId> = HashSet::new();
    let mut released = Vec::new();
    for id in &task_ids {
        let task = db.get_task_mut(id).unwrap();
        if let Some(pid) = task.parent_id().cloned() {
            parent_ids.insert(pid);
        }
        task.release();
        let task = task.clone();
        db.save_task(&task)?;
        released.push(task);
    }

    for pid in &parent_ids {
        db.sync_parent_state(pid)?;
    }

    Ok(released)
}

/// Find all in-progress tasks that have been running longer than the given threshold.
/// Used by prep to surface stale task advisories.
pub fn find_stale_tasks(threshold: SignedDuration, db: &Database) -> Vec<&Task> {
    let Ok(cutoff) = Timestamp::now().checked_sub(threshold) else {
        return Vec::new();
    };
    db.list_goals()
        .iter()
        .flat_map(|goal| db.list_tasks(goal.id()))
        .filter(|t| {
            if t.state() != TaskState::InProgress {
                return false;
            }
            let started = t.started_at().unwrap_or_else(|| t.updated_at());
            started <= cutoff
        })
        .collect()
}

/// Complete an in-progress task, recording its outcome and metrics.
///
/// If `elapsed` is `None`, it is derived from the time between the task's
/// `started_at` (or `updated_at`, for tasks started before that field
/// existed) and now.
///
/// Compacts the task, unblocks any tasks that were waiting on it (or on its
/// parent, if completing this task also completes the parent), and updates
/// the goal's state if every task under it is now resolved.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn complete(
    task_id: &TaskId,
    result_summary: String,
    artifacts: Option<Vec<String>>,
    tokens: Option<i64>,
    elapsed: Option<i64>,
    force: bool,
    db: &mut Database,
) -> Result<CompleteResult> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) && !force {
        return Err(subtasks_block_err(task_id, "complete", true, db));
    }
    let force_override = force && db.has_subtasks(task_id);

    if task.state() != TaskState::InProgress {
        return Err(anyhow!(
            "Task must be in 'in_progress' state to complete. Current state: {}",
            task.state().as_ref()
        ));
    }

    let outcome = build_outcome_and_metrics(task, result_summary, artifacts, tokens, elapsed);
    if force_override {
        record_force_override(task_id, "complete", db)?;
    }
    finish_completion(task_id, outcome.0, outcome.1, db)
}

/// Accept a task under review, completing it the same way `complete` does
/// from `InProgress` — but from `Verifying`, as the other half of the
/// `submit`/`accept`/`reject` review flow.
#[allow(clippy::too_many_arguments)]
pub fn accept(
    task_id: &TaskId,
    result_summary: String,
    artifacts: Option<Vec<String>>,
    tokens: Option<i64>,
    elapsed: Option<i64>,
    force: bool,
    db: &mut Database,
) -> Result<CompleteResult> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) && !force {
        return Err(subtasks_block_err(task_id, "accept", true, db));
    }
    let force_override = force && db.has_subtasks(task_id);

    if task.state() != TaskState::Verifying {
        return Err(anyhow!(
            "Task must be in 'verifying' state to accept. Current state: {}",
            task.state().as_ref()
        ));
    }

    let outcome = build_outcome_and_metrics(task, result_summary, artifacts, tokens, elapsed);
    if force_override {
        record_force_override(task_id, "accept", db)?;
    }
    finish_completion(task_id, outcome.0, outcome.1, db)
}

fn build_outcome_and_metrics(
    task: &Task,
    result_summary: String,
    artifacts: Option<Vec<String>>,
    tokens: Option<i64>,
    elapsed: Option<i64>,
) -> (Outcome, TaskMetrics) {
    let retry_count = task.metrics().retry_count();
    let started_at = task.started_at().unwrap_or_else(|| task.updated_at());
    let artifacts_list = artifacts.unwrap_or_default();

    let outcome = Outcome::new(result_summary, artifacts_list);
    // Derive elapsed time from started_at when the caller doesn't supply one,
    // so agents aren't required to track wall-clock time themselves.
    let elapsed_ms = elapsed.unwrap_or_else(|| {
        Timestamp::now()
            .duration_since(started_at)
            .as_millis()
            .try_into()
            .unwrap_or(i64::MAX)
    });
    let metrics = TaskMetrics::new(tokens.unwrap_or(0), elapsed_ms, retry_count);
    (outcome, metrics)
}

/// Shared tail of `complete`/`accept`: transitions the task to `Completed`,
/// unblocks downstream tasks, syncs the parent, and refreshes goal state.
fn finish_completion(
    task_id: &TaskId,
    outcome: Outcome,
    metrics: TaskMetrics,
    db: &mut Database,
) -> Result<CompleteResult> {
    let task = db.get_task_mut(task_id).unwrap();
    if !task.complete(outcome, metrics) {
        return Err(anyhow!(
            "Failed to complete task: another process may have changed its state"
        ));
    }
    let completed_task = task.clone();
    db.save_task(&completed_task)?;
    let goal_id = completed_task.goal_id().clone();
    let parent_id = completed_task.parent_id().cloned();

    // Snapshot blocked tasks in this goal for unblocking checks
    let tasks_snapshot: Vec<(TaskId, Vec<TaskId>)> = db
        .list_tasks(&goal_id)
        .iter()
        .filter(|t| t.state() == TaskState::Blocked)
        .map(|t| (t.id().clone(), t.blocked_by().to_vec()))
        .collect();

    let mut unblocked_task_ids = Vec::new();

    // Unblock tasks that were waiting on this subtask
    for (dep_id, dep_blocked_by) in &tasks_snapshot {
        if dep_blocked_by.contains(task_id) {
            let all_blockers_done = dep_blocked_by.iter().all(|blocker_id| {
                db.get_task(blocker_id)
                    .is_some_and(|t| t.state() == TaskState::Completed)
            });

            if all_blockers_done {
                let dep_task = db.get_task_mut(dep_id).unwrap();
                dep_task.unblock();
                let dep_task = dep_task.clone();
                db.save_task(&dep_task)?;
                unblocked_task_ids.push(dep_id.clone());
            }
        }
    }

    // Sync parent state and handle tasks blocked by the parent
    if let Some(ref pid) = parent_id {
        let new_parent_state = db.sync_parent_state(pid)?;

        if new_parent_state == Some(TaskState::Completed) {
            // Unblock tasks that were waiting on the parent
            let parent_blocked: Vec<(TaskId, Vec<TaskId>)> = db
                .list_tasks(&goal_id)
                .iter()
                .filter(|t| t.state() == TaskState::Blocked && t.blocked_by().contains(pid))
                .map(|t| (t.id().clone(), t.blocked_by().to_vec()))
                .collect();

            for (dep_id, dep_blocked_by) in &parent_blocked {
                let all_blockers_done = dep_blocked_by.iter().all(|blocker_id| {
                    db.get_task(blocker_id)
                        .is_some_and(|t| t.state() == TaskState::Completed)
                });

                if all_blockers_done {
                    let dep_task = db.get_task_mut(dep_id).unwrap();
                    dep_task.unblock();
                    let dep_task = dep_task.clone();
                    db.save_task(&dep_task)?;
                    unblocked_task_ids.push(dep_id.clone());
                }
            }
        }
    }

    // Check goal completion
    // Cancelled tasks count as "resolved" alongside completed tasks
    let all_tasks = db.list_tasks(&goal_id);
    let all_resolved = all_tasks
        .iter()
        .all(|t| matches!(t.state(), TaskState::Completed | TaskState::Cancelled));
    let any_completed = all_tasks.iter().any(|t| t.state() == TaskState::Completed);
    let any_failed = all_tasks.iter().any(|t| t.state() == TaskState::Failed);

    let goal = db
        .get_goal_mut(&goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    if all_resolved {
        if any_completed {
            goal.mark_completed();
        } else {
            // All tasks cancelled, none completed
            goal.mark_cancelled();
        }
    } else if any_failed {
        goal.mark_failed();
    } else {
        goal.touch();
    }
    db.refresh_goal_metrics(&goal_id)?;

    Ok(CompleteResult {
        task: completed_task,
        unblocked_task_ids,
    })
}

/// Mark an in-progress or verifying task as failed, optionally with a reason.
///
/// Auto-compacts the task when `compact` is set or the retry count has
/// reached 3, so repeated failure history doesn't pollute future context.
pub fn fail(
    task_id: &TaskId,
    reason: Option<String>,
    compact: bool,
    force: bool,
    db: &mut Database,
) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) && !force {
        return Err(subtasks_block_err(task_id, "fail", true, db));
    }
    let force_override = force && db.has_subtasks(task_id);

    if task.state() != TaskState::InProgress && task.state() != TaskState::Verifying {
        return Err(anyhow!(
            "Task must be in 'in_progress' or 'verifying' state to fail. Current state: {}",
            task.state().as_ref()
        ));
    }

    let task = db.get_task_mut(task_id).unwrap();
    if !task.transition_from_any(
        &[TaskState::InProgress, TaskState::Verifying],
        TaskState::Failed,
    ) {
        return Err(anyhow!(
            "Failed to mark task as failed: state may have changed"
        ));
    }

    if let Some(ref r) = reason {
        task.set_result(Outcome::new(r.clone(), vec![]));
    }

    // Auto-compact when the agent explicitly requests it or the retry limit is reached.
    // At 3+ retries the failure history has been consumed by multiple agents; compacting
    // keeps it from polluting future context.
    let should_compact = compact || task.metrics().retry_count() >= 3;
    if should_compact {
        // reason is guaranteed by CLI when --compact is set; use empty string as fallback
        // for the retry-threshold path where reason was not provided.
        let summary = reason.unwrap_or_default();
        task.compact(summary);
    }

    if force_override {
        record_force_override(task_id, "fail", db)?;
    }

    let failed_task = db.get_task(task_id).unwrap().clone();
    db.save_task(&failed_task)?;

    if let Some(pid) = failed_task.parent_id() {
        db.sync_parent_state(pid)?;
    }
    db.refresh_goal_metrics(failed_task.goal_id())?;

    Ok(failed_task)
}

/// Retry a failed task: moves it back to `Pending` (not `InProgress` — the
/// caller needs an explicit `start` to resume work), increments its retry
/// count, and records the prior result as a comment tagged with the attempt
/// number.
pub fn retry(task_id: &TaskId, force: bool, db: &mut Database) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) && !force {
        return Err(subtasks_block_err(task_id, "retry", true, db));
    }
    let force_override = force && db.has_subtasks(task_id);

    if task.state() != TaskState::Failed {
        return Err(anyhow!(
            "Task must be in 'failed' state to retry. Current state: {}",
            task.state().as_ref()
        ));
    }

    let task = db.get_task_mut(task_id).unwrap();
    if !task.retry() {
        return Err(anyhow!("Failed to retry task: state may have changed"));
    }
    if force_override {
        record_force_override(task_id, "retry", db)?;
    }
    let retried_task = db.get_task(task_id).unwrap().clone();
    db.save_task(&retried_task)?;

    if let Some(pid) = retried_task.parent_id() {
        db.sync_parent_state(pid)?;
    }
    db.refresh_goal_metrics(retried_task.goal_id())?;

    Ok(retried_task)
}

/// Submit an in-progress task for review: moves it to `Verifying` with no
/// result recorded yet. The task stays excluded from `rd ready` and keeps
/// downstream tasks blocked until a reviewer `accept`s it with `complete`'s
/// usual effects, or `reject`s it back to `InProgress` for rework.
pub fn submit(task_id: &TaskId, force: bool, db: &mut Database) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) && !force {
        return Err(subtasks_block_err(task_id, "submit", true, db));
    }
    let force_override = force && db.has_subtasks(task_id);

    if task.state() != TaskState::InProgress {
        return Err(anyhow!(
            "Task must be in 'in_progress' state to submit. Current state: {}",
            task.state().as_ref()
        ));
    }

    let task = db.get_task_mut(task_id).unwrap();
    if !task.transition_from_any(&[TaskState::InProgress], TaskState::Verifying) {
        return Err(anyhow!("Failed to submit task: state may have changed"));
    }
    if force_override {
        record_force_override(task_id, "submit", db)?;
    }
    let submitted_task = db.get_task(task_id).unwrap().clone();
    db.save_task(&submitted_task)?;

    if let Some(pid) = submitted_task.parent_id() {
        db.sync_parent_state(pid)?;
    }
    db.refresh_goal_metrics(submitted_task.goal_id())?;

    Ok(submitted_task)
}

/// Reject a task under review: sends it back to `InProgress` for rework
/// (the assignee stays claimed, unlike `retry`, which requires a fresh
/// `start`), increments the retry counter, and records the reviewer's
/// reason as an attempt-numbered comment.
pub fn reject(task_id: &TaskId, reason: String, force: bool, db: &mut Database) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) && !force {
        return Err(subtasks_block_err(task_id, "reject", true, db));
    }
    let force_override = force && db.has_subtasks(task_id);

    if task.state() != TaskState::Verifying {
        return Err(anyhow!(
            "Task must be in 'verifying' state to reject. Current state: {}",
            task.state().as_ref()
        ));
    }

    let task = db.get_task_mut(task_id).unwrap();
    if !task.reject(&reason) {
        return Err(anyhow!("Failed to reject task: state may have changed"));
    }
    if force_override {
        record_force_override(task_id, "reject", db)?;
    }
    let rejected_task = db.get_task(task_id).unwrap().clone();
    db.save_task(&rejected_task)?;

    if let Some(pid) = rejected_task.parent_id() {
        db.sync_parent_state(pid)?;
    }
    db.refresh_goal_metrics(rejected_task.goal_id())?;

    Ok(rejected_task)
}

/// Clear a task's assignee, returning it to `Pending`.
pub fn release(task_id: &TaskId, force: bool, db: &mut Database) -> Result<Task> {
    if db.get_task(task_id).is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    if db.has_subtasks(task_id) && !force {
        return Err(subtasks_block_err(task_id, "release", true, db));
    }
    let force_override = force && db.has_subtasks(task_id);

    let task = db.get_task_mut(task_id).unwrap();
    if !task.release() {
        return Err(anyhow!(
            "Task has no assignee to release. Current state: {}",
            task.state().as_ref()
        ));
    }
    if force_override {
        record_force_override(task_id, "release", db)?;
    }
    let released_task = db.get_task(task_id).unwrap().clone();
    db.save_task(&released_task)?;

    if let Some(pid) = released_task.parent_id() {
        db.sync_parent_state(pid)?;
    }

    Ok(released_task)
}

/// Cascade cancel a task and all its downstream dependencies using BFS.
///
/// Returns the IDs of all tasks that were cascade-cancelled.
fn cascade_cancel_downstream(
    root_task_id: &TaskId,
    goal_id: &GoalId,
    downstream_ids: &[TaskId],
    db: &mut Database,
) -> Result<Vec<TaskId>> {
    let mut cascaded_task_ids = Vec::new();
    let mut to_cancel: VecDeque<TaskId> = VecDeque::new();
    let mut visited: HashSet<TaskId> = HashSet::new();

    // Start BFS with tasks that were directly blocked by the cancelled task
    for dep_id in downstream_ids {
        to_cancel.push_back(dep_id.clone());
    }

    while let Some(current_id) = to_cancel.pop_front() {
        if visited.contains(&current_id) {
            continue;
        }
        visited.insert(current_id.clone());

        let Some(current_task) = db.get_task(&current_id) else {
            continue; // Task was deleted or doesn't exist
        };

        // Skip if already terminal
        if matches!(
            current_task.state(),
            TaskState::Completed | TaskState::Cancelled
        ) {
            continue;
        }

        // Cancel this task
        let task_mut = db.get_task_mut(&current_id).unwrap();
        if task_mut.cancel() {
            let cascade_reason = format!("cascaded from cancellation of {root_task_id}");
            let cascade_comment = Comment::new(generate_id(), cascade_reason, Timestamp::now());
            task_mut.add_comment(cascade_comment);
            let cancelled = task_mut.clone();
            db.save_task(&cancelled)?;
            cascaded_task_ids.push(current_id.clone());

            // Sync parent if this task has one
            if let Some(pid) = cancelled.parent_id().cloned() {
                db.sync_parent_state(&pid)?;
            }

            // Find tasks blocked by this cancelled task and add to queue
            let next_downstream: Vec<TaskId> = db
                .list_tasks(goal_id)
                .iter()
                .filter(|t| t.blocked_by().contains(&current_id))
                .map(|t| t.id().clone())
                .collect();

            for next_id in next_downstream {
                if !visited.contains(&next_id) {
                    to_cancel.push_back(next_id);
                }
            }
        }
    }

    Ok(cascaded_task_ids)
}

/// Cancel a task, removing it from downstream tasks' `blocked_by` lists and
/// auto-unblocking any that become fully unblocked as a result.
///
/// If `cascade` is set, also cancels every downstream task reachable through
/// `blocked_by` chains. Fails if the task has subtasks, or is already
/// completed or cancelled.
#[allow(clippy::too_many_arguments)]
pub fn cancel(
    task_id: &TaskId,
    reason: Option<String>,
    author: &str,
    cascade: bool,
    force: bool,
    db: &mut Database,
) -> Result<CancelResult> {
    db.get_task(task_id)
        .ok_or_else(|| task_not_found_err(task_id, db))?;

    if db.has_subtasks(task_id) && !force {
        return Err(subtasks_block_err(task_id, "cancel", true, db));
    }
    let force_override = force && db.has_subtasks(task_id);
    let override_text = force_override.then(|| force_override_comment_text(task_id, "cancel", db));

    let task = db.get_task(task_id).unwrap();
    if task.state() == TaskState::Completed {
        return Err(anyhow!(
            "Cannot cancel a completed task. Completed work is history."
        ));
    }

    if task.state() == TaskState::Cancelled {
        return Err(anyhow!("Task is already cancelled."));
    }

    let goal_id = task.goal_id().clone();
    let parent_id = task.parent_id().cloned();

    // Cancel the task
    let task = db.get_task_mut(task_id).unwrap();
    if !task.cancel() {
        return Err(anyhow!("Failed to cancel task: state may have changed"));
    }

    // Add cancel comment with author and reason
    let comment_text = if let Some(ref r) = reason {
        format!("Cancelled by {author}: {r}")
    } else {
        format!("Cancelled by {author}")
    };
    let comment = Comment::new(generate_id(), comment_text, Timestamp::now());
    task.add_comment(comment);
    if let Some(text) = override_text {
        task.add_comment(Comment::new(generate_id(), text, Timestamp::now()));
    }
    let cancelled_task = task.clone();
    db.save_task(&cancelled_task)?;

    // Remove the cancelled task ID from any downstream blocked_by lists and unblock
    // tasks whose blocker list becomes empty (Option A: auto-unblock)
    let downstream_ids: Vec<TaskId> = db
        .list_tasks(&goal_id)
        .iter()
        .filter(|t| t.blocked_by().contains(task_id))
        .map(|t| t.id().clone())
        .collect();

    let mut unblocked_task_ids = Vec::new();

    for dep_id in &downstream_ids {
        let dep = db.get_task_mut(dep_id).unwrap();
        let new_blockers: Vec<TaskId> = dep
            .blocked_by()
            .iter()
            .filter(|id| *id != task_id)
            .cloned()
            .collect();
        let should_unblock = new_blockers.is_empty() && dep.state() == TaskState::Blocked;
        dep.set_blocked_by(new_blockers);

        // Add system comment noting the cancelled dependency
        let dep_comment_text = format!(
            "Dependency {} (\"{}\") was cancelled{}. Verify this task's `receives` is still satisfiable before starting.",
            task_id,
            cancelled_task.description(),
            reason.as_ref().map_or(String::new(), |r| format!(": {r}"))
        );
        let dep_comment = Comment::new(generate_id(), dep_comment_text, Timestamp::now());
        dep.add_comment(dep_comment);

        if should_unblock {
            dep.unblock();
            unblocked_task_ids.push(dep_id.clone());
        }
        let dep = dep.clone();
        db.save_task(&dep)?;
    }

    // Sync parent state
    if let Some(ref pid) = parent_id {
        db.sync_parent_state(pid)?;
    }

    // Cascade cancellation to downstream dependencies if requested
    let cascaded_task_ids = if cascade {
        cascade_cancel_downstream(task_id, &goal_id, &downstream_ids, db)?
    } else {
        Vec::new()
    };

    db.refresh_goal_metrics(&goal_id)?;

    Ok(CancelResult {
        task: cancelled_task,
        unblocked_task_ids,
        cascaded_task_ids,
    })
}

/// Delete a pending task, removing it from any downstream `blocked_by`
/// lists so those tasks don't deadlock on a task that no longer exists.
///
/// Fails if the task has subtasks or is not `Pending`.
pub fn delete(task_id: &TaskId, db: &mut Database) -> Result<Task> {
    let task = db
        .get_task(task_id)
        .ok_or_else(|| task_not_found_err(task_id, db))?;

    if db.has_subtasks(task_id) {
        return Err(subtasks_block_err(task_id, "delete", false, db));
    }

    if task.state() != TaskState::Pending {
        return Err(anyhow!(
            "Task must be in 'pending' state to delete. Current state: {}",
            task.state().as_ref()
        ));
    }

    let task = task.clone();
    let parent_id = task.parent_id().cloned();
    let goal_id = task.goal_id().clone();
    db.delete_task(task_id, &goal_id)?;

    // Remove the deleted task ID from any downstream blocked_by lists so those
    // tasks are not permanently deadlocked waiting on a task that no longer exists.
    let downstream_ids: Vec<TaskId> = db
        .list_tasks(&goal_id)
        .iter()
        .filter(|t| t.blocked_by().contains(task_id))
        .map(|t| t.id().clone())
        .collect();

    for dep_id in &downstream_ids {
        let dep = db.get_task_mut(dep_id).unwrap();
        let new_blockers: Vec<TaskId> = dep
            .blocked_by()
            .iter()
            .filter(|id| *id != task_id)
            .cloned()
            .collect();
        let should_unblock = new_blockers.is_empty() && dep.state() == TaskState::Blocked;
        dep.set_blocked_by(new_blockers);
        if should_unblock {
            dep.unblock();
        }
        let dep = dep.clone();
        db.save_task(&dep)?;
    }

    if let Some(pid) = parent_id {
        db.sync_parent_state(&pid)?;
    }
    db.refresh_goal_metrics(&goal_id)?;

    Ok(task)
}

/// Look up a task and its full comment history.
pub fn comments(task_id: &TaskId, db: &Database) -> Result<Task> {
    db.get_task(task_id)
        .ok_or_else(|| task_not_found_err(task_id, db))
        .cloned()
}

/// Add a comment to a task.
pub fn comment(task_id: &TaskId, text: String, db: &mut Database) -> Result<Task> {
    if db.get_task(task_id).is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let comment = Comment::new(generate_id(), text, Timestamp::now());

    let task = db.get_task_mut(task_id).unwrap();
    task.add_comment(comment);
    let task = task.clone();
    db.save_task(&task)?;

    Ok(task)
}
