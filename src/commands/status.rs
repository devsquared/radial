use anyhow::{Result, anyhow};
use serde::Serialize;

use crate::db::Database;
use crate::models::{Goal, Metrics, Task};

#[derive(Debug, Serialize)]
pub struct GoalStatus {
    #[serde(flatten)]
    goal: Goal,
    tasks: Vec<Task>,
    metrics: Metrics,
}

impl GoalStatus {
    pub fn goal(&self) -> &Goal {
        &self.goal
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }
}

#[derive(Debug, Serialize)]
pub struct GoalSummary {
    #[serde(flatten)]
    goal: Goal,
    computed_metrics: Metrics,
}

impl GoalSummary {
    pub fn goal(&self) -> &Goal {
        &self.goal
    }

    pub fn computed_metrics(&self) -> &Metrics {
        &self.computed_metrics
    }
}

/// Result of a status query - can be a single task, single goal, or all goals.
#[derive(Debug)]
pub enum StatusResult {
    Task(Task),
    Goal(GoalStatus),
    AllGoals(Vec<GoalSummary>),
}

pub fn run(
    goal_id: Option<String>,
    task_id: Option<String>,
    db: &Database,
) -> Result<StatusResult> {
    if let Some(tid) = task_id {
        return get_task(&tid, db).map(StatusResult::Task);
    }

    if let Some(gid) = goal_id {
        return get_goal(&gid, db).map(StatusResult::Goal);
    }

    Ok(StatusResult::AllGoals(get_all_goals(db)))
}

fn get_task(task_id: &str, db: &Database) -> Result<Task> {
    db.get_task(task_id)
        .cloned()
        .ok_or_else(|| anyhow!("Task not found: {task_id}"))
}

fn get_goal(goal_id: &str, db: &Database) -> Result<GoalStatus> {
    let goal = db
        .get_goal(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?
        .clone();

    let tasks: Vec<Task> = db.list_tasks(goal_id).into_iter().cloned().collect();
    let metrics = db.compute_goal_metrics(goal_id);

    Ok(GoalStatus {
        goal,
        tasks,
        metrics,
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
