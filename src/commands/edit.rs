use anyhow::{Result, anyhow};

use crate::db::Database;
use crate::helpers::find_similar_id;
use crate::id::{GoalId, TaskId};
use crate::models::{Contract, Goal, Priority, Task};

pub fn goal(goal_id: &GoalId, description: String, db: &mut Database) -> Result<Goal> {
    let base = db.base_path().to_path_buf();
    let goal = db
        .get_goal_mut(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    goal.set_description(description);
    goal.write_file(&base)?;
    Ok(goal.clone())
}

#[allow(clippy::too_many_arguments)]
pub fn task(
    task_id: &TaskId,
    description: Option<String>,
    priority: Option<Priority>,
    receives: Option<String>,
    produces: Option<String>,
    verify: Option<String>,
    blocked_by: Option<Vec<TaskId>>,
    db: &mut Database,
) -> Result<Task> {
    // Read the task (immutable) to get its goal_id and validate blocked_by
    let task = db
        .get_task(task_id)
        .ok_or_else(|| anyhow!("Task not found: {task_id}"))?;
    let goal_id = task.goal_id().clone();

    // Validate blocked_by task IDs exist in the same goal
    if let Some(ref dep_ids) = blocked_by {
        let all_tasks = db.list_tasks(&goal_id);
        let existing_task_ids: Vec<&str> = all_tasks.iter().map(|t| t.id().as_ref()).collect();

        for dep_id in dep_ids {
            if dep_id == task_id {
                return Err(anyhow!("Task cannot block itself: {dep_id}"));
            }
            if !existing_task_ids.contains(&dep_id.as_ref()) {
                return if let Some(suggestion) =
                    find_similar_id(dep_id.as_ref(), &existing_task_ids)
                {
                    Err(anyhow!(
                        "Task not found in blocked-by list: {dep_id}\nDid you mean: {suggestion}"
                    ))
                } else {
                    Err(anyhow!(
                        "Task not found in blocked-by list: {dep_id}\nTask must exist in the same goal."
                    ))
                };
            }
        }
    }

    let base = db.base_path().to_path_buf();
    let task = db.get_task_mut(task_id).unwrap();

    if let Some(desc) = description {
        task.set_description(desc);
    }

    if let Some(prio) = priority {
        task.set_priority(prio);
    }

    // Update contract fields, merging with existing values
    if receives.is_some() || produces.is_some() || verify.is_some() {
        let existing = task.contract();
        let new_receives = receives
            .unwrap_or_else(|| existing.map_or(String::new(), |c| c.receives().to_string()));
        let new_produces = produces
            .unwrap_or_else(|| existing.map_or(String::new(), |c| c.produces().to_string()));
        let new_verify =
            verify.unwrap_or_else(|| existing.map_or(String::new(), |c| c.verify().to_string()));
        task.set_contract(Contract::new(new_receives, new_produces, new_verify));
    }

    if let Some(deps) = blocked_by {
        task.set_blocked_by(deps);
    }

    task.write_file(&base)?;
    Ok(task.clone())
}
