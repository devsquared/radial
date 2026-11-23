use std::collections::BTreeMap;

use anyhow::{Result, anyhow};

use crate::db::Database;
use crate::helpers::{detect_cycle, find_similar_id};
use crate::id::{GoalId, TaskId};
use crate::models::{Contract, Goal, Priority, Task, TaskState};

/// Update a goal's description and/or upsert tags onto it.
pub fn goal(
    goal_id: &GoalId,
    description: String,
    tags: Option<BTreeMap<String, String>>,
    db: &mut Database,
) -> Result<Goal> {
    let goal = db
        .get_goal_mut(goal_id)
        .ok_or_else(|| anyhow!("Goal not found: {goal_id}"))?;

    goal.set_description(description);
    if let Some(tags) = tags {
        goal.merge_tags(tags);
    }
    let goal = goal.clone();
    db.save_goal(&goal)?;
    Ok(goal)
}

/// Update a task's description, priority, contract fields, and/or
/// `blocked_by` list. Each parameter left `None` is unchanged; contract
/// fields merge with the existing contract rather than replacing it wholesale.
///
/// `add_blocked_by` and `remove_blocked_by` modify the existing list;
/// `set_blocked_by` replaces it wholesale and cannot be combined with the
/// other two. Validates that the resulting `blocked_by` IDs exist in the
/// same goal, aren't the task itself, and don't introduce a dependency
/// cycle. If the task hasn't started yet, its state is recomputed from the
/// new list (`Blocked` if any blocker isn't `Completed`, `Pending`
/// otherwise) using the same logic `ops::task::create` applies to a
/// freshly created task.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn task(
    task_id: &TaskId,
    description: Option<String>,
    priority: Option<Priority>,
    receives: Option<String>,
    produces: Option<String>,
    verify: Option<String>,
    add_blocked_by: Option<Vec<TaskId>>,
    remove_blocked_by: Option<Vec<TaskId>>,
    set_blocked_by: Option<Vec<TaskId>>,
    tags: Option<BTreeMap<String, String>>,
    db: &mut Database,
) -> Result<Task> {
    if set_blocked_by.is_some() && (add_blocked_by.is_some() || remove_blocked_by.is_some()) {
        return Err(anyhow!(
            "--set-blocked-by cannot be combined with --add-blocked-by or --remove-blocked-by"
        ));
    }

    // Read the task (immutable) to get its goal_id/state and validate blocked_by
    let task = db
        .get_task(task_id)
        .ok_or_else(|| anyhow!("Task not found: {task_id}"))?;
    let goal_id = task.goal_id().clone();
    let current_state = task.state();

    let new_blocked_by = if let Some(deps) = set_blocked_by {
        Some(deps)
    } else if add_blocked_by.is_some() || remove_blocked_by.is_some() {
        let mut deps = task.blocked_by().to_vec();

        if let Some(ref remove_ids) = remove_blocked_by {
            for id in remove_ids {
                if !deps.contains(id) {
                    return Err(anyhow!("Task not in blocked-by list: {id}"));
                }
            }
            deps.retain(|d| !remove_ids.contains(d));
        }

        if let Some(add_ids) = add_blocked_by {
            for id in add_ids {
                if !deps.contains(&id) {
                    deps.push(id);
                }
            }
        }

        Some(deps)
    } else {
        None
    };

    // Validate the resulting blocked_by list, and check for cycles
    if let Some(ref dep_ids) = new_blocked_by {
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

        if let Some(cycle_path) = detect_cycle(task_id, dep_ids, &all_tasks) {
            let path_str: Vec<String> = cycle_path.iter().map(ToString::to_string).collect();
            return Err(anyhow!(
                "Circular dependency detected: {}",
                path_str.join(" -> ")
            ));
        }
    }

    // Recompute Pending/Blocked state while the snapshot above is still in
    // scope; only meaningful for a task that hasn't started yet.
    let new_state = if matches!(current_state, TaskState::Pending | TaskState::Blocked) {
        new_blocked_by.as_ref().map(|dep_ids| {
            let all_tasks = db.list_tasks(&goal_id);
            let any_incomplete = dep_ids.iter().any(|id| {
                all_tasks
                    .iter()
                    .find(|t| t.id() == id)
                    .is_none_or(|t| t.state() != TaskState::Completed)
            });
            if any_incomplete {
                TaskState::Blocked
            } else {
                TaskState::Pending
            }
        })
    } else {
        None
    };

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

    if let Some(deps) = new_blocked_by {
        task.set_blocked_by(deps);
    }

    if let Some(state) = new_state {
        task.set_blocked_state(state);
    }

    if let Some(tags) = tags {
        task.merge_tags(tags);
    }

    let task = task.clone();
    db.save_task(&task)?;
    Ok(task)
}
