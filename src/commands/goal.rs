use anyhow::Result;
use jiff::Timestamp;

use crate::db::Database;
use crate::id::generate_id;
use crate::models::{Goal, GoalState, Metrics};

pub fn create(description: String, db: &mut Database) -> Result<Goal> {
    let now = Timestamp::now();
    let goal = Goal::new(
        generate_id(),
        None,
        description,
        GoalState::Pending,
        now,
        now,
        None,
        Metrics::default(),
    );

    db.create_goal(goal.clone())?;
    Ok(goal)
}

pub fn list(db: &Database) -> Vec<Goal> {
    db.list_goals().into_iter().cloned().collect()
}
