use std::collections::{HashMap, HashSet};

use strsim::levenshtein;

use crate::id::TaskId;
use crate::models::Task;

/// Find the most similar ID from a list of candidates
pub fn find_similar_id<'a>(target: &str, candidates: &[&'a str]) -> Option<&'a str> {
    candidates
        .iter()
        .map(|&candidate| (candidate, levenshtein(target, candidate)))
        .filter(|(_, distance)| *distance <= 2)
        .min_by_key(|(_, distance)| *distance)
        .map(|(id, _)| id)
}

/// Detect if adding the given `blocked_by` edges to `task_id` would create a cycle.
///
/// Walks the dependency graph starting from each blocker in `new_blocked_by`,
/// following existing `blocked_by` edges. If any path leads back to `task_id`,
/// returns `Some(cycle_path)` with the IDs forming the cycle.
pub fn detect_cycle(
    task_id: &TaskId,
    new_blocked_by: &[TaskId],
    tasks: &[&Task],
) -> Option<Vec<TaskId>> {
    let blocked_by_map: HashMap<&TaskId, Vec<&TaskId>> = tasks
        .iter()
        .map(|t| (t.id(), t.blocked_by().iter().collect()))
        .collect();

    for blocker in new_blocked_by {
        let mut visited = HashSet::new();
        let mut path = vec![task_id.clone(), blocker.clone()];

        if blocker == task_id {
            return Some(path);
        }

        visited.insert(task_id);
        visited.insert(blocker);

        if dfs_finds_cycle(blocker, task_id, &blocked_by_map, &mut visited, &mut path) {
            return Some(path);
        }
    }

    None
}

fn dfs_finds_cycle<'a>(
    current: &'a TaskId,
    target: &'a TaskId,
    blocked_by_map: &HashMap<&'a TaskId, Vec<&'a TaskId>>,
    visited: &mut HashSet<&'a TaskId>,
    path: &mut Vec<TaskId>,
) -> bool {
    if let Some(deps) = blocked_by_map.get(current) {
        for &dep in deps {
            if dep == target {
                path.push(dep.clone());
                return true;
            }
            if visited.insert(dep) {
                path.push(dep.clone());
                if dfs_finds_cycle(dep, target, blocked_by_map, visited, path) {
                    return true;
                }
                path.pop();
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{GoalId, TaskId};
    use crate::models::{Priority, Task, TaskState};
    use jiff::Timestamp;

    #[test]
    fn test_find_similar_id() {
        let candidates = vec!["t8zwaROl", "xYz9Kp2m", "V1StGXR8"];

        assert_eq!(find_similar_id("t8zwaRO1", &candidates), Some("t8zwaROl"));

        assert_eq!(find_similar_id("xYz9Kp2n", &candidates), Some("xYz9Kp2m"));

        // Very different ID should return None
        assert_eq!(find_similar_id("zzzzz", &candidates), None);
    }

    fn make_task(id: &str, blocked_by: Vec<&str>) -> Task {
        let now = Timestamp::now();
        Task::new(
            TaskId::from(id.to_string()),
            GoalId::from("g1".to_string()),
            "test".to_string(),
            Priority::default(),
            None,
            TaskState::Pending,
            blocked_by
                .into_iter()
                .map(|s| TaskId::from(s.to_string()))
                .collect(),
            now,
            now,
        )
    }

    #[test]
    fn test_no_cycle() {
        // A -> B -> C (linear chain, no cycle)
        let a = make_task("AAAAAAAA", vec![]);
        let b = make_task("BBBBBBBB", vec!["AAAAAAAA"]);
        let c = make_task("CCCCCCCC", vec!["BBBBBBBB"]);
        let tasks: Vec<&Task> = vec![&a, &b, &c];

        // Adding D blocked_by C — no cycle
        let d_id = TaskId::from("DDDDDDDD".to_string());
        let blocked_by = vec![TaskId::from("CCCCCCCC".to_string())];
        assert!(detect_cycle(&d_id, &blocked_by, &tasks).is_none());
    }

    #[test]
    fn test_direct_cycle() {
        // A exists, try to make A blocked_by A
        let a = make_task("AAAAAAAA", vec![]);
        let tasks: Vec<&Task> = vec![&a];

        let a_id = TaskId::from("AAAAAAAA".to_string());
        let blocked_by = vec![TaskId::from("AAAAAAAA".to_string())];
        let cycle = detect_cycle(&a_id, &blocked_by, &tasks);
        assert!(cycle.is_some());
    }

    #[test]
    fn test_transitive_cycle() {
        // A -> B (B blocked_by A). Now try to make A blocked_by B — cycle: A -> B -> A
        let a = make_task("AAAAAAAA", vec![]);
        let b = make_task("BBBBBBBB", vec!["AAAAAAAA"]);
        let tasks: Vec<&Task> = vec![&a, &b];

        let a_id = TaskId::from("AAAAAAAA".to_string());
        let blocked_by = vec![TaskId::from("BBBBBBBB".to_string())];
        let cycle = detect_cycle(&a_id, &blocked_by, &tasks);
        assert!(cycle.is_some());
        let path = cycle.unwrap();
        // Path should contain A, B, A
        assert!(path.len() >= 3);
    }

    #[test]
    fn test_longer_cycle() {
        // A -> B -> C. Now try to make A blocked_by C — cycle: A -> C -> B -> A
        let a = make_task("AAAAAAAA", vec![]);
        let b = make_task("BBBBBBBB", vec!["AAAAAAAA"]);
        let c = make_task("CCCCCCCC", vec!["BBBBBBBB"]);
        let tasks: Vec<&Task> = vec![&a, &b, &c];

        let a_id = TaskId::from("AAAAAAAA".to_string());
        let blocked_by = vec![TaskId::from("CCCCCCCC".to_string())];
        let cycle = detect_cycle(&a_id, &blocked_by, &tasks);
        assert!(cycle.is_some());
    }
}
