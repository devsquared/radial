use anyhow::{anyhow, Result};

use crate::db::Database;
use crate::models::{Task, TaskState};

pub fn run(goal_id: &str, db: &Database) -> Result<Vec<Task>> {
    db.get_goal(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    Ok(db
        .list_tasks(goal_id)
        .into_iter()
        .filter(|t| t.state == TaskState::Pending && t.contract.is_some())
        .cloned()
        .collect())
}
