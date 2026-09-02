use anyhow::Result;
use serde::Serialize;

use crate::db::Database;
use crate::models::{Goal, GoalState};

/// Result of a `clean` run: what was found and what got removed.
#[derive(Debug, Serialize)]
pub struct CleanResult {
    /// Number of goals eligible for cleaning.
    pub candidates: usize,
    /// The goals that were actually archived or deleted.
    pub removed: Vec<Goal>,
    /// Whether removal deleted goals outright rather than archiving them.
    pub purge: bool,
    /// Whether every completed/cancelled goal was force-removed without confirmation.
    pub force: bool,
}

/// Archive or delete completed/cancelled goals, prompting per-goal via
/// `confirm` unless `all` or `force` is set. `on_removed` is invoked
/// immediately after each goal is actually archived/deleted, so callers can
/// report progress interleaved with the confirmation prompts.
pub fn run(
    all: bool,
    force: bool,
    purge: bool,
    db: &mut Database,
    mut confirm: impl FnMut(&Goal, bool) -> Result<bool>,
    mut on_removed: impl FnMut(&Goal, bool) -> Result<()>,
) -> Result<CleanResult> {
    let goals: Vec<_> = db
        .list_goals()
        .into_iter()
        .filter(|g| force || g.state() == GoalState::Completed || g.state() == GoalState::Cancelled)
        .cloned()
        .collect();

    let candidates = goals.len();
    let mut removed = Vec::new();

    for goal in goals {
        let should_remove = all || force || confirm(&goal, purge)?;

        if should_remove {
            if purge {
                db.delete_goal(goal.id())?;
            } else {
                db.archive_goal(goal.id())?;
            }
            on_removed(&goal, purge)?;
            removed.push(goal);
        }
    }

    Ok(CleanResult {
        candidates,
        removed,
        purge,
        force,
    })
}
