use anyhow::{Result, anyhow};

use crate::db::Database;
use crate::helpers::{detect_cycle, find_similar_id};
use crate::id::{GoalId, TaskId};
use crate::models::{Contract, Goal, Priority, Task};

/// Update a goal's description.
pub fn goal(goal_id: &GoalId, description: String, db: &mut Database) -> Result<Goal> {
    let goal = db
        .get_goal_mut(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    goal.set_description(description);
    let goal = goal.clone();
    db.save_goal(&goal)?;
    Ok(goal)
}

/// Update a task's description, priority, contract fields, and/or
/// `blocked_by` list. Each parameter left `None` is unchanged; contract
/// fields merge with the existing contract rather than replacing it wholesale.
///
/// Validates that new `blocked_by` IDs exist in the same goal, aren't the
/// task itself, and don't introduce a dependency cycle.
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

        // Check for cycles
        if let Some(cycle_path) = detect_cycle(task_id, dep_ids, &all_tasks) {
            let path_str: Vec<String> = cycle_path.iter().map(ToString::to_string).collect();
            return Err(anyhow!(
                "Circular dependency detected: {}",
                path_str.join(" -> ")
            ));
        }
    }

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

    let task = task.clone();
    db.save_task(&task)?;
    Ok(task)
}
