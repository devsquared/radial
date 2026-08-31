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

    fn run_expect_fail(&self, args: &[&str]) -> String {
        self.run(args).expect_err("Expected command to fail")
    }
}

#[test]
fn test_task_cancel_basic() {
    let env = TestEnv::new();
    env.run(&["init"]).unwrap();

    let output = env.run(&["goal", "create", "Test goal"]).unwrap();
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task to cancel",
            "--receives",
            "input",
            "--produces",
            "output",
            "--verify",
            "check",
        ])
        .unwrap();

    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&["task", "cancel", task_id, "--reason", "no longer needed"])
        .unwrap();
    assert!(output.contains("Cancelled task:"));

    let output = env.run(&["show", task_id]).unwrap();
    assert!(output.contains("[cancelled]"));
    assert!(output.contains("Cancelled by cli: no longer needed"));
}

#[test]
fn test_task_cancel_rejects_completed() {
    let env = TestEnv::new();
    env.run(&["init"]).unwrap();

    let output = env.run(&["goal", "create", "Test goal"]).unwrap();
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task to complete",
            "--receives",
            "input",
            "--produces",
            "output",
            "--verify",
            "check",
        ])
        .unwrap();

    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    env.run(&["task", "start", task_id, "--assignee", "test"])
        .unwrap();
    env.run(&["task", "complete", task_id, "--result", "done"])
        .unwrap();

    let error = env.run_expect_fail(&["task", "cancel", task_id]);
    assert!(error.contains("Cannot cancel a completed task"));
}

#[test]
fn test_cancel_unblocks_dependent() {
    let env = TestEnv::new();
    env.run(&["init"]).unwrap();

    let output = env.run(&["goal", "create", "Test goal"]).unwrap();
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Blocker task",
            "--receives",
            "input",
            "--produces",
            "output",
            "--verify",
            "check",
        ])
        .unwrap();

    let blocker_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Dependent task",
            "--receives",
            "output",
            "--produces",
            "result",
            "--verify",
            "check",
            "--blocked-by",
            blocker_id,
        ])
        .unwrap();

    let dependent_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env.run(&["show", dependent_id]).unwrap();
    assert!(output.contains("[blocked]"));

    let output = env
        .run(&["task", "cancel", blocker_id, "--reason", "not needed"])
        .unwrap();
    assert!(output.contains("Unblocked 1 task(s)"));

    let output = env.run(&["show", dependent_id]).unwrap();
    assert!(output.contains("[pending]"));
    assert!(output.contains("was cancelled"));
}

#[test]
fn test_cancel_cascade() {
    let env = TestEnv::new();
    env.run(&["init"]).unwrap();

    let output = env.run(&["goal", "create", "Test goal"]).unwrap();
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task 1",
            "--receives",
            "input",
            "--produces",
            "out1",
            "--verify",
            "check",
        ])
        .unwrap();

    let task1_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task 2",
            "--receives",
            "out1",
            "--produces",
            "out2",
            "--verify",
            "check",
            "--blocked-by",
            task1_id,
        ])
        .unwrap();

    let task2_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task 3",
            "--receives",
            "out2",
            "--produces",
            "out3",
            "--verify",
            "check",
            "--blocked-by",
            task2_id,
        ])
        .unwrap();

    let task3_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&["task", "cancel", task1_id, "--cascade", "--reason", "abort"])
        .unwrap();
    assert!(output.contains("Cancelled task:"));
    assert!(output.contains("Cascade-cancelled 2 task(s)"));

    for task_id in [task1_id, task2_id, task3_id] {
        let output = env.run(&["show", task_id]).unwrap();
        assert!(output.contains("[cancelled]"));
    }

    let output = env.run(&["show", task2_id]).unwrap();
    assert!(output.contains("cascaded from cancellation"));
}

#[test]
fn test_cancel_with_parent_task() {
    let env = TestEnv::new();
    env.run(&["init"]).unwrap();

    let output = env.run(&["goal", "create", "Test goal"]).unwrap();
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Parent task",
            "--receives",
            "input",
            "--produces",
            "output",
            "--verify",
            "check",
        ])
        .unwrap();

    let parent_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Subtask 1",
            "--parent",
            parent_id,
            "--receives",
            "input",
            "--produces",
            "part1",
            "--verify",
            "check",
        ])
        .unwrap();

    let subtask1_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Subtask 2",
            "--parent",
            parent_id,
            "--receives",
            "input",
            "--produces",
            "part2",
            "--verify",
            "check",
        ])
        .unwrap();

    let subtask2_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    env.run(&["task", "cancel", subtask1_id]).unwrap();
    env.run(&["task", "cancel", subtask2_id]).unwrap();

    let output = env.run(&["show", parent_id]).unwrap();
    assert!(output.contains("[cancelled]"));
}
