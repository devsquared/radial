use anyhow::{Result, anyhow};
use serde::Serialize;

use crate::db::Database;
use crate::models::TaskState;

#[derive(Debug, Serialize)]
pub struct CompactCandidate {
    pub id: String,
    pub goal_id: String,
    pub description: String,
    pub state: String,
    pub completed_at: Option<String>,
}

pub fn analyze(goal: Option<&str>, db: &Database) -> Result<Vec<CompactCandidate>> {
    let goals = match goal {
        Some(id) => {
            let goal_id = db.resolve_any_goal(id).map_err(|e| anyhow!("{e}"))?;
            let g = db
                .get_goal(&goal_id)
                .ok_or_else(|| anyhow!("Goal not found: {id}"))?;
            vec![g]
        }
        None => db.list_goals(),
    };

    let mut candidates = Vec::new();
    for g in &goals {
        for task in db.list_tasks(g.id()) {
            if task.compacted() {
                continue;
            }
            if task.state() != TaskState::Completed && task.state() != TaskState::Failed {
                continue;
            }
            candidates.push(CompactCandidate {
                id: task.id().to_string(),
                goal_id: task.goal_id().to_string(),
                description: task.description().to_string(),
                state: task.state().as_ref().to_string(),
                completed_at: task.completed_at().map(|t| t.to_string()),
            });
        }
    }

    Ok(candidates)
}

pub fn apply(task_id: &str, summary: String, db: &mut Database) -> Result<String> {
    let base = db.base_path().to_path_buf();
    let tid = db.resolve_any_task(task_id).map_err(|e| anyhow!("{e}"))?;
    let task = db
        .get_task_mut(&tid)
        .ok_or_else(|| anyhow!("Task not found: {task_id}"))?;

    if !task.compact(summary) {
        if task.compacted() {
            return Err(anyhow!("Task {task_id} is already compacted"));
        }
        return Err(anyhow!(
            "Task {task_id} is in state '{}' — only completed or failed tasks can be compacted",
            task.state().as_ref()
        ));
    }

    task.write_file(&base)?;
    Ok(task_id.to_string())
}

/// Count tasks eligible for compaction across all goals.
pub fn count_candidates(db: &Database) -> usize {
    db.list_goals()
        .iter()
        .flat_map(|g| db.list_tasks(g.id()))
        .filter(|t| {
            !t.compacted() && (t.state() == TaskState::Completed || t.state() == TaskState::Failed)
        })
        .count()
}
