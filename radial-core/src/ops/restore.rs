use anyhow::{Result, anyhow};

use crate::db::Database;
use crate::id::GoalId;
use crate::models::Goal;

/// Restore an archived goal back into the active database.
pub fn run(goal_id_str: &str, db: &mut Database) -> Result<Goal> {
    // Try to resolve as full ID or prefix
    let goal_id = GoalId::new_unchecked(goal_id_str.to_string());

    db.restore_goal(&goal_id)?;

    // After restore, reload the database to pick up the restored goal
    db.reload()?;

    // Get the restored goal
    db.get_goal(&goal_id)
        .cloned()
        .ok_or_else(|| anyhow!("Failed to load restored goal: {goal_id}"))
}
