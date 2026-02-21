use anyhow::{Result, anyhow};
use jiff::Timestamp;

use crate::db::Database;
use crate::helpers::find_similar_id;
use crate::id::generate_id;
use crate::models::{Comment, Contract, GoalState, Outcome, Task, TaskMetrics, TaskState};

/// Result of completing a task, including any unblocked tasks.
#[derive(Debug)]
pub struct CompleteResult {
    pub task: Task,
    pub unblocked_task_ids: Vec<String>,
}

fn task_not_found_err(task_id: &str, db: &Database) -> anyhow::Error {
    let all_task_ids: Vec<&str> = db
        .list_goals()
        .iter()
        .flat_map(|goal| db.list_tasks(goal.id()).into_iter().map(Task::id))
        .collect();

    if let Some(suggestion) = find_similar_id(task_id, &all_task_ids) {
        anyhow!("Task not found: {task_id}\nDid you mean: {suggestion}")
    } else {
        anyhow!("Task not found: {task_id}")
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create(
    goal_id: &str,
    description: String,
    receives: Option<String>,
    produces: Option<String>,
    verify: Option<String>,
    blocked_by: Option<Vec<String>>,
    db: &mut Database,
) -> Result<Task> {
    let goal = db.get_goal(goal_id);

    if goal.is_none() {
        let all_goals = db.list_goals();
        let goal_ids: Vec<&str> = all_goals.iter().map(|g| g.id()).collect();

        return if let Some(suggestion) = find_similar_id(goal_id, &goal_ids) {
            Err(anyhow!(
                "Goal not found: {goal_id}\nDid you mean: {suggestion}"
            ))
        } else {
            Err(anyhow!("Goal not found: {goal_id}"))
        };
    }

    let goal = goal.unwrap();
    let goal_id_owned = goal.id().to_owned();
    let goal_state = goal.state();

    // Validate blocked_by task IDs exist
    if let Some(ref task_ids) = blocked_by {
        let all_tasks = db.list_tasks(&goal_id_owned);
        let existing_task_ids: Vec<&str> = all_tasks.iter().map(|t| t.id()).collect();

        for task_id in task_ids {
            if !existing_task_ids.contains(&task_id.as_str()) {
                return if let Some(suggestion) = find_similar_id(task_id, &existing_task_ids) {
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
        generate_id(),
        goal_id_owned.clone(),
        description,
        contract,
        state,
        blocked_by_ids,
        now,
        now,
    );

    db.create_task(task.clone())?;

    // Update the goal
    let base = db.base_path().to_owned();
    let goal = db.get_goal_mut(&goal_id_owned).unwrap();
    if goal_state == GoalState::Pending {
        goal.mark_in_progress();
    } else {
        goal.touch();
    }
    goal.write_file(&base)?;

    Ok(task)
}

pub fn list(goal_id: &str, db: &Database) -> Result<Vec<Task>> {
    db.get_goal(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    Ok(db.list_tasks(goal_id).into_iter().cloned().collect())
}

pub fn start(task_id: &str, db: &mut Database) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

    if task.contract().is_none() {
        return Err(anyhow!(
            "Task has no contract. Set a contract before starting.\nUse: radial task contract {} --receives \"...\" --produces \"...\" --verify \"...\"",
            task.id()
        ));
    }

    if task.state() == TaskState::Blocked && !task.blocked_by().is_empty() {
        return Err(anyhow!(
            "Task is blocked by: {}\nComplete those tasks first, or use --force to override.",
            task.blocked_by().join(", ")
        ));
    }

    if task.state() != TaskState::Pending {
        return Err(anyhow!(
            "Task must be in 'pending' state to start. Current state: {}",
            task.state().as_ref()
        ));
    }

    let base = db.base_path().to_owned();
    let task = db.get_task_mut(task_id).unwrap();
    if !task.transition(TaskState::Pending, TaskState::InProgress) {
        return Err(anyhow!(
            "Failed to start task: another process may have already started it"
        ));
    }
    task.write_file(&base)?;

    Ok(task.clone())
}

pub fn complete(
    task_id: &str,
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

    if task.state() != TaskState::InProgress {
        return Err(anyhow!(
            "Task must be in 'in_progress' state to complete. Current state: {}",
            task.state().as_ref()
        ));
    }

    let goal_id = task.goal_id().to_owned();
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

    // Snapshot only the fields needed for unblocking
    let tasks_snapshot: Vec<(String, TaskState, Vec<String>)> = db
        .list_tasks(&goal_id)
        .iter()
        .filter(|t| t.state() == TaskState::Blocked)
        .map(|t| (t.id().to_owned(), t.state(), t.blocked_by().to_vec()))
        .collect();

    let mut unblocked_task_ids = Vec::new();

    for (dep_id, _, dep_blocked_by) in &tasks_snapshot {
        if dep_blocked_by.contains(&task_id.to_owned()) {
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

pub fn fail(task_id: &str, db: &mut Database) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

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

    Ok(task.clone())
}

pub fn retry(task_id: &str, db: &mut Database) -> Result<Task> {
    let task = db.get_task(task_id);

    if task.is_none() {
        return Err(task_not_found_err(task_id, db));
    }

    let task = task.unwrap();

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

    Ok(task.clone())
}

pub fn comment(task_id: &str, text: String, db: &mut Database) -> Result<Task> {
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
