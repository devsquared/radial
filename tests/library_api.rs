//! Drives radial purely as a library, through `ops` alone, to prove the crate
//! is usable as a dependency and not only via the `rd` binary.

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

#[test]
fn derives_elapsed_ms_from_started_at_when_not_supplied() {
    let temp_dir = TempDir::new().expect("failed to create temp directory");
    let radial_dir = temp_dir.path().join(".radial");
    fs::create_dir_all(&radial_dir).expect("failed to create .radial directory");

    let (mut db, _lock) =
        Database::open_for_write(&radial_dir).expect("failed to open database for write");

    let goal =
        ops::goal::create("Time a task".to_string(), &mut db).expect("failed to create goal");
    let task = ops::task::create(
        goal.id(),
        "Do something measurable".to_string(),
        Priority::P2,
        None,
        Some("receives".to_string()),
        Some("produces".to_string()),
        Some("verify".to_string()),
        None,
        &mut db,
    )
    .expect("failed to create task");

    let task =
        ops::task::start(task.id(), "claude-code", false, &mut db).expect("failed to start task");
    assert!(task.started_at().is_some());

    std::thread::sleep(std::time::Duration::from_millis(20));

    let result = ops::task::complete(task.id(), "Finished".to_string(), None, None, None, &mut db)
        .expect("failed to complete task");

    assert!(
        result.task.metrics().elapsed_ms() >= 20,
        "expected derived elapsed_ms to reflect real wall-clock time, got {}",
        result.task.metrics().elapsed_ms()
    );
}

#[test]
fn explicit_elapsed_overrides_derivation_from_started_at() {
    let temp_dir = TempDir::new().expect("failed to create temp directory");
    let radial_dir = temp_dir.path().join(".radial");
    fs::create_dir_all(&radial_dir).expect("failed to create .radial directory");

    let (mut db, _lock) =
        Database::open_for_write(&radial_dir).expect("failed to open database for write");

    let goal = ops::goal::create("Time a task explicitly".to_string(), &mut db)
        .expect("failed to create goal");
    let task = ops::task::create(
        goal.id(),
        "Do something with a known duration".to_string(),
        Priority::P2,
        None,
        Some("receives".to_string()),
        Some("produces".to_string()),
        Some("verify".to_string()),
        None,
        &mut db,
    )
    .expect("failed to create task");

    let task =
        ops::task::start(task.id(), "claude-code", false, &mut db).expect("failed to start task");

    let result = ops::task::complete(
        task.id(),
        "Finished".to_string(),
        None,
        None,
        Some(1234),
        &mut db,
    )
    .expect("failed to complete task");

    assert_eq!(result.task.metrics().elapsed_ms(), 1234);
}
