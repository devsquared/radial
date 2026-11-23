use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use serde::Serialize;

use crate::db::Database;
use crate::id::GoalId;
use crate::id::TaskId;
use crate::models::{Goal, Metrics, Task};
use crate::ops::task::tags_matching;

/// A single goal with its tasks and aggregate metrics.
#[derive(Debug, Serialize)]
pub struct GoalStatus {
    #[serde(flatten)]
    goal: Goal,
    tasks: Vec<Task>,
    /// Named distinctly from the flattened `Goal`'s own persisted `metrics`
    /// field so the two don't collide under the same JSON key.
    computed_metrics: Metrics,
}

impl GoalStatus {
    /// The goal.
    pub fn goal(&self) -> &Goal {
        &self.goal
    }

    /// The goal's tasks.
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// The goal's freshly computed aggregate metrics.
    pub fn computed_metrics(&self) -> &Metrics {
        &self.computed_metrics
    }
}

/// A goal with freshly computed metrics, for a listing of all goals.
#[derive(Debug, Serialize)]
pub struct GoalSummary {
    #[serde(flatten)]
    goal: Goal,
    computed_metrics: Metrics,
}

impl GoalSummary {
    /// The goal.
    pub fn goal(&self) -> &Goal {
        &self.goal
    }

    /// The goal's freshly computed metrics.
    pub fn computed_metrics(&self) -> &Metrics {
        &self.computed_metrics
    }
}

/// Result of a status query - can be a single task, single goal, or all goals.
#[derive(Debug)]
pub enum StatusResult {
    /// Status of a single task.
    Task(Task),
    /// Status of a single goal and its tasks.
    Goal(GoalStatus),
    /// Summary of every goal in the database.
    AllGoals(Vec<GoalSummary>),
}

/// Look up status for a task, a goal, or (if neither ID is given) every goal.
pub fn run(
    goal_id: Option<GoalId>,
    task_id: Option<TaskId>,
    assignee: Option<String>,
    tag_filter: &BTreeMap<String, String>,
    db: &Database,
) -> Result<StatusResult> {
    if let Some(tid) = task_id {
        return get_task(&tid, db).map(StatusResult::Task);
    }

    if let Some(gid) = goal_id {
        return get_goal(&gid, assignee.as_deref(), tag_filter, db).map(StatusResult::Goal);
    }

    Ok(StatusResult::AllGoals(get_all_goals(db)))
}

fn get_task(task_id: &TaskId, db: &Database) -> Result<Task> {
    db.get_task(task_id)
        .cloned()
        .ok_or_else(|| anyhow!("Task not found: {task_id}"))
}

fn get_goal(
    goal_id: &GoalId,
    assignee: Option<&str>,
    tag_filter: &BTreeMap<String, String>,
    db: &Database,
) -> Result<GoalStatus> {
    let goal = db
        .get_goal(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?
        .clone();

    let tasks: Vec<Task> = db
        .list_tasks(goal_id)
        .into_iter()
        .filter(|t| assignee.is_none_or(|a| t.assignee() == Some(a)))
        .filter(|t| tags_matching(t, tag_filter))
        .cloned()
        .collect();
    let metrics = db.compute_goal_metrics(goal_id);

    Ok(GoalStatus {
        goal,
        tasks,
        computed_metrics: metrics,
    })
}

fn get_all_goals(db: &Database) -> Vec<GoalSummary> {
    db.list_goals()
        .into_iter()
        .map(|goal| {
            let computed_metrics = db.compute_goal_metrics(goal.id());
            GoalSummary {
                goal: goal.clone(),
                computed_metrics,
            }
        })
        .collect()
}
