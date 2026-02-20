use anyhow::{Result, anyhow};

use crate::db::Database;
use crate::models::{Contract, Goal, Task};

pub fn goal(goal_id: &str, description: String, db: &mut Database) -> Result<Goal> {
    let base = db.base_path().to_path_buf();
    let goal = db
        .get_goal_mut(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    goal.set_description(description);
    goal.write_file(&base)?;
    Ok(goal.clone())
}

pub fn task(
    task_id: &str,
    description: Option<String>,
    receives: Option<String>,
    produces: Option<String>,
    verify: Option<String>,
    blocked_by: Option<Vec<String>>,
    db: &mut Database,
) -> Result<Task> {
    let base = db.base_path().to_path_buf();
    let task = db
        .get_task_mut(task_id)
        .ok_or_else(|| anyhow!("Task not found: {task_id}"))?;

    if let Some(desc) = description {
        task.set_description(desc);
    }

    // Update contract fields, merging with existing values
    if receives.is_some() || produces.is_some() || verify.is_some() {
        let existing = task.contract();
        let new_receives = receives.unwrap_or_else(|| existing.map_or(String::new(), |c| c.receives().to_string()));
        let new_produces = produces.unwrap_or_else(|| existing.map_or(String::new(), |c| c.produces().to_string()));
        let new_verify = verify.unwrap_or_else(|| existing.map_or(String::new(), |c| c.verify().to_string()));
        task.set_contract(Contract::new(new_receives, new_produces, new_verify));
    }

    if let Some(deps) = blocked_by {
        task.set_blocked_by(deps);
    }

    task.write_file(&base)?;
    Ok(task.clone())
}
