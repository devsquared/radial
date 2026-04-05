use anyhow::{Result, anyhow};
use jiff::{SignedDuration, Timestamp};

use crate::db::Database;
use crate::helpers::find_similar_id;
use crate::id::{GoalId, TaskId, generate_id};
use crate::models::{
    Comment, Contract, GoalState, Outcome, Priority, Task, TaskMetrics, TaskState,
};

/// Result of completing a task, including any unblocked tasks.
#[derive(Debug)]
pub struct CompleteResult {
    pub task: Task,
    pub unblocked_task_ids: Vec<TaskId>,
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

#[allow(clippy::too_many_arguments)]
pub fn create(
    goal_id: &GoalId,
    description: String,
    priority: Priority,
    parent_id: Option<TaskId>,
    receives: Option<String>,
    produces: Option<String>,
    verify: Option<String>,
    blocked_by: Option<Vec<TaskId>>,
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

    let blocked_by_ids = blocked_by.unwrap_or_default();
    let state = if blocked_by_ids.is_empty() {
        TaskState::Pending
    } else {
        TaskState::Blocked
    };
    let now = Timestamp::now();
    let task = Task::new(
        TaskId::new(),
        goal_id_owned.clone(),
        parent_id.clone(),
        description,
        priority,
        contract,
        state,
        blocked_by_ids,
        now,
        now,
    );

    db.create_task(task.clone())?;

    // Sync parent state now that it has a new subtask
    let base = db.base_path().to_owned();
    if let Some(ref pid) = parent_id {
        db.sync_parent_state(pid, &base)?;
    }

    // Update the goal
    let goal = db.get_goal_mut(&goal_id_owned).unwrap();
    goal.touch();
    goal.write_file(&base)?;

    Ok(task)
}

pub fn list(
    goal_id: &GoalId,
    priority: Option<&Priority>,
    assignee: Option<&str>,
    db: &Database,
) -> Result<Vec<Task>> {
    db.get_goal(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    let mut tasks: Vec<Task> = db
        .list_tasks(goal_id)
        .into_iter()
        .filter(|t| priority.is_none_or(|p| t.priority() == *p))
        .filter(|t| assignee.is_none_or(|a| t.assignee() == Some(a)))
        .cloned()
        .collect();
    tasks.sort_by_key(Task::priority);
    Ok(tasks)
}

pub fn start(task_id: &TaskId, assignee: &str, db: &mut Database) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) {
        return Err(anyhow!(
            "Task {task_id} has subtasks and cannot be started directly. Start its subtasks instead."
        ));
    }

    if task.contract().is_none() {
        return Err(anyhow!(
            "Task has no contract. Set a contract before starting.\nUse: radial task contract {} --receives \"...\" --produces \"...\" --verify \"...\"",
            task.id()
        ));
    }

    if task.state() == TaskState::Blocked && !task.blocked_by().is_empty() {
        let ids: Vec<&str> = task.blocked_by().iter().map(AsRef::as_ref).collect();
        return Err(anyhow!(
            "Task is blocked by: {}\nComplete those tasks first, or use --force to override.",
            ids.join(", ")
        ));
    }

    if task.state() != TaskState::Pending {
        return Err(anyhow!(
            "Task must be in 'pending' state to start. Current state: {}",
            task.state().as_ref()
        ));
    }

    let goal_id = task.goal_id().to_owned();

    let base = db.base_path().to_owned();
    let task = db.get_task_mut(task_id).unwrap();
    if !task.transition(TaskState::Pending, TaskState::InProgress) {
        return Err(anyhow!(
            "Failed to start task: another process may have already started it"
        ));
    }
    task.set_assignee(Some(assignee.to_owned()));
    task.write_file(&base)?;

    // Transition the goal to in_progress on first task start.
    let goal = db.get_goal_mut(&goal_id).unwrap();
    if goal.state() == GoalState::Pending {
        goal.mark_in_progress();
        goal.write_file(&base)?;
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

    let base = db.base_path().to_owned();
    let mut parent_ids: HashSet<TaskId> = HashSet::new();
    let mut released = Vec::new();
    for id in &stale_ids {
        let task = db.get_task_mut(id).unwrap();
        if let Some(pid) = task.parent_id().cloned() {
            parent_ids.insert(pid);
        }
        task.release();
        task.write_file(&base)?;
        released.push(task.clone());
    }

    for pid in &parent_ids {
        db.sync_parent_state(pid, &base)?;
    }

    Ok(released)
}

/// Release every in-progress task regardless of how long it has been running.
pub fn release_all_in_progress(db: &mut Database) -> Result<Vec<Task>> {
    use std::collections::HashSet;

    let task_ids = collect_in_progress_task_ids(db);
    let base = db.base_path().to_owned();

    let mut parent_ids: HashSet<TaskId> = HashSet::new();
    let mut released = Vec::new();
    for id in &task_ids {
        let task = db.get_task_mut(id).unwrap();
        if let Some(pid) = task.parent_id().cloned() {
            parent_ids.insert(pid);
        }
        task.release();
        task.write_file(&base)?;
        released.push(task.clone());
    }

    for pid in &parent_ids {
        db.sync_parent_state(pid, &base)?;
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

pub fn complete(
    task_id: &TaskId,
    result_summary: String,
    artifacts: Option<Vec<String>>,
    tokens: Option<i64>,
    elapsed: Option<i64>,
    db: &mut Database,
) -> Result<CompleteResult> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) {
        return Err(anyhow!(
            "Task {task_id} has subtasks and cannot be completed directly. Complete its subtasks instead."
        ));
    }

    if task.state() != TaskState::InProgress {
        return Err(anyhow!(
            "Task must be in 'in_progress' state to complete. Current state: {}",
            task.state().as_ref()
        ));
    }

    let goal_id = task.goal_id().clone();
    let retry_count = task.metrics().retry_count();
    let artifacts_list = artifacts.unwrap_or_default();

    let outcome = Outcome::new(result_summary, artifacts_list);
    let metrics = TaskMetrics::new(tokens.unwrap_or(0), elapsed.unwrap_or(0), retry_count);

    let base = db.base_path().to_owned();
    let task = db.get_task_mut(task_id).unwrap();
    if !task.complete(outcome, metrics) {
        return Err(anyhow!(
            "Failed to complete task: another process may have changed its state"
        ));
    }
    task.write_file(&base)?;
    let completed_task = task.clone();
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
                dep_task.write_file(&base)?;
                unblocked_task_ids.push(dep_id.clone());
            }
        }
    }

    // Sync parent state and handle tasks blocked by the parent
    if let Some(ref pid) = parent_id {
        let new_parent_state = db.sync_parent_state(pid, &base)?;

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
                    dep_task.write_file(&base)?;
                    unblocked_task_ids.push(dep_id.clone());
                }
            }
        }
    }

    // Check goal completion
    let all_tasks = db.list_tasks(&goal_id);
    let all_completed = all_tasks.iter().all(|t| t.state() == TaskState::Completed);
    let any_failed = all_tasks.iter().any(|t| t.state() == TaskState::Failed);

    let goal = db
        .get_goal_mut(&goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    if all_completed {
        goal.mark_completed();
    } else if any_failed {
        goal.mark_failed();
    } else {
        goal.touch();
    }
    goal.write_file(&base)?;

    Ok(CompleteResult {
        task: completed_task,
        unblocked_task_ids,
    })
}

