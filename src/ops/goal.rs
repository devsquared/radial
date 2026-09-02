use anyhow::{Result, anyhow};
use jiff::Timestamp;

use crate::db::Database;
use crate::helpers::find_similar_id;
use crate::id::{GoalId, TaskId};
use crate::models::{Goal, GoalState, Metrics, TaskState};
use crate::ops::task;

/// Create a new goal with the next sequence number.
pub fn create(description: String, db: &mut Database) -> Result<Goal> {
    let now = Timestamp::now();
    let seq = db.next_goal_seq();
    let goal = Goal::new(
        GoalId::new(),
        Some(seq),
        description,
        GoalState::Pending,
        now,
        now,
        None,
        Metrics::default(),
    );

    db.create_goal(goal.clone())?;
    Ok(goal)
}

/// List every non-archived goal.
pub fn list(db: &Database) -> Vec<Goal> {
    db.list_goals().into_iter().cloned().collect()
}

/// Cancel a goal and all its non-terminal tasks.
///
/// Returns the cancelled goal and the IDs of all tasks that were cancelled.
pub fn cancel(
    goal_id: &GoalId,
    reason: Option<String>,
    author: &str,
    db: &mut Database,
) -> Result<(Goal, Vec<TaskId>)> {
    let goal = db.get_goal(goal_id).ok_or_else(|| {
        let all_goals = db.list_goals();
        let goal_ids: Vec<&str> = all_goals.iter().map(|g| g.id().as_ref()).collect();

        if let Some(suggestion) = find_similar_id(goal_id.as_ref(), &goal_ids) {
            anyhow!("Goal not found: {goal_id}\nDid you mean: {suggestion}")
        } else {
            anyhow!("Goal not found: {goal_id}")
        }
    })?;

    if goal.state() == GoalState::Cancelled {
        return Err(anyhow!("Goal is already cancelled"));
    }

    // Collect IDs of non-terminal tasks to cancel
    let tasks_to_cancel: Vec<TaskId> = db
        .list_tasks(goal_id)
        .iter()
        .filter(|t| {
            // Skip Completed tasks - completed work is history
            // Skip already-Cancelled tasks
            t.state() != TaskState::Completed && t.state() != TaskState::Cancelled
        })
        .map(|t| t.id().clone())
        .collect();

    let mut cancelled_task_ids = Vec::new();

    // Cancel each non-terminal task
    for task_id in &tasks_to_cancel {
        task::cancel(task_id, reason.clone(), author, false, db)?;
        cancelled_task_ids.push(task_id.clone());
    }

    // Recompute metrics to include cancelled count
    let metrics = db.compute_goal_metrics(goal_id);

    // Mark the goal as cancelled and update metrics
    let goal_mut = db.get_goal_mut(goal_id).unwrap();
    goal_mut.mark_cancelled();
    goal_mut.set_metrics(metrics);
    let cancelled_goal = goal_mut.clone();
    db.save_goal(&cancelled_goal)?;

    Ok((cancelled_goal, cancelled_task_ids))
}
