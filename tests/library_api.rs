//! Drives radial purely as a library, through `ops` alone, to prove the crate
//! is usable as a dependency and not only via the `rd` binary. See
//! `PR 6` in the core-seam plan: this is the forcing function for the
//! re-export list PR 7 has to produce.

use radial::db::Database;
use radial::models::{GoalState, Priority, TaskState};
use radial::ops;
use std::fs;
use tempfile::TempDir;

#[test]
fn drives_a_goal_through_create_start_complete_via_ops() {
    let temp_dir = TempDir::new().expect("failed to create temp directory");
    let radial_dir = temp_dir.path().join(".radial");
    fs::create_dir_all(&radial_dir).expect("failed to create .radial directory");

    let (mut db, _lock) =
        Database::open_for_write(&radial_dir).expect("failed to open database for write");

    let goal = ops::goal::create("Ship the library seam".to_string(), &mut db)
        .expect("failed to create goal");
    assert_eq!(goal.state(), GoalState::Pending);

    let task = ops::task::create(
        goal.id(),
        "Write tests/library_api.rs".to_string(),
        Priority::P1,
        None,
        Some("A crate usable only through its own tests".to_string()),
        Some("An integration test exercising ops directly".to_string()),
        Some("cargo nextest run --all-targets passes".to_string()),
        None,
        &mut db,
    )
    .expect("failed to create task");
    assert_eq!(task.state(), TaskState::Pending);

    let task =
        ops::task::start(task.id(), "claude-code", false, &mut db).expect("failed to start task");
    assert_eq!(task.state(), TaskState::InProgress);

    let goal_after_start = db
        .get_goal(goal.id())
        .expect("goal missing after task start");
    assert_eq!(goal_after_start.state(), GoalState::InProgress);

    let result = ops::task::complete(
        task.id(),
        "Wrote the library-consumer test".to_string(),
        None,
        None,
        None,
        &mut db,
    )
    .expect("failed to complete task");
    assert_eq!(result.task.state(), TaskState::Completed);
    assert!(result.unblocked_task_ids.is_empty());

    let goal_after_complete = db
        .get_goal(goal.id())
        .expect("goal missing after task complete");
    assert_eq!(goal_after_complete.state(), GoalState::Completed);
}
