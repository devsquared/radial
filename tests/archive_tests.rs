use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[derive(Debug)]
struct TestEnv {
    #[allow(dead_code)]
    temp_dir: TempDir,
    radial_dir: PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let radial_dir = temp_dir.path().join(".radial");
        fs::create_dir_all(&radial_dir).unwrap();

        Self {
            temp_dir,
            radial_dir,
        }
    }

    fn run(&self, args: &[&str]) -> Result<String, String> {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_rd"));
        cmd.current_dir(self.radial_dir.parent().unwrap());
        cmd.args(args);

        let output = cmd.output().expect("Failed to execute command");

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn radial_dir(&self) -> &PathBuf {
        &self.radial_dir
    }
}

#[test]
fn test_archive_completed_goal() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create and complete a goal
    let output = env
        .run(&["goal", "create", "Test goal"])
        .expect("Create failed");
    let goal_id = extract_goal_id(&output);

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            "Task 1",
            "--receives",
            "none",
            "--produces",
            "result",
            "--verify",
            "done",
        ])
        .expect("Create task failed");
    let task_id = extract_task_id(&output);

    env.run(&["task", "start", &task_id, "--assignee", "test"])
        .expect("Start failed");
    env.run(&["task", "complete", &task_id, "--result", "done"])
        .expect("Complete failed");

    // Archive the goal
    let output = env.run(&["clean", "--all"]).expect("Clean failed");
    assert!(output.contains("Archived"));
    assert!(output.contains(&goal_id));

    // Verify goal is not in active list
    let output = env.run(&["list"]).expect("List failed");
    assert!(!output.contains(&goal_id));

    // Verify archive directory exists
    let archive_dir = env.radial_dir().join("archive").join(&goal_id);
    assert!(archive_dir.exists());
    assert!(archive_dir.join("goal.toml").exists());
}

#[test]
fn test_archive_cancelled_goal() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create a goal and cancel it
    let output = env
        .run(&["goal", "create", "Test goal"])
        .expect("Create failed");
    let goal_id = extract_goal_id(&output);

    let output = env
        .run(&["task", "create", &goal_id, "Task 1"])
        .expect("Create task failed");
    let _task_id = extract_task_id(&output);

    env.run(&["goal", "cancel", &goal_id, "--reason", "testing"])
        .expect("Cancel failed");

    // Archive the cancelled goal
    let output = env.run(&["clean", "--all"]).expect("Clean failed");
    assert!(output.contains("Archived"));
    assert!(output.contains(&goal_id));

    // Verify it's archived
    let archive_dir = env.radial_dir().join("archive").join(&goal_id);
    assert!(archive_dir.exists());
}

#[test]
fn test_restore_archived_goal() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create, complete, and archive a goal
    let output = env
        .run(&["goal", "create", "Test goal"])
        .expect("Create failed");
    let goal_id = extract_goal_id(&output);

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            "Task 1",
            "--receives",
            "none",
            "--produces",
            "result",
            "--verify",
            "done",
        ])
        .expect("Create task failed");
    let task_id = extract_task_id(&output);

    env.run(&["task", "start", &task_id, "--assignee", "test"])
        .expect("Start failed");
    env.run(&["task", "complete", &task_id, "--result", "done"])
        .expect("Complete failed");
    env.run(&["clean", "--all"]).expect("Clean failed");

    // Verify it's archived
    let output = env.run(&["list"]).expect("List failed");
    assert!(!output.contains(&goal_id));

    // Restore the goal
    let output = env.run(&["restore", &goal_id]).expect("Restore failed");
    assert!(output.contains("Restored"));
    assert!(output.contains(&goal_id));

    // Verify goal is back in active list
    let output = env.run(&["list"]).expect("List failed");
    assert!(output.contains(&goal_id));

    // Verify it's no longer in archive
    let archive_dir = env.radial_dir().join("archive").join(&goal_id);
    assert!(!archive_dir.exists());
}

#[test]
fn test_list_archived_goals() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create and archive multiple goals
    let output1 = env
        .run(&["goal", "create", "First goal"])
        .expect("Create failed");
    let goal_id1 = extract_goal_id(&output1);

    let output2 = env
        .run(&["goal", "create", "Second goal"])
        .expect("Create failed");
    let goal_id2 = extract_goal_id(&output2);

    // Complete and archive them
    let output = env
        .run(&[
            "task",
            "create",
            &goal_id1,
            "Task 1",
            "--receives",
            "none",
            "--produces",
            "result",
            "--verify",
            "done",
        ])
        .expect("Create task failed");
    let task_id1 = extract_task_id(&output);
    env.run(&["task", "start", &task_id1, "--assignee", "test"])
        .expect("Start failed");
    env.run(&["task", "complete", &task_id1, "--result", "done"])
        .expect("Complete failed");

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id2,
            "Task 2",
            "--receives",
            "none",
            "--produces",
            "result",
            "--verify",
            "done",
        ])
        .expect("Create task failed");
    let task_id2 = extract_task_id(&output);
    env.run(&["task", "start", &task_id2, "--assignee", "test"])
        .expect("Start failed");
    env.run(&["task", "complete", &task_id2, "--result", "done"])
        .expect("Complete failed");

    env.run(&["clean", "--all"]).expect("Clean failed");

    // List archived goals
    let output = env
        .run(&["list", "--archived"])
        .expect("List archived failed");
    assert!(output.contains(&goal_id1));
    assert!(output.contains(&goal_id2));
    assert!(output.contains("First goal") || output.contains("First goa")); // May be truncated
    assert!(output.contains("Second goal") || output.contains("Second go"));

    // Verify active list is empty
    let output = env.run(&["list"]).expect("List failed");
    assert!(!output.contains(&goal_id1));
    assert!(!output.contains(&goal_id2));
}

