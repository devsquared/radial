use anyhow::{anyhow, Result};
use jiff::Timestamp;

use crate::db::Database;
use crate::helpers::find_similar_id;
use crate::id::generate_id;
use crate::models::{Comment, Contract, GoalState, Task, TaskMetrics, TaskState};

/// Result of completing a task, including any unblocked tasks.
#[derive(Debug)]
pub struct CompleteResult {
    pub task: Task,
    pub unblocked_task_ids: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn create(
    goal_id: String,
    description: String,
    receives: Option<String>,
    produces: Option<String>,
    verify: Option<String>,
    blocked_by: Option<Vec<String>>,
    db: &mut Database,
) -> Result<Task> {
    let goal = db.get_goal(&goal_id)?;

    if goal.is_none() {
        let all_goals = db.list_goals()?;
        let goal_ids: Vec<String> = all_goals.iter().map(|g| g.id.clone()).collect();

        return if let Some(suggestion) = find_similar_id(&goal_id, &goal_ids) {
            Err(anyhow!(
                "Goal not found: {goal_id}\nDid you mean: {suggestion}"
            ))
        } else {
            Err(anyhow!("Goal not found: {goal_id}"))
        };
    }

    let goal = goal.unwrap();

    // Validate blocked_by task IDs exist
    if let Some(ref task_ids) = blocked_by {
        let all_tasks = db.list_tasks(&goal.id)?;
        let existing_task_ids: Vec<String> = all_tasks.iter().map(|t| t.id.clone()).collect();

        for task_id in task_ids {
            if !existing_task_ids.contains(task_id) {
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
        Some(Contract {
            receives: receives.unwrap_or_default(),
            produces: produces.unwrap_or_default(),
            verify: verify.unwrap_or_default(),
        })
    } else {
        None
    };

    let blocked_by_ids = blocked_by.unwrap_or_default();
    let now = Timestamp::now();
    let task = Task {
        id: generate_id(),
        goal_id: goal.id.clone(),
        description,
        contract,
        state: if blocked_by_ids.is_empty() {
            TaskState::Pending
        } else {
            TaskState::Blocked
        },
        blocked_by: blocked_by_ids,
        result: None,
        created_at: now,
        updated_at: now,
        completed_at: None,
        metrics: TaskMetrics::default(),
        comments: Vec::new(),
    };

    db.create_task(&task)?;

    let mut updated_goal = goal;
    updated_goal.updated_at = Timestamp::now();
    if updated_goal.state == GoalState::Pending {
        updated_goal.state = GoalState::InProgress;
    }
    db.update_goal(&updated_goal)?;

    Ok(task)
}

pub fn list(goal_id: String, db: &Database) -> Result<Vec<Task>> {
    let _goal = db
        .get_goal(&goal_id)?
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    db.list_tasks(&goal_id)
}

pub fn start(task_id: String, db: &mut Database) -> Result<Task> {
    let task = db.get_task(&task_id)?;

    if task.is_none() {
        // Get all tasks across all goals for suggestions
        let all_task_ids: Vec<String> = db
            .list_goals()?
            .iter()
            .flat_map(|goal| {
                db.list_tasks(&goal.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| t.id)
            })
            .collect();

        return if let Some(suggestion) = find_similar_id(&task_id, &all_task_ids) {
            Err(anyhow!(
                "Task not found: {task_id}\nDid you mean: {suggestion}"
            ))
        } else {
            Err(anyhow!("Task not found: {task_id}"))
        };
    }

    let task = task.unwrap();

    if task.contract.is_none() {
        return Err(anyhow!(
            "Task has no contract. Set a contract before starting.\nUse: radial task contract {} --receives \"...\" --produces \"...\" --verify \"...\"",
            task.id
        ));
    }

    if task.state == TaskState::Blocked && !task.blocked_by.is_empty() {
        return Err(anyhow!(
            "Task is blocked by: {}\nComplete those tasks first, or use --force to override.",
            task.blocked_by.join(", ")
        ));
    }

    if task.state != TaskState::Pending {
        return Err(anyhow!(
            "Task must be in 'pending' state to start. Current state: {}",
            task.state.as_ref()
        ));
    }

    let updated_at = Timestamp::now().to_string();
    let transitioned = db.transition_task_state(
        &task.id,
        &TaskState::Pending,
        &TaskState::InProgress,
        &updated_at,
    )?;

    if !transitioned {
        return Err(anyhow!(
            "Failed to start task: another process may have already started it"
        ));
    }

    // Re-fetch to get the updated state
    db.get_task(&task_id)?
        .ok_or_else(|| anyhow!("Task not found after transition"))
}

pub fn complete(
    task_id: String,
    result_summary: String,
    artifacts: Option<Vec<String>>,
    tokens: Option<i64>,
    elapsed: Option<i64>,
    db: &mut Database,
) -> Result<CompleteResult> {
    let task = db.get_task(&task_id)?;

    if task.is_none() {
        // Get all tasks across all goals for suggestions
        let all_task_ids: Vec<String> = db
            .list_goals()?
            .iter()
            .flat_map(|goal| {
                db.list_tasks(&goal.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| t.id)
            })
            .collect();

        return if let Some(suggestion) = find_similar_id(&task_id, &all_task_ids) {
            Err(anyhow!(
                "Task not found: {task_id}\nDid you mean: {suggestion}"
            ))
        } else {
            Err(anyhow!("Task not found: {task_id}"))
        };
    }

    let task = task.unwrap();

    if task.state != TaskState::InProgress {
        return Err(anyhow!(
            "Task must be in 'in_progress' state to complete. Current state: {}",
            task.state.as_ref()
        ));
    }

    let now = Timestamp::now();
    let updated_at = now.to_string();
    let completed_at = now.to_string();
    let artifacts_list = artifacts.unwrap_or_default();

    let transitioned = db.complete_task(
        &task.id,
        &result_summary,
        artifacts_list,
        tokens.unwrap_or(0),
        elapsed.unwrap_or(0),
        &updated_at,
        &completed_at,
    )?;

    if !transitioned {
        return Err(anyhow!(
            "Failed to complete task: another process may have changed its state"
        ));
    }

    // Re-fetch for subsequent logic
    let task = db.get_task(&task_id)?.unwrap();

    let mut goal = db
        .get_goal(&task.goal_id)?
        .ok_or_else(|| anyhow!("Goal not found: {}", task.goal_id))?;

    goal.updated_at = Timestamp::now();

    let all_tasks = db.list_tasks(&goal.id)?;

    // Unblock tasks that were waiting on this task
    let completed_task_id = task.id.clone();
    let mut unblocked_task_ids = Vec::new();

    for mut dependent_task in all_tasks.iter().cloned() {
        if dependent_task.state == TaskState::Blocked
            && dependent_task.blocked_by.contains(&completed_task_id)
        {
            // Check if all blocking tasks are now completed
            let all_blockers_done = dependent_task.blocked_by.iter().all(|blocker_id| {
                all_tasks
                    .iter()
                    .any(|t| t.id == *blocker_id && t.state == TaskState::Completed)
            });

            if all_blockers_done {
                dependent_task.state = TaskState::Pending;
                dependent_task.updated_at = Timestamp::now();
                db.update_task(&dependent_task)?;
                unblocked_task_ids.push(dependent_task.id.clone());
            }
        }
    }

    // Refresh task list after unblocking
    let all_tasks = db.list_tasks(&goal.id)?;
    let all_completed = all_tasks.iter().all(|t| t.state == TaskState::Completed);
    let any_failed = all_tasks.iter().any(|t| t.state == TaskState::Failed);

    if all_completed {
        goal.state = GoalState::Completed;
        goal.completed_at = Some(Timestamp::now());
    } else if any_failed {
        goal.state = GoalState::Failed;
    }

    db.update_goal(&goal)?;

    Ok(CompleteResult {
        task,
        unblocked_task_ids,
    })
}

pub fn fail(task_id: String, db: &mut Database) -> Result<Task> {
    let task = db.get_task(&task_id)?;

    if task.is_none() {
        let all_task_ids: Vec<String> = db
            .list_goals()?
            .iter()
            .flat_map(|goal| {
                db.list_tasks(&goal.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| t.id)
            })
            .collect();

        return if let Some(suggestion) = find_similar_id(&task_id, &all_task_ids) {
            Err(anyhow!(
                "Task not found: {task_id}\nDid you mean: {suggestion}"
            ))
        } else {
            Err(anyhow!("Task not found: {task_id}"))
        };
    }

    let task = task.unwrap();

    if task.state != TaskState::InProgress && task.state != TaskState::Verifying {
        return Err(anyhow!(
            "Task must be in 'in_progress' or 'verifying' state to fail. Current state: {}",
            task.state.as_ref()
        ));
    }

    let updated_at = Timestamp::now().to_string();
    let transitioned = db.transition_task_state_from_any(
        &task.id,
        &[&TaskState::InProgress, &TaskState::Verifying],
        &TaskState::Failed,
        &updated_at,
    )?;

    if !transitioned {
        return Err(anyhow!(
            "Failed to mark task as failed: state may have changed"
        ));
    }

    // Re-fetch to get the updated state
    db.get_task(&task_id)?
        .ok_or_else(|| anyhow!("Task not found after transition"))
}

pub fn retry(task_id: String, db: &mut Database) -> Result<Task> {
    let task = db.get_task(&task_id)?;

    if task.is_none() {
        let all_task_ids: Vec<String> = db
            .list_goals()?
            .iter()
            .flat_map(|goal| {
                db.list_tasks(&goal.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| t.id)
            })
            .collect();

        return if let Some(suggestion) = find_similar_id(&task_id, &all_task_ids) {
            Err(anyhow!(
                "Task not found: {task_id}\nDid you mean: {suggestion}"
            ))
        } else {
            Err(anyhow!("Task not found: {task_id}"))
        };
    }

    let task = task.unwrap();

    if task.state != TaskState::Failed {
        return Err(anyhow!(
            "Task must be in 'failed' state to retry. Current state: {}",
            task.state.as_ref()
        ));
    }

    let updated_at = Timestamp::now().to_string();
    let transitioned = db.retry_task(&task.id, &updated_at)?;

    if !transitioned {
        return Err(anyhow!("Failed to retry task: state may have changed"));
    }

    // Re-fetch to get updated retry_count
    db.get_task(&task_id)?
        .ok_or_else(|| anyhow!("Task not found after retry"))
}

pub fn comment(task_id: String, text: String, db: &mut Database) -> Result<Task> {
    let task = db.get_task(&task_id)?;

    if task.is_none() {
        let all_task_ids: Vec<String> = db
            .list_goals()?
            .iter()
            .flat_map(|goal| {
                db.list_tasks(&goal.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| t.id)
            })
            .collect();

        return if let Some(suggestion) = find_similar_id(&task_id, &all_task_ids) {
            Err(anyhow!(
                "Task not found: {task_id}\nDid you mean: {suggestion}"
            ))
        } else {
            Err(anyhow!("Task not found: {task_id}"))
        };
    }

    let mut task = task.unwrap();

    let comment = Comment {
        id: generate_id(),
        text,
        created_at: Timestamp::now(),
    };

    task.comments.push(comment);
    task.updated_at = Timestamp::now();

    db.update_task(&task)?;

    Ok(task)
}
