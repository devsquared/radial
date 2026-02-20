use anyhow::{Result, anyhow};
use serde::Serialize;

use crate::db::Database;
use crate::helpers::find_similar_id;
use crate::models::{Goal, Metrics, Task};

/// Full detail view of either a goal or a task.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ShowResult {
    Goal {
        #[serde(flatten)]
        goal: Goal,
        tasks: Vec<Task>,
        metrics: Metrics,
    },
    Task(Task),
}

pub fn run(id: &str, db: &Database) -> Result<ShowResult> {
    // Try task first (more common lookup), then goal
    if let Some(task) = db.get_task(id) {
        return Ok(ShowResult::Task(task.clone()));
    }

    if let Some(goal) = db.get_goal(id) {
        let tasks: Vec<Task> = db.list_tasks(id).into_iter().cloned().collect();
        let metrics = db.compute_goal_metrics(id);
        return Ok(ShowResult::Goal {
            goal: goal.clone(),
            tasks,
            metrics,
        });
    }

    // Not found — try fuzzy matching for a helpful error
    let all_ids = collect_all_ids(db);
    let refs: Vec<&str> = all_ids.iter().map(String::as_str).collect();

    if let Some(suggestion) = find_similar_id(id, &refs) {
        Err(anyhow!("Not found: {id}\nDid you mean: {suggestion}"))
    } else {
        Err(anyhow!("Not found: {id}"))
    }
}

fn collect_all_ids(db: &Database) -> Vec<String> {
    let mut ids: Vec<String> = db.list_goals().iter().map(|g| g.id().to_string()).collect();
    for goal in db.list_goals() {
        for task in db.list_tasks(goal.id()) {
            ids.push(task.id().to_string());
        }
    }
    ids
}
