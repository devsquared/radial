use anyhow::{Result, anyhow};

use crate::db::Database;
use crate::id::GoalId;
use crate::models::{Priority, Task, TaskState};

pub fn run(
    goal_id: &GoalId,
    priority: Option<&Priority>,
    db: &Database,
) -> Result<Vec<(Task, Option<Task>)>> {
    db.get_goal(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    let mut tasks: Vec<(Task, Option<Task>)> = db
        .list_tasks(goal_id)
        .into_iter()
        .filter(|t| t.state() == TaskState::Pending && t.contract().is_some())
        .filter(|t| !db.has_subtasks(t.id()))
        .filter(|t| priority.is_none_or(|p| t.priority() == *p))
        .map(|t| {
            let parent = t.parent_id().and_then(|pid| db.get_task(pid)).cloned();
            (t.clone(), parent)
        })
        .collect();

    tasks.sort_by_key(|(t, _)| t.priority());
    Ok(tasks)
}