pub fn fail(task_id: &TaskId, db: &mut Database) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) {
        return Err(anyhow!(
            "Task {task_id} has subtasks and cannot be failed directly. Fail its subtasks instead."
        ));
    }

    if task.state() != TaskState::InProgress && task.state() != TaskState::Verifying {
        return Err(anyhow!(
            "Task must be in 'in_progress' or 'verifying' state to fail. Current state: {}",
            task.state().as_ref()
        ));
    }

    let base = db.base_path().to_owned();
    let task = db.get_task_mut(task_id).unwrap();
    if !task.transition_from_any(
        &[TaskState::InProgress, TaskState::Verifying],
        TaskState::Failed,
    ) {
        return Err(anyhow!(
            "Failed to mark task as failed: state may have changed"
        ));
    }
    task.write_file(&base)?;
    let failed_task = task.clone();

    if let Some(pid) = failed_task.parent_id() {
        db.sync_parent_state(pid, &base)?;
    }

    Ok(failed_task)
}

pub fn retry(task_id: &TaskId, db: &mut Database) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if db.has_subtasks(task_id) {
        return Err(anyhow!(
            "Task {task_id} has subtasks and cannot be retried directly. Retry its subtasks instead."
        ));
    }

    if task.state() != TaskState::Failed {
        return Err(anyhow!(
            "Task must be in 'failed' state to retry. Current state: {}",
            task.state().as_ref()
        ));
    }

    let base = db.base_path().to_owned();
    let task = db.get_task_mut(task_id).unwrap();
    if !task.retry() {
        return Err(anyhow!("Failed to retry task: state may have changed"));
    }
    task.write_file(&base)?;
    let retried_task = task.clone();

    if let Some(pid) = retried_task.parent_id() {
        db.sync_parent_state(pid, &base)?;
    }

    Ok(retried_task)
}

pub fn release(task_id: &TaskId, db: &mut Database) -> Result<Task> {
    if db.get_task(task_id).is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    if db.has_subtasks(task_id) {
        return Err(anyhow!(
            "Task {task_id} has subtasks and cannot be released directly. Release its subtasks instead."
        ));
    }

    let base = db.base_path().to_owned();
    let task = db.get_task_mut(task_id).unwrap();
    if !task.release() {
        return Err(anyhow!(
            "Task has no assignee to release. Current state: {}",
            task.state().as_ref()
        ));
    }
    task.write_file(&base)?;
    let released_task = task.clone();

    if let Some(pid) = released_task.parent_id() {
        db.sync_parent_state(pid, &base)?;
    }

    Ok(released_task)
}

pub fn delete(task_id: &TaskId, db: &mut Database) -> Result<Task> {
    let task = db
        .get_task(task_id)
        .ok_or_else(|| task_not_found_err(task_id, db))?;

    if db.has_subtasks(task_id) {
        return Err(anyhow!(
            "Task {task_id} has subtasks. Delete its subtasks first."
        ));
    }

    if task.state() != TaskState::Pending {
        return Err(anyhow!(
            "Task must be in 'pending' state to delete. Current state: {}",
            task.state().as_ref()
        ));
    }

    let task = task.clone();
    let parent_id = task.parent_id().cloned();
    db.delete_task(task_id, task.goal_id())?;

    if let Some(pid) = parent_id {
        let base = db.base_path().to_owned();
        db.sync_parent_state(&pid, &base)?;
    }

    Ok(task)
}

pub fn comment(task_id: &TaskId, text: String, db: &mut Database) -> Result<Task> {
    if db.get_task(task_id).is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let comment = Comment::new(generate_id(), text, Timestamp::now());

    let base = db.base_path().to_owned();
    let task = db.get_task_mut(task_id).unwrap();
    task.add_comment(comment);
    task.write_file(&base)?;

    Ok(task.clone())
}