#[test]
fn test_purge_permanently_deletes() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create and complete a goal
    let output = env
        .run(&["goal", "create", "Test goal"])
        .expect("Create failed");
    let goal_id = extract_goal_id(&output);

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            "Task 1",
            "--receives",
            "none",
            "--produces",
            "result",
            "--verify",
            "done",
        ])
        .expect("Create task failed");
    let task_id = extract_task_id(&output);

    env.run(&["task", "start", &task_id, "--assignee", "test"])
        .expect("Start failed");
    env.run(&["task", "complete", &task_id, "--result", "done"])
        .expect("Complete failed");

    // Purge the goal (permanently delete)
    let output = env
        .run(&["clean", "--all", "--purge"])
        .expect("Clean with purge failed");
    assert!(output.contains("Deleted"));
    assert!(output.contains(&goal_id));

    // Verify goal is not in active list
    let output = env.run(&["list"]).expect("List failed");
    assert!(!output.contains(&goal_id));

    // Verify goal is NOT in archive
    let archive_dir = env.radial_dir().join("archive").join(&goal_id);
    assert!(!archive_dir.exists());

    // Verify goal directory is completely gone
    let goal_dir = env.radial_dir().join(&goal_id);
    assert!(!goal_dir.exists());
}

#[test]
fn test_archive_directory_skipped_during_load() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create and archive a goal
    let output = env
        .run(&["goal", "create", "Test goal"])
        .expect("Create failed");
    let goal_id = extract_goal_id(&output);

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            "Task 1",
            "--receives",
            "none",
            "--produces",
            "result",
            "--verify",
            "done",
        ])
        .expect("Create task failed");
    let task_id = extract_task_id(&output);

    env.run(&["task", "start", &task_id, "--assignee", "test"])
        .expect("Start failed");
    env.run(&["task", "complete", &task_id, "--result", "done"])
        .expect("Complete failed");
    env.run(&["clean", "--all"]).expect("Clean failed");

    // Create a new active goal
    let output = env
        .run(&["goal", "create", "Active goal"])
        .expect("Create failed");
    let active_goal_id = extract_goal_id(&output);

    // List active goals - should only show the active one
    let output = env.run(&["list"]).expect("List failed");
    assert!(output.contains(&active_goal_id));
    assert!(!output.contains(&goal_id)); // Archived goal should not appear

    // Status should also exclude archived goals
    let output = env.run(&["status"]).expect("Status failed");
    assert!(output.contains(&active_goal_id));
    assert!(!output.contains(&goal_id));
}

#[test]
fn test_restore_fails_if_goal_exists() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create a goal with a specific ID by creating and archiving it
    let output = env
        .run(&["goal", "create", "Test goal"])
        .expect("Create failed");
    let goal_id = extract_goal_id(&output);

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            "Task 1",
            "--receives",
            "none",
            "--produces",
            "result",
            "--verify",
            "done",
        ])
        .expect("Create task failed");
    let task_id = extract_task_id(&output);

    env.run(&["task", "start", &task_id, "--assignee", "test"])
        .expect("Start failed");
    env.run(&["task", "complete", &task_id, "--result", "done"])
        .expect("Complete failed");
    env.run(&["clean", "--all"]).expect("Clean failed");

    // Create a new goal (which will have a different ID)
    let output2 = env
        .run(&["goal", "create", "New goal"])
        .expect("Create failed");
    let new_goal_id = extract_goal_id(&output2);

    // Manually move it to have the same ID as the archived one (simulate conflict)
    let new_goal_dir = env.radial_dir().join(&new_goal_id);
    let conflict_dir = env.radial_dir().join(&goal_id);
    fs::rename(&new_goal_dir, &conflict_dir).expect("Failed to create conflict");

    // Try to restore - should fail
    let result = env.run(&["restore", &goal_id]);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("already exists") || error.contains("Cannot restore"));
}

fn extract_goal_id(output: &str) -> String {
    output
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(2))
        .expect("Could not extract goal ID")
        .to_string()
}

fn extract_task_id(output: &str) -> String {
    output
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(2))
        .expect("Could not extract task ID")
        .to_string()
}
