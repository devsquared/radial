use anyhow::{Result, anyhow};

use crate::db::Database;
use crate::models::{Priority, Task, TaskState};

pub fn run(goal_id: &str, priority: Option<&Priority>, db: &Database) -> Result<Vec<Task>> {
    db.get_goal(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    let mut tasks: Vec<Task> = db
        .list_tasks(goal_id)
        .into_iter()
        .filter(|t| t.state() == TaskState::Pending && t.contract().is_some())
        .filter(|t| priority.is_none_or(|p| t.priority() == *p))
        .cloned()
        .collect();
    tasks.sort_by_key(Task::priority);
    Ok(tasks)
}
