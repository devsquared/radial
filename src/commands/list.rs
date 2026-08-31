use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::Result;

use crate::db::Database;
use crate::id::TaskId;
use crate::models::{Goal, Metrics, Task};

pub struct GoalWithTasks {
    pub goal: Goal,
    pub tasks: Vec<Task>,
    pub metrics: Metrics,
}

pub fn run(db: &Database) -> Result<Vec<GoalWithTasks>> {
    let results = db
        .list_goals()
        .into_iter()
        .map(|goal| {
            let tasks = topo_sort(db.list_tasks(goal.id()));
            let metrics = db.compute_goal_metrics(goal.id());
            GoalWithTasks {
                goal: goal.clone(),
                tasks,
                metrics,
            }
        })
        .collect();

    Ok(results)
}

pub fn run_archived(db: &Database) -> Result<Vec<GoalWithTasks>> {
    let archived_goals = db.list_archived_goals()?;
    let results = archived_goals
        .into_iter()
        .map(|goal| GoalWithTasks {
            goal,
            tasks: Vec::new(), // Archived goals don't load tasks
            metrics: Metrics::default(),
        })
        .collect();

    Ok(results)
}

/// Topological sort of tasks by `blocked_by` dependencies.
/// Tasks with no blockers come first. Falls back to creation order for ties.
fn topo_sort(tasks: Vec<&Task>) -> Vec<Task> {
    let task_ids: HashSet<&TaskId> = tasks.iter().map(|t| t.id()).collect();

    // Build adjacency: for each task, count how many in-graph blockers it has
    let mut in_degree: HashMap<&TaskId, usize> = HashMap::new();
    let mut dependents: HashMap<&TaskId, Vec<&TaskId>> = HashMap::new();

    for task in &tasks {
        let blocked_count = task
            .blocked_by()
            .iter()
            .filter(|b| task_ids.contains(b))
            .count();
        in_degree.insert(task.id(), blocked_count);

        // Register this task as a dependent of each blocker
        for blocker in task.blocked_by() {
            if task_ids.contains(blocker) {
                dependents.entry(blocker).or_default().push(task.id());
            }
        }
    }

    // Kahn's algorithm
    let mut queue: VecDeque<&TaskId> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut ordered_ids: Vec<&TaskId> = Vec::with_capacity(tasks.len());
    while let Some(id) = queue.pop_front() {
        ordered_ids.push(id);
        if let Some(deps) = dependents.get(id) {
            for &dep in deps {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }
    }

    // Build lookup and return in topo order
    let task_map: HashMap<&TaskId, &Task> = tasks.iter().map(|t| (t.id(), *t)).collect();
    ordered_ids
        .iter()
        .filter_map(|id| task_map.get(id))
        .map(|t| (*t).clone())
        .collect()
}
