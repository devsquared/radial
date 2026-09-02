use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Helper struct to manage test environment
struct TestEnv {
    _temp_dir: TempDir,
    work_dir: PathBuf,
    binary_path: PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let work_dir = temp_dir.path().to_path_buf();

        // Get the path to the compiled binary
        let mut binary_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        binary_path.push("target");
        binary_path.push("debug");
        binary_path.push("rd");

        Self {
            _temp_dir: temp_dir,
            work_dir,
            binary_path,
        }
    }

    /// Run a radial command and return the output
    fn run(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(&self.binary_path)
            .args(args)
            .current_dir(&self.work_dir)
            .output()
            .expect("Failed to execute radial command");

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    /// Check if .radial directory exists
    fn radial_dir_exists(&self) -> bool {
        self.work_dir.join(".radial").exists()
    }

    /// Check if the database directory exists (sentinel for a valid initialized state)
    fn db_exists(&self) -> bool {
        self.work_dir.join(".radial").is_dir()
    }
}

#[test]
fn test_init_creates_radial_directory() {
    let env = TestEnv::new();

    assert!(
        !env.radial_dir_exists(),
        "Radial directory should not exist initially"
    );

    let output = env.run(&["init"]).expect("Init command failed");
    assert!(output.contains("Initialized radial"));

    assert!(
        env.radial_dir_exists(),
        "Radial directory should exist after init"
    );
    assert!(env.db_exists(), "Database file should exist after init");
}

#[test]
fn test_init_is_idempotent() {
    let env = TestEnv::new();

    env.run(&["init"]).expect("First init failed");
    let output = env.run(&["init"]).expect("Second init failed");

    assert!(output.contains("already initialized"));
}

#[test]
fn test_commands_fail_without_init() {
    let env = TestEnv::new();

    let result = env.run(&["goal", "list"]);
    assert!(result.is_err(), "Commands should fail without init");
    assert!(result.unwrap_err().contains("not initialized"));
}

#[test]
fn test_create_and_list_goals() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Initially no goals
    let output = env.run(&["goal", "list"]).expect("List failed");
    assert!(output.contains("No goals found"));

    // Create a goal
    let output = env
        .run(&["goal", "create", "Test goal description"])
        .expect("Create goal failed");
    assert!(output.contains("Created goal:"));

    // Extract goal ID from output (format: "Created goal: XXXXXXXX")
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .expect("Could not extract goal ID");

    assert_eq!(goal_id.len(), 8, "Goal ID should be 8 characters");

    // List goals should show the created goal
    let output = env.run(&["goal", "list"]).expect("List failed");
    assert!(output.contains(goal_id));
    assert!(output.contains("Test goal description"));
    assert!(output.contains("pending"));
}

#[test]
fn test_create_task_and_workflow() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create a goal
    let output = env
        .run(&["goal", "create", "Test workflow"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .expect("Could not extract goal ID");

    // Create a task
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Test task",
            "--receives",
            "Nothing",
            "--produces",
            "Something",
            "--verify",
            "It exists",
        ])
        .expect("Create task failed");

    assert!(output.contains("Created task:"));
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .expect("Could not extract task ID");

    assert_eq!(task_id.len(), 8, "Task ID should be 8 characters");

    // List tasks
    let output = env
        .run(&["task", "list", goal_id])
        .expect("List tasks failed");
    assert!(output.contains(task_id));
    assert!(output.contains("Test task"));
    assert!(output.contains("pending"));

    // Full detail via show
    let output = env.run(&["show", task_id]).expect("Show task failed");
    assert!(output.contains("Receives"));
    assert!(output.contains("Nothing"));
    assert!(output.contains("Produces"));
    assert!(output.contains("Something"));
    assert!(output.contains("Verify"));
    assert!(output.contains("It exists"));

    // Goal should now be in_progress
    let output = env.run(&["goal", "list"]).expect("List goals failed");
    assert!(output.contains("pending") || output.contains("in_progress"));
}

#[test]
fn test_task_state_transitions() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create goal and task
    let output = env
        .run(&["goal", "create", "State test"])
        .expect("Create goal failed");
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
            "State task",
            "--receives",
            "Input",
            "--produces",
            "Output",
            "--verify",
            "Check",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Task should be pending
    let output = env
        .run(&["status", "--task", task_id])
        .expect("Status failed");
    assert!(output.contains("pending"));

    // Start the task
    env.run(&["task", "start", task_id, "--assignee", "test-agent"])
        .expect("Start task failed");
    let output = env
        .run(&["status", "--task", task_id])
        .expect("Status failed");
    assert!(output.contains("in_progress"));

    // Complete the task
    env.run(&[
        "task",
        "complete",
        task_id,
        "--result",
        "Task completed successfully",
    ])
    .expect("Complete task failed");

    let output = env
        .run(&["status", "--task", task_id])
        .expect("Status failed");
    assert!(output.contains("completed"));

    // Full detail via show
    let output = env.run(&["show", task_id]).expect("Show task failed");
    assert!(output.contains("Task completed successfully"));
}

#[test]
fn test_task_with_artifacts() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Artifact test"])
        .expect("Create goal failed");
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
            "Create files",
            "--receives",
            "Requirements",
            "--produces",
            "Files",
            "--verify",
            "Files exist",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    env.run(&["task", "start", task_id, "--assignee", "test-agent"])
        .expect("Start failed");
    env.run(&[
        "task",
        "complete",
        task_id,
        "--result",
        "Created multiple files",
        "--artifacts",
        "file1.txt,file2.txt,src/main.rs",
    ])
    .expect("Complete failed");

    let output = env.run(&["show", task_id]).expect("Show task failed");
    assert!(output.contains("file1.txt"));
    assert!(output.contains("file2.txt"));
    assert!(output.contains("src/main.rs"));
}

#[test]
fn test_blocked_tasks() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Dependency test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Create first task
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "First task",
            "--receives",
            "Nothing",
            "--produces",
            "Foundation",
            "--verify",
            "Foundation exists",
        ])
        .expect("Create task failed");
    let task_id_1 = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Create second task blocked by first
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Second task",
            "--receives",
            "Foundation",
            "--produces",
            "Building",
            "--verify",
            "Building complete",
            "--blocked-by",
            task_id_1,
        ])
        .expect("Create task failed");
    let task_id_2 = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Second task should be blocked
    let output = env
        .run(&["status", "--task", task_id_2])
        .expect("Status failed");
    assert!(output.contains("blocked"));

    // Full detail shows blocker
    let output = env.run(&["show", task_id_2]).expect("Show task failed");
    assert!(output.contains(task_id_1));
}

#[test]
fn test_goal_completion() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Completion test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Create and complete a task
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Only task",
            "--receives",
            "Goal",
            "--produces",
            "Result",
            "--verify",
            "Done",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    env.run(&["task", "start", task_id, "--assignee", "test-agent"])
        .expect("Start failed");
    env.run(&["task", "complete", task_id, "--result", "All done"])
        .expect("Complete failed");

    // Goal should now be completed
    let output = env
        .run(&["status", "--goal", goal_id])
        .expect("Status failed");
    assert!(output.contains("completed"));

    // Full detail via show
    let output = env.run(&["show", goal_id]).expect("Show goal failed");
    assert!(output.contains("1 total, 1 completed"));
}

#[test]
fn test_status_commands() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create a goal
    let output = env
        .run(&["goal", "create", "Status test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Status with no filter shows all goals
    let output = env.run(&["status"]).expect("Status failed");
    assert!(output.contains("ID"));
    assert!(output.contains(goal_id));

    // Status with goal filter
    let output = env
        .run(&["status", "--goal", goal_id])
        .expect("Status failed");
    assert!(output.contains("Goal:"));
    assert!(output.contains(goal_id));
    assert!(output.contains("Status test"));
}

#[test]
fn test_multiple_goals_and_tasks() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create multiple goals
    let output1 = env
        .run(&["goal", "create", "First goal"])
        .expect("Create goal 1 failed");
    let goal_id_1 = output1
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output2 = env
        .run(&["goal", "create", "Second goal"])
        .expect("Create goal 2 failed");
    let goal_id_2 = output2
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Create tasks for each goal
    env.run(&[
        "task",
        "create",
        goal_id_1,
        "Task for goal 1",
        "--receives",
        "A",
        "--produces",
        "B",
        "--verify",
        "C",
    ])
    .expect("Create task 1 failed");

    env.run(&[
        "task",
        "create",
        goal_id_2,
        "Task for goal 2",
        "--receives",
        "X",
        "--produces",
        "Y",
        "--verify",
        "Z",
    ])
    .expect("Create task 2 failed");

    // List all goals
    let output = env.run(&["goal", "list"]).expect("List goals failed");
    assert!(output.contains(goal_id_1));
    assert!(output.contains(goal_id_2));
    assert!(output.contains("First goal"));
    assert!(output.contains("Second goal"));

    // List tasks for each goal separately
    let output1 = env
        .run(&["task", "list", goal_id_1])
        .expect("List tasks 1 failed");
    assert!(output1.contains("Task for goal 1"));
    assert!(!output1.contains("Task for goal 2"));

    let output2 = env
        .run(&["task", "list", goal_id_2])
        .expect("List tasks 2 failed");
    assert!(output2.contains("Task for goal 2"));
    assert!(!output2.contains("Task for goal 1"));
}

#[test]
fn test_directory_walkup() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create a goal in the root
    let output = env
        .run(&["goal", "create", "Walkup test goal"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .expect("Could not extract goal ID");

    // Create a nested subdirectory
    let subdir = env.work_dir.join("src").join("deep").join("nested");
    std::fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

    // Run radial from the subdirectory
    let output = Command::new(&env.binary_path)
        .args(["goal", "list"])
        .current_dir(&subdir)
        .output()
        .expect("Failed to execute radial");

    assert!(
        output.status.success(),
        "Command should succeed from subdirectory"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(goal_id),
        "Should find goal from subdirectory"
    );
    assert!(stdout.contains("Walkup test goal"));
}

#[test]
fn test_stealth_mode_with_git_repo() {
    let env = TestEnv::new();

    // Initialize a git repo
    Command::new("git")
        .args(["init"])
        .current_dir(&env.work_dir)
        .output()
        .expect("Failed to init git");

    // Initialize radial with stealth mode
    let output = env
        .run(&["init", "--stealth"])
        .expect("Init --stealth failed");
    assert!(output.contains("Initialized radial"));
    assert!(output.contains("Added .radial to"));

    // Check that .radial is in .git/info/exclude
    let exclude_path = env.work_dir.join(".git").join("info").join("exclude");
    let exclude_content = std::fs::read_to_string(&exclude_path).expect("Failed to read exclude");
    assert!(
        exclude_content.contains(".radial"),
        "Exclude file should contain .radial"
    );
}

#[test]
fn test_redirect_file() {
    // Create two separate temp directories
    let project_a = TempDir::new().expect("Failed to create project_a");
    let project_b = TempDir::new().expect("Failed to create project_b");

    let binary_path = {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("target");
        path.push("debug");
        path.push("rd");
        path
    };

    // Initialize radial in project_a
    let output = Command::new(&binary_path)
        .args(["init"])
        .current_dir(project_a.path())
        .output()
        .expect("Failed to init project_a");
    assert!(output.status.success());

    // Create a .radial directory in project_b with a redirect file
    let project_b_radial = project_b.path().join(".radial");
    std::fs::create_dir_all(&project_b_radial).expect("Failed to create .radial in project_b");

    let redirect_target = project_a.path().join(".radial");
    std::fs::write(
        project_b_radial.join("redirect"),
        redirect_target.to_string_lossy().as_ref(),
    )
    .expect("Failed to write redirect file");

    // Create a goal from project_b (should go to project_a's database)
    let output = Command::new(&binary_path)
        .args(["goal", "create", "Goal from project B"])
        .current_dir(project_b.path())
        .output()
        .expect("Failed to create goal from project_b");
    assert!(output.status.success());

    // List goals from project_a (should see the goal created from project_b)
    let output = Command::new(&binary_path)
        .args(["goal", "list"])
        .current_dir(project_a.path())
        .output()
        .expect("Failed to list goals from project_a");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Goal from project B"),
        "Project A should see goal created via redirect from project B"
    );
}

#[test]
fn test_json_output_goal_list() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Empty list should return valid JSON array
    let output = env
        .run(&["goal", "list", "--json"])
        .expect("List --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed.is_array(), "Output should be a JSON array");
    assert_eq!(parsed.as_array().unwrap().len(), 0, "Array should be empty");

    // Create a goal
    env.run(&["goal", "create", "JSON test goal"])
        .expect("Create goal failed");

    // List should return array with one goal
    let output = env
        .run(&["goal", "list", "--json"])
        .expect("List --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed.is_array());

    let goals = parsed.as_array().unwrap();
    assert_eq!(goals.len(), 1);
    assert_eq!(goals[0]["description"], "JSON test goal");
    assert!(goals[0]["id"].is_string());
    assert!(goals[0]["state"].is_string());
    assert!(goals[0]["created_at"].is_string());
}

#[test]
fn test_json_output_task_list() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Task list JSON test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Empty task list
    let output = env
        .run(&["task", "list", goal_id, "--json"])
        .expect("List tasks --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 0);

    // Create a task
    env.run(&[
        "task",
        "create",
        goal_id,
        "JSON task",
        "--receives",
        "Input data",
        "--produces",
        "Output data",
        "--verify",
        "Data is processed",
    ])
    .expect("Create task failed");

    // List tasks should return array with one task
    let output = env
        .run(&["task", "list", goal_id, "--json"])
        .expect("List tasks --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed.is_array());

    let tasks = parsed.as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["description"], "JSON task");
    assert!(tasks[0]["id"].is_string());
    assert!(tasks[0]["goal_id"].is_string());
    assert!(tasks[0]["contract"]["receives"].is_string());
    assert_eq!(tasks[0]["contract"]["receives"], "Input data");
}

#[test]
fn test_json_output_task_start_and_complete() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Start/complete JSON test"])
        .expect("Create goal failed");
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
            "JSON start/complete task",
            "--receives",
            "Input data",
            "--produces",
            "Output data",
            "--verify",
            "Data is processed",
            "--json",
        ])
        .expect("Create task failed");
    let created: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    let task_id = created["id"].as_str().unwrap();

    let output = env
        .run(&["task", "start", task_id, "--assignee", "tester", "--json"])
        .expect("Start task --json failed");
    let started: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(started["state"], "inprogress");
    assert_eq!(started["assignee"], "tester");

    let output = env
        .run(&["task", "complete", task_id, "--result", "Done", "--json"])
        .expect("Complete task --json failed");
    let completed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(completed["task"]["state"], "completed");
    assert!(completed["unblocked_task_ids"].is_array());
}

#[test]
fn test_json_output_init() {
    let env = TestEnv::new();

    let output = env.run(&["init", "--json"]).expect("Init --json failed");
    let init: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(init["already_initialized"], false);

    let output = env.run(&["init", "--json"]).expect("Init --json failed");
    let init_again: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(init_again["already_initialized"], true);
}

#[test]
fn test_json_output_edit_and_task_lifecycle_mutations() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "JSON lifecycle coverage"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let output = env
        .run(&[
            "edit",
            "goal",
            &goal_id,
            "--description",
            "Renamed",
            "--json",
        ])
        .expect("Edit goal --json failed");
    let edited: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(edited["description"], "Renamed");

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            "Fail me",
            "--receives",
            "r",
            "--produces",
            "p",
            "--verify",
            "v",
            "--json",
        ])
        .expect("Create task failed");
    let task: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    let task_id = task["id"].as_str().unwrap().to_string();

    let output = env
        .run(&[
            "edit",
            "task",
            &task_id,
            "--description",
            "Fail me too",
            "--json",
        ])
        .expect("Edit task --json failed");
    let edited_task: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(edited_task["description"], "Fail me too");

    env.run(&["task", "start", &task_id, "--assignee", "tester"])
        .expect("Start task failed");

    let output = env
        .run(&["task", "fail", &task_id, "--reason", "broke", "--json"])
        .expect("Fail task --json failed");
    let failed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(failed["state"], "failed");

    let output = env
        .run(&["task", "retry", &task_id, "--json"])
        .expect("Retry task --json failed");
    let retried: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(retried["state"], "inprogress");

    let output = env
        .run(&["task", "release", &task_id, "--json"])
        .expect("Release task --json failed");
    let released: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(released["state"], "pending");

    let output = env
        .run(&["task", "release", "--all-in-progress", "--json"])
        .expect("Release --all-in-progress --json failed");
    let released_all: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(released_all.is_array());

    let output = env
        .run(&[
            "task",
            "cancel",
            &task_id,
            "--reason",
            "no longer needed",
            "--json",
        ])
        .expect("Cancel task --json failed");
    let cancelled: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(cancelled["task"]["state"], "cancelled");
    assert!(cancelled["unblocked_task_ids"].is_array());
    assert!(cancelled["cascaded_task_ids"].is_array());

    let output = env
        .run(&["task", "create", &goal_id, "Delete me", "--json"])
        .expect("Create task failed");
    let deletable: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    let deletable_id = deletable["id"].as_str().unwrap();
    let output = env
        .run(&["task", "delete", deletable_id, "--json"])
        .expect("Delete task --json failed");
    let deleted: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(deleted["id"], deletable_id);
}

#[test]
fn test_json_output_goal_lifecycle_and_prep_and_compact() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "JSON goal lifecycle coverage"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let output = env
        .run(&[
            "goal",
            "cancel",
            &goal_id,
            "--reason",
            "wrapping up",
            "--json",
        ])
        .expect("Cancel goal --json failed");
    let goal_cancelled: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(goal_cancelled["goal"]["state"], "cancelled");
    assert!(goal_cancelled["cancelled_task_ids"].is_array());

    let output = env.run(&["clean", "--json"]).expect("Clean --json failed");
    let cleaned: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(cleaned["candidates"], 1);
    assert_eq!(cleaned["removed"].as_array().unwrap().len(), 1);

    let output = env
        .run(&["restore", &goal_id, "--json"])
        .expect("Restore --json failed");
    let restored: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(restored["id"], goal_id);

    let output = env.run(&["prep", "--json"]).expect("Prep --json failed");
    let prep: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(prep["guide"].as_str().unwrap().contains("rd"));

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            "Compact me",
            "--receives",
            "r",
            "--produces",
            "p",
            "--verify",
            "v",
            "--json",
        ])
        .expect("Create task failed");
    let compactable: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    let compactable_id = compactable["id"].as_str().unwrap().to_string();
    env.run(&["task", "start", &compactable_id, "--assignee", "tester"])
        .expect("Start task failed");
    env.run(&[
        "task",
        "fail",
        &compactable_id,
        "--reason",
        "for compaction test",
    ])
    .expect("Fail task failed");
    let output = env
        .run(&[
            "compact",
            "apply",
            &compactable_id,
            "--summary",
            "short",
            "--json",
        ])
        .expect("Compact apply --json failed");
    let compacted: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(compacted["task_id"], compactable_id);
}

#[test]
fn test_json_output_status() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create a goal with a task
    let output = env
        .run(&["goal", "create", "Status JSON test"])
        .expect("Create goal failed");
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
            "Status task",
            "--receives",
            "In",
            "--produces",
            "Out",
            "--verify",
            "Check",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Test status --json (all goals)
    let output = env
        .run(&["status", "--json"])
        .expect("Status --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed.is_array(), "All goals status should be an array");
    let goals = parsed.as_array().unwrap();
    assert_eq!(goals.len(), 1);
    assert!(goals[0]["computed_metrics"].is_object());

    // Test status --goal <id> --json
    let output = env
        .run(&["status", "--goal", goal_id, "--json"])
        .expect("Status --goal --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed.is_object(), "Goal status should be an object");
    assert_eq!(parsed["id"], goal_id);
    assert!(parsed["tasks"].is_array());
    assert_eq!(parsed["tasks"].as_array().unwrap().len(), 1);

    // Test status --task <id> --json
    let output = env
        .run(&["status", "--task", task_id, "--json"])
        .expect("Status --task --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed.is_object(), "Task status should be an object");
    assert_eq!(parsed["id"], task_id);
    assert_eq!(parsed["description"], "Status task");
    assert!(parsed["contract"].is_object());
}

#[test]
fn test_task_comments() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create goal and task
    let output = env
        .run(&["goal", "create", "Comment test"])
        .expect("Create goal failed");
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
            "Task with comments",
            "--receives",
            "Input",
            "--produces",
            "Output",
            "--verify",
            "Check",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Add first comment
    let output = env
        .run(&["task", "comment", task_id, "First comment on task"])
        .expect("Add comment failed");
    assert!(output.contains("Added comment to task"));
    assert!(output.contains(task_id));

    // Add second comment
    let output = env
        .run(&[
            "task",
            "comment",
            task_id,
            "Second comment with more detail",
        ])
        .expect("Add second comment failed");
    assert!(output.contains("Added comment to task"));

    // Check that show displays comments
    let output = env.run(&["show", task_id]).expect("Show failed");
    assert!(output.contains("Comments"));
    assert!(output.contains("First comment on task"));
    assert!(output.contains("Second comment with more detail"));
}

#[test]
fn test_status_compact_vs_show_detail() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create goal and task
    let output = env
        .run(&["goal", "create", "Compact test"])
        .expect("Create goal failed");
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
            "Task for compact test",
            "--receives",
            "Input",
            "--produces",
            "Output",
            "--verify",
            "Check",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Add a comment
    env.run(&[
        "task",
        "comment",
        task_id,
        "This comment should only appear in show",
    ])
    .expect("Add comment failed");

    // Status should be compact — no comments, no contract detail
    let output = env
        .run(&["status", "--task", task_id])
        .expect("Status failed");
    assert!(output.contains(task_id));
    assert!(output.contains("Task for compact test"));
    assert!(!output.contains("Comments"));
    assert!(!output.contains("This comment should only appear in show"));

    // Show should have full detail including comments
    let output = env.run(&["show", task_id]).expect("Show failed");
    assert!(output.contains(task_id));
    assert!(output.contains("Task for compact test"));
    assert!(output.contains("Comments"));
    assert!(output.contains("This comment should only appear in show"));
    assert!(output.contains("Receives"));
    assert!(output.contains("Input"));
}

#[test]
fn test_task_comments_json_output() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create goal and task
    let output = env
        .run(&["goal", "create", "JSON comment test"])
        .expect("Create goal failed");
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
            "Task for JSON comment test",
            "--receives",
            "Input",
            "--produces",
            "Output",
            "--verify",
            "Check",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Task should start with empty comments array
    let output = env
        .run(&["status", "--task", task_id, "--json"])
        .expect("Status --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed["comments"].is_array());
    assert_eq!(parsed["comments"].as_array().unwrap().len(), 0);

    // Add comments
    env.run(&["task", "comment", task_id, "JSON comment one"])
        .expect("Add comment failed");
    env.run(&["task", "comment", task_id, "JSON comment two"])
        .expect("Add second comment failed");

    // Check JSON output includes comments
    let output = env
        .run(&["status", "--task", task_id, "--json"])
        .expect("Status --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed["comments"].is_array());

    let comments = parsed["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0]["text"], "JSON comment one");
    assert_eq!(comments[1]["text"], "JSON comment two");
    assert!(comments[0]["id"].is_string());
    assert!(comments[0]["created_at"].is_string());

    // Show --json should also include comments
    let output = env
        .run(&["show", task_id, "--json"])
        .expect("Show --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    let comments = parsed["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2, "Show JSON should include comments");
}

#[test]
fn test_task_assignee_and_release() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Assignee test"])
        .expect("Create goal failed");
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
            "Assignee task",
            "--receives",
            "Input",
            "--produces",
            "Output",
            "--verify",
            "Check",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Start requires --assignee
    let result = env.run(&["task", "start", task_id]);
    assert!(result.is_err(), "Start without --assignee should fail");

    // Start with assignee
    let output = env
        .run(&["task", "start", task_id, "--assignee", "agent-1"])
        .expect("Start with assignee failed");
    assert!(output.contains("Started task:"));
    assert!(output.contains("Assigned to: agent-1"));

    // Show should display assignee
    let output = env.run(&["show", task_id]).expect("Show failed");
    assert!(output.contains("agent-1"));

    // Task list should show assignee
    let output = env.run(&["task", "list", goal_id]).expect("List failed");
    assert!(output.contains("agent-1"));

    // JSON output should include assignee
    let output = env
        .run(&["show", task_id, "--json"])
        .expect("Show --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(parsed["assignee"], "agent-1");

    // Release the task
    let output = env
        .run(&["task", "release", task_id])
        .expect("Release failed");
    assert!(output.contains("Released task:"));
    assert!(output.contains("pending"));

    // After release, assignee should be cleared
    let output = env
        .run(&["show", task_id, "--json"])
        .expect("Show --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(
        parsed["assignee"].is_null(),
        "Assignee should be null after release"
    );
    assert_eq!(parsed["state"], "pending");
}

#[test]
fn test_task_list_filter_by_assignee() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Filter test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Create two tasks
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task A",
            "--receives",
            "In",
            "--produces",
            "Out",
            "--verify",
            "Check",
        ])
        .expect("Create task A failed");
    let task_a = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task B",
            "--receives",
            "In",
            "--produces",
            "Out",
            "--verify",
            "Check",
        ])
        .expect("Create task B failed");
    let task_b = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    // Start tasks with different assignees
    env.run(&["task", "start", &task_a, "--assignee", "agent-1"])
        .expect("Start A failed");
    env.run(&["task", "start", &task_b, "--assignee", "agent-2"])
        .expect("Start B failed");

    // Filter by agent-1
    let output = env
        .run(&["task", "list", goal_id, "--assignee", "agent-1"])
        .expect("List filtered failed");
    assert!(output.contains("Task A"));
    assert!(!output.contains("Task B"));

    // Filter by agent-2
    let output = env
        .run(&["task", "list", goal_id, "--assignee", "agent-2"])
        .expect("List filtered failed");
    assert!(!output.contains("Task A"));
    assert!(output.contains("Task B"));

    // No filter shows both
    let output = env
        .run(&["task", "list", goal_id])
        .expect("List all failed");
    assert!(output.contains("Task A"));
    assert!(output.contains("Task B"));
}

#[test]
fn test_task_release_from_failed() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Release from failed test"])
        .expect("Create goal failed");
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
            "Failing task",
            "--receives",
            "In",
            "--produces",
            "Out",
            "--verify",
            "Check",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Start and fail the task
    env.run(&["task", "start", task_id, "--assignee", "agent-1"])
        .expect("Start failed");
    env.run(&["task", "fail", task_id]).expect("Fail failed");

    // Release from failed state
    let output = env
        .run(&["task", "release", task_id])
        .expect("Release from failed should work");
    assert!(output.contains("Released task:"));
    assert!(output.contains("pending"));

    // Verify state is back to pending with no assignee
    let output = env
        .run(&["show", task_id, "--json"])
        .expect("Show --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(parsed["state"], "pending");
    assert!(parsed["assignee"].is_null());
}

#[test]
fn test_release_unassigned_task_fails() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Release unassigned test"])
        .expect("Create goal failed");
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
            "Unassigned task",
            "--receives",
            "In",
            "--produces",
            "Out",
            "--verify",
            "Check",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Releasing an unassigned task should fail
    let result = env.run(&["task", "release", task_id]);
    assert!(result.is_err(), "Release on unassigned task should fail");
    assert!(result.unwrap_err().contains("no assignee"));
}

// -- Compaction tests --

/// Helper: create a goal, create a task with contract, start it, and complete it.
/// Returns (`goal_id`, `task_id`).
fn create_completed_task(env: &TestEnv, label: &str) -> (String, String) {
    let output = env
        .run(&["goal", "create", &format!("{label} goal")])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            &format!("{label} task"),
            "--receives",
            "Input data",
            "--produces",
            "Output data",
            "--verify",
            "Check output",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    env.run(&["task", "start", &task_id, "--assignee", "agent-1"])
        .expect("Start failed");
    env.run(&["task", "complete", &task_id, "--result", "Done"])
        .expect("Complete failed");

    (goal_id, task_id)
}

fn create_failed_task(env: &TestEnv, label: &str) -> (String, String) {
    let output = env
        .run(&["goal", "create", &format!("{label} goal")])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            &format!("{label} task"),
            "--receives",
            "Input data",
            "--produces",
            "Output data",
            "--verify",
            "Check output",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    env.run(&["task", "start", &task_id, "--assignee", "agent-1"])
        .expect("Start failed");
    env.run(&[
        "task",
        "fail",
        &task_id,
        "--reason",
        "Task failed during test",
    ])
    .expect("Fail failed");

    (goal_id, task_id)
}

#[test]
fn test_compact_analyze_empty() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // No tasks at all
    let output = env
        .run(&["compact", "analyze"])
        .expect("Compact analyze failed");
    assert!(output.contains("No tasks eligible"));
}

#[test]
fn test_compact_analyze_finds_completed_tasks() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Completed tasks are auto-compacted; only failed tasks remain as candidates.
    let (_goal_id, task_id) = create_failed_task(&env, "Compact");

    let output = env
        .run(&["compact", "analyze"])
        .expect("Compact analyze failed");
    assert!(output.contains(&task_id));
    assert!(output.contains("1 task(s) eligible"));
}

#[test]
fn test_compact_analyze_json() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let (goal_id, task_id) = create_failed_task(&env, "JSON compact");

    let output = env
        .run(&["compact", "analyze", "--json"])
        .expect("Compact analyze --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed.is_array());
    let candidates = parsed.as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["id"], task_id);
    assert_eq!(candidates[0]["goal_id"], goal_id);
    assert_eq!(candidates[0]["state"], "failed");
}

#[test]
fn test_compact_analyze_filter_by_goal() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let (goal_id_1, task_id_1) = create_failed_task(&env, "Goal1");
    let (_goal_id_2, task_id_2) = create_failed_task(&env, "Goal2");

    // All goals
    let output = env
        .run(&["compact", "analyze", "--json"])
        .expect("Analyze all failed");
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 2);

    // Filter to goal 1
    let output = env
        .run(&["compact", "analyze", "--goal", &goal_id_1, "--json"])
        .expect("Analyze filtered failed");
    let parsed: Value = serde_json::from_str(&output).unwrap();
    let candidates = parsed.as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["id"], task_id_1);

    // Ensure task_id_2 is not in the filtered result (just for clarity)
    assert_ne!(candidates[0]["id"], task_id_2);
}

#[test]
fn test_compact_apply_success() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let (_goal_id, task_id) = create_failed_task(&env, "Apply");

    let output = env
        .run(&[
            "compact",
            "apply",
            &task_id,
            "--summary",
            "Implemented the apply feature successfully.",
        ])
        .expect("Compact apply failed");
    assert!(output.contains("Compacted task:"));
    assert!(output.contains(&task_id));

    // After compaction, show should display summary instead of full detail
    let output = env.run(&["show", &task_id]).expect("Show failed");
    assert!(output.contains("[compacted]"));
    assert!(output.contains("Implemented the apply feature successfully."));
    assert!(!output.contains("Input data")); // contract should be gone
    assert!(!output.contains("Check output")); // verify should be gone

    // JSON should have compacted=true and summary
    let output = env
        .run(&["show", &task_id, "--json"])
        .expect("Show --json failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(parsed["compacted"], true);
    assert_eq!(
        parsed["summary"],
        "Implemented the apply feature successfully."
    );
    assert!(parsed["contract"].is_null());
    assert_eq!(parsed["description"], "[compacted]");
}

#[test]
fn test_compact_apply_already_compacted() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Completed tasks are auto-compacted; a manual apply should fail immediately.
    let (_goal_id, task_id) = create_completed_task(&env, "Double");

    let result = env.run(&["compact", "apply", &task_id, "--summary", "Any summary"]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("already compacted"));
}

#[test]
fn test_compact_apply_rejects_pending_task() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Reject test goal"])
        .expect("Create goal failed");
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
            "Pending task",
            "--receives",
            "In",
            "--produces",
            "Out",
            "--verify",
            "Check",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let result = env.run(&["compact", "apply", task_id, "--summary", "Should not work"]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("only completed or failed"));
}

#[test]
fn test_compact_not_in_analyze_after_compaction() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Failed tasks are not auto-compacted and should appear as candidates.
    let (_goal_id, task_id) = create_failed_task(&env, "Disappear");

    let output = env
        .run(&["compact", "analyze", "--json"])
        .expect("Analyze failed");
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 1);

    // Compact it manually
    env.run(&["compact", "apply", &task_id, "--summary", "Done and dusted"])
        .expect("Compact failed");

    // Should no longer appear in analyze
    let output = env
        .run(&["compact", "analyze", "--json"])
        .expect("Analyze failed");
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 0);
}

#[test]
fn test_prep_is_static() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Prep is now a static guide with no dynamic advisories.
    let output_before = env.run(&["prep"]).expect("Prep failed");
    // These strings only appear in the old dynamic count messages, not the static docs.
    assert!(!output_before.contains("task(s) eligible for compaction"));
    assert!(!output_before.contains("task(s) that have been in progress"));

    // Creating tasks does not change prep output.
    create_failed_task(&env, "Prep");

    let output_after = env.run(&["prep"]).expect("Prep failed");
    assert_eq!(output_before, output_after);
}

#[test]
fn test_edit_task_blocked_by_validates_ids() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Validation test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&["task", "create", goal_id, "A task"])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Editing blocked-by with a nonexistent task ID should fail
    let result = env.run(&["edit", "task", task_id, "--blocked-by", "AAAAAAAA"]);
    assert!(
        result.is_err(),
        "Edit with nonexistent blocked-by should fail"
    );
    let err = result.unwrap_err();
    assert!(err.contains("not found") || err.contains("Task not found in blocked-by list"));

    // Editing blocked-by with self-reference should fail
    let result = env.run(&["edit", "task", task_id, "--blocked-by", task_id]);
    assert!(result.is_err(), "Edit with self-reference should fail");
    let err = result.unwrap_err();
    assert!(err.contains("cannot block itself"));
}

#[test]
fn test_edit_task_blocked_by_rejects_cycles() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Cycle test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Create task A
    let output = env
        .run(&["task", "create", goal_id, "Task A"])
        .expect("Create task A failed");
    let task_a = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    // Create task B blocked by A
    let output = env
        .run(&["task", "create", goal_id, "Task B", "--blocked-by", &task_a])
        .expect("Create task B failed");
    let task_b = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    // Try to edit A to be blocked by B — should fail with cycle error
    let result = env.run(&["edit", "task", &task_a, "--blocked-by", &task_b]);
    assert!(result.is_err(), "Edit creating a cycle should fail");
    let err = result.unwrap_err();
    assert!(err.contains("Circular dependency detected"));

    // Create task C blocked by B
    let output = env
        .run(&["task", "create", goal_id, "Task C", "--blocked-by", &task_b])
        .expect("Create task C failed");
    let task_c = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    // Try to edit A to be blocked by C — transitive cycle: A -> C -> B -> A
    let result = env.run(&["edit", "task", &task_a, "--blocked-by", &task_c]);
    assert!(
        result.is_err(),
        "Edit creating a transitive cycle should fail"
    );
    let err = result.unwrap_err();
    assert!(err.contains("Circular dependency detected"));
}

#[test]
fn test_goal_stays_pending_until_task_start() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Goal state test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Goal should be pending before any tasks
    let output = env
        .run(&["goal", "list", "--json"])
        .expect("Goal list --json failed");
    let goals: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(goals[0]["state"], "pending");

    // Create a task — goal must remain pending
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "First task",
            "--receives",
            "nothing",
            "--produces",
            "something",
            "--verify",
            "it works",
        ])
        .expect("Create task failed");
    let task_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    let output = env
        .run(&["goal", "list", "--json"])
        .expect("Goal list --json failed");
    let goals: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        goals[0]["state"], "pending",
        "Goal must stay pending after task create, before task start"
    );

    // Start the task — goal transitions to in_progress
    env.run(&["task", "start", task_id, "--assignee", "test-agent"])
        .expect("Start task failed");

    let output = env
        .run(&["goal", "list", "--json"])
        .expect("Goal list --json failed");
    let goals: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        goals[0]["state"], "inprogress",
        "Goal must be in_progress after first task start"
    );
}

#[test]
#[allow(clippy::similar_names)]
fn test_delete_cascades_blocked_by_cleanup() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Cascade blocked_by cleanup"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Create task A (the blocker)
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task A",
            "--receives",
            "nothing",
            "--produces",
            "something",
            "--verify",
            "it works",
        ])
        .expect("Create task A failed");
    let task_a_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Create task B blocked by A
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task B",
            "--receives",
            "nothing",
            "--produces",
            "something",
            "--verify",
            "it works",
            "--blocked-by",
            task_a_id,
        ])
        .expect("Create task B failed");
    let task_b_id = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap();

    // Confirm B is blocked
    let output = env
        .run(&["task", "list", goal_id, "--json"])
        .expect("Task list failed");
    let tasks: Value = serde_json::from_str(&output).unwrap();
    let task_b_blocked = tasks
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"].as_str() == Some(task_b_id))
        .unwrap();
    assert_eq!(task_b_blocked["state"], "blocked");

    // Delete task A
    env.run(&["task", "delete", task_a_id])
        .expect("Delete task A failed");

    // Task B must now be pending, not stuck in blocked
    let output = env
        .run(&["task", "list", goal_id, "--json"])
        .expect("Task list after delete failed");
    let tasks: Value = serde_json::from_str(&output).unwrap();
    let task_b_unblocked = tasks
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"].as_str() == Some(task_b_id))
        .unwrap();
    assert_eq!(
        task_b_unblocked["state"], "pending",
        "Task B must be unblocked after its blocker is deleted"
    );
    assert!(
        task_b_unblocked["blocked_by"].is_null()
            || task_b_unblocked["blocked_by"]
                .as_array()
                .is_none_or(Vec::is_empty),
        "Task B must have no stale blocked_by entries"
    );
}

#[test]
fn test_create_task_blocked_by_completed_task_yields_pending() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Completed blocker test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|l| l.contains("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap();

    // Create task A with a contract so it can be started and completed
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task A",
            "--receives",
            "nothing",
            "--produces",
            "something",
            "--verify",
            "done",
        ])
        .expect("Create task A failed");
    let task_a = output
        .lines()
        .find(|l| l.contains("Created task:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    // Start and complete task A
    env.run(&["task", "start", &task_a, "--assignee", "test"])
        .expect("Start task A failed");
    env.run(&["task", "complete", &task_a, "--result", "done"])
        .expect("Complete task A failed");

    // Create task B blocked by the already-completed task A
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task B",
            "--receives",
            "nothing",
            "--produces",
            "something",
            "--verify",
            "done",
            "--blocked-by",
            &task_a,
        ])
        .expect("Create task B blocked by completed task A failed");
    let task_b = output
        .lines()
        .find(|l| l.contains("Created task:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    // Task B should be pending, not blocked
    let list_output = env
        .run(&["task", "list", goal_id, "--json"])
        .expect("List tasks failed");
    let tasks: Value = serde_json::from_str(&list_output).expect("Invalid JSON");
    let task_b_json = tasks
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"].as_str() == Some(&task_b))
        .expect("Task B not found in list");
    assert_eq!(
        task_b_json["state"].as_str().unwrap(),
        "pending",
        "Task created with only completed blockers must start as pending"
    );
}

#[test]
fn test_blocked_by_comma_separated() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Comma separated blocked-by test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|l| l.contains("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap();

    let make_task = |env: &TestEnv, desc: &str| -> String {
        env.run(&["task", "create", goal_id, desc])
            .expect("Create task failed")
            .lines()
            .find(|l| l.contains("Created task:"))
            .and_then(|l| l.split_whitespace().nth(2))
            .unwrap()
            .to_string()
    };

    let task_a = make_task(&env, "Task A");
    let task_b = make_task(&env, "Task B");

    // Pass both IDs as a single comma-separated value
    let combined = format!("{task_a},{task_b}");
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task C",
            "--blocked-by",
            &combined,
        ])
        .expect("Create task C with comma-separated blocked-by failed");

    let task_c = output
        .lines()
        .find(|l| l.contains("Created task:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let show = env
        .run(&["show", &task_c, "--json"])
        .expect("Show task C failed");
    let json: serde_json::Value = serde_json::from_str(&show).expect("Invalid JSON");
    let blocked_by: Vec<&str> = json["blocked_by"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        blocked_by.contains(&task_a.as_str()),
        "Task C must be blocked by Task A"
    );
    assert!(
        blocked_by.contains(&task_b.as_str()),
        "Task C must be blocked by Task B"
    );
}

#[test]
fn test_blocked_by_space_separated_unquoted() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Space separated blocked-by test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|l| l.contains("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap();

    let make_task = |env: &TestEnv, desc: &str| -> String {
        env.run(&["task", "create", goal_id, desc])
            .expect("Create task failed")
            .lines()
            .find(|l| l.contains("Created task:"))
            .and_then(|l| l.split_whitespace().nth(2))
            .unwrap()
            .to_string()
    };

    let task_a = make_task(&env, "Task A");
    let task_b = make_task(&env, "Task B");

    // Pass both IDs as separate arguments (unquoted space-separated)
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task C",
            "--blocked-by",
            &task_a,
            &task_b,
        ])
        .expect("Create task C with space-separated blocked-by failed");

    let task_c = output
        .lines()
        .find(|l| l.contains("Created task:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let show = env
        .run(&["show", &task_c, "--json"])
        .expect("Show task C failed");
    let json: serde_json::Value = serde_json::from_str(&show).expect("Invalid JSON");
    let blocked_by: Vec<&str> = json["blocked_by"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        blocked_by.contains(&task_a.as_str()),
        "Task C must be blocked by Task A"
    );
    assert!(
        blocked_by.contains(&task_b.as_str()),
        "Task C must be blocked by Task B"
    );
}

#[test]
fn test_blocked_by_mixed_comma_and_space() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Mixed comma/space blocked-by test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|l| l.contains("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap();

    let make_task = |env: &TestEnv, desc: &str| -> String {
        env.run(&["task", "create", goal_id, desc])
            .expect("Create task failed")
            .lines()
            .find(|l| l.contains("Created task:"))
            .and_then(|l| l.split_whitespace().nth(2))
            .unwrap()
            .to_string()
    };

    let task_a = make_task(&env, "Task A");
    let task_b = make_task(&env, "Task B");
    let task_c = make_task(&env, "Task C");

    // Pass two as comma-separated and one as a separate arg
    let ab = format!("{task_a},{task_b}");
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            "Task D",
            "--blocked-by",
            &ab,
            &task_c,
        ])
        .expect("Create task D with mixed blocked-by failed");

    let task_d = output
        .lines()
        .find(|l| l.contains("Created task:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let show = env
        .run(&["show", &task_d, "--json"])
        .expect("Show task D failed");
    let json: serde_json::Value = serde_json::from_str(&show).expect("Invalid JSON");
    let blocked_by: Vec<&str> = json["blocked_by"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(blocked_by.len(), 3, "Task D must have exactly 3 blockers");
    assert!(blocked_by.contains(&task_a.as_str()));
    assert!(blocked_by.contains(&task_b.as_str()));
    assert!(blocked_by.contains(&task_c.as_str()));
}

#[test]
fn test_list_truncates_descriptions_by_default() {
    let long_desc = "A".repeat(100);
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", &long_desc])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|l| l.contains("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap();

    env.run(&["task", "create", goal_id, &long_desc])
        .expect("Create task failed");

    let list_output = env.run(&["list"]).expect("List failed");
    assert!(
        !list_output.contains(&long_desc),
        "rd list must truncate long descriptions by default"
    );
}

#[test]
fn test_list_full_shows_complete_descriptions() {
    let long_desc = "A".repeat(100);
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", &long_desc])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|l| l.contains("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap();

    env.run(&["task", "create", goal_id, &long_desc])
        .expect("Create task failed");

    let full_output = env.run(&["list", "--full"]).expect("List --full failed");
    assert!(
        full_output.contains(&long_desc),
        "rd list --full must show complete descriptions"
    );
}

#[test]
fn test_task_list_full_shows_complete_descriptions() {
    let long_desc = "B".repeat(100);
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Goal for task list full test"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|l| l.contains("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap();

    env.run(&["task", "create", goal_id, &long_desc])
        .expect("Create task failed");

    let default_output = env
        .run(&["task", "list", goal_id])
        .expect("task list failed");
    assert!(
        !default_output.contains(&long_desc),
        "rd task list must truncate long descriptions by default"
    );

    let full_output = env
        .run(&["task", "list", goal_id, "--full"])
        .expect("task list --full failed");
    assert!(
        full_output.contains(&long_desc),
        "rd task list --full must show complete descriptions"
    );
}

fn create_task_with_contract(env: &TestEnv, goal_id: &str, description: &str) -> String {
    let output = env
        .run(&[
            "task",
            "create",
            goal_id,
            description,
            "--receives",
            "input",
            "--produces",
            "output",
            "--verify",
            "check",
        ])
        .expect("Create task failed");
    output
        .lines()
        .find(|l| l.contains("Created task:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap()
        .to_owned()
}

#[test]
fn test_task_comments_no_comments() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Comments test goal"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|l| l.contains("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap();

    let task_id = create_task_with_contract(&env, goal_id, "Task with no comments");

    let output = env
        .run(&["task", "comments", &task_id])
        .expect("task comments failed");
    assert!(output.contains(&task_id));
    assert!(output.contains("0 total"));
    assert!(output.contains("No comments"));
}

#[test]
fn test_task_comments_one_comment() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Comments test goal"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|l| l.contains("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap();

    let task_id = create_task_with_contract(&env, goal_id, "Task with one comment");

    env.run(&["task", "comment", &task_id, "Single comment text"])
        .expect("Add comment failed");

    let output = env
        .run(&["task", "comments", &task_id])
        .expect("task comments failed");
    assert!(output.contains(&task_id));
    assert!(output.contains("1 total"));
    assert!(output.contains("Single comment text"));
}

#[test]
fn test_task_comments_multiple_comments() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Comments test goal"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|l| l.contains("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .unwrap();

    let task_id = create_task_with_contract(&env, goal_id, "Task with multiple comments");

    env.run(&["task", "comment", &task_id, "First comment"])
        .expect("Add first comment failed");
    env.run(&["task", "comment", &task_id, "Second comment"])
        .expect("Add second comment failed");
    env.run(&["task", "comment", &task_id, "Third comment"])
        .expect("Add third comment failed");

    let output = env
        .run(&["task", "comments", &task_id])
        .expect("task comments failed");
    assert!(output.contains(&task_id));
    assert!(output.contains("3 total"));
    assert!(output.contains("First comment"));
    assert!(output.contains("Second comment"));
    assert!(output.contains("Third comment"));
}

#[test]
fn test_task_comments_nonexistent_task() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let err = env
        .run(&["task", "comments", "nonexistent123"])
        .expect_err("Expected error for nonexistent task");
    assert!(err.contains("not found") || err.contains("Task not found"));
}

#[test]
fn test_concurrent_task_claim_race() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    const NUM_AGENTS: usize = 5;

    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let goal_id = env
        .run(&["goal", "create", "Concurrent claim test"])
        .expect("Goal creation failed")
        .lines()
        .find(|l| l.starts_with("Created goal:"))
        .and_then(|l| l.split_whitespace().nth(2))
        .expect("Failed to extract goal ID")
        .to_string();

    let task_id = create_task_with_contract(&env, &goal_id, "Concurrent task");

    // Spawn N processes simultaneously, all trying to claim the same task
    let barrier = Arc::new(Barrier::new(NUM_AGENTS));
    let work_dir = Arc::new(env.work_dir.clone());
    let binary_path = Arc::new(env.binary_path.clone());
    let task_id = Arc::new(task_id);

    let handles: Vec<_> = (0..NUM_AGENTS)
        .map(|i| {
            let barrier = Arc::clone(&barrier);
            let work_dir = Arc::clone(&work_dir);
            let binary_path = Arc::clone(&binary_path);
            let task_id = Arc::clone(&task_id);

            thread::spawn(move || {
                barrier.wait(); // All threads start simultaneously

                let output = Command::new(binary_path.as_ref())
                    .args([
                        "task",
                        "start",
                        &task_id,
                        "--assignee",
                        &format!("agent-{i}"),
                    ])
                    .current_dir(work_dir.as_ref())
                    .output()
                    .expect("Failed to execute rd task start");

                (i, output.status.success())
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Exactly one agent should have successfully claimed the task
    let successful_claims = results.iter().filter(|(_, success)| *success).count();
    assert_eq!(
        successful_claims, 1,
        "Expected exactly 1 successful claim, got {successful_claims}. Results: {results:?}"
    );

    // Verify the task is no longer pending (one agent claimed it)
    let output = env.run(&["show", &task_id]).expect("show task failed");

    // The task should show one of the agents as assignee
    let has_assignee = (0..NUM_AGENTS).any(|i| output.contains(&format!("agent-{i}")));
    assert!(
        has_assignee,
        "Task should have an assignee from one of the agents"
    );
}

#[test]
fn test_prefix_resolution() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Test prefix resolution"])
        .expect("Create goal failed");
    let goal_id: String = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            "First task",
            "--receives",
            "input",
            "--produces",
            "output",
            "--verify",
            "check",
        ])
        .expect("Create task 1 failed");
    let task_id_1: String = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let output = env
        .run(&[
            "task",
            "create",
            &goal_id,
            "Second task",
            "--receives",
            "input",
            "--produces",
            "output",
            "--verify",
            "check",
        ])
        .expect("Create task 2 failed");
    let task_id_2: String = output
        .lines()
        .find(|line| line.contains("Created task:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    // Test goal prefix resolution (3-4 chars)
    let goal_prefix = &goal_id[..4];
    let output = env
        .run(&["task", "list", goal_prefix])
        .expect("List with goal prefix failed");
    assert!(output.contains("First task"));
    assert!(output.contains("Second task"));

    // Test task prefix resolution (3-4 chars, extended if needed to stay
    // unambiguous -- random IDs can share a 3-char prefix by chance).
    let diverge_at = task_id_1
        .chars()
        .zip(task_id_2.chars())
        .position(|(a, b)| a != b)
        .map_or(task_id_1.len(), |i| i + 1);
    let prefix_len = diverge_at.max(3).min(task_id_1.len());
    let task_prefix = &task_id_1[..prefix_len];
    env.run(&["task", "start", task_prefix, "--assignee", "test-agent"])
        .expect("Start with task prefix failed");

    // Verify the task was started
    let output = env
        .run(&["show", task_prefix])
        .expect("Show with prefix failed");
    assert!(output.contains("in_progress") || output.contains("InProgress"));

    // Test ambiguous prefix error
    if task_id_1.chars().next() == task_id_2.chars().next() {
        let ambiguous_prefix = &task_id_1[..1];
        let result = env.run(&["task", "start", ambiguous_prefix, "--assignee", "test"]);
        if let Err(err) = result {
            assert!(err.contains("Ambiguous") || err.contains("matches multiple"));
        }
    }

    // Test case-insensitive prefix
    let upper_prefix = goal_prefix.to_uppercase();
    let output = env
        .run(&["ready", &upper_prefix])
        .expect("Ready with uppercase prefix failed");
    assert!(output.contains("Second task"));
}

#[test]
fn test_display_ref_usage() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create first goal
    env.run(&["goal", "create", "First goal"])
        .expect("Create first goal failed");

    // Create second goal
    env.run(&["goal", "create", "Second goal"])
        .expect("Create second goal failed");

    // Create tasks for first goal using display ref g1
    env.run(&["task", "create", "g1", "Task 1 for goal 1"])
        .expect("Create task with display ref failed");

    env.run(&["task", "create", "g1", "Task 2 for goal 1"])
        .expect("Create second task with display ref failed");

    // Create task for second goal using display ref g2
    env.run(&["task", "create", "g2", "Task 1 for goal 2"])
        .expect("Create task for goal 2 failed");

    // Test rd show with goal display refs
    let output = env
        .run(&["show", "g1"])
        .expect("Show goal with display ref failed");
    assert!(output.contains("First goal"));

    let output = env
        .run(&["show", "g2"])
        .expect("Show goal 2 with display ref failed");
    assert!(output.contains("Second goal"));

    // Test rd show with task display refs
    let output = env
        .run(&["show", "g1.1"])
        .expect("Show task with display ref failed");
    assert!(output.contains("Task 1 for goal 1"));

    let output = env
        .run(&["show", "g1.2"])
        .expect("Show task 2 with display ref failed");
    assert!(output.contains("Task 2 for goal 1"));

    let output = env
        .run(&["show", "g2.1"])
        .expect("Show task for goal 2 failed");
    assert!(output.contains("Task 1 for goal 2"));

    // Test rd list output shows display refs
    let output = env.run(&["list"]).expect("List failed");
    assert!(output.contains("g1"));
    assert!(output.contains("g2"));

    // Test task edit with display refs
    env.run(&[
        "edit",
        "task",
        "g1.1",
        "--description",
        "Updated task description",
    ])
    .expect("Edit task with display ref failed");

    let output = env.run(&["show", "g1.1"]).expect("Show edited task failed");
    assert!(output.contains("Updated task description"));

    // Test JSON output includes ref field
    let output = env
        .run(&["goal", "list", "--json"])
        .expect("Goal list JSON failed");
    assert!(output.contains("\"ref\":"));
    assert!(output.contains("\"g1\""));
    assert!(output.contains("\"g2\""));

    // Test that task JSON includes ref field
    let output = env.run(&["list", "--json"]).expect("List JSON failed");
    assert!(output.contains("\"ref\":"));
}

#[test]
fn test_ready_and_task_list_default_to_single_active_goal() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // No goals at all: bare ready/list should error, not panic.
    let err = env
        .run(&["ready"])
        .expect_err("Expected no-active-goal error");
    assert!(err.contains("No active goals found"));
    let err = env
        .run(&["task", "list"])
        .expect_err("Expected no-active-goal error");
    assert!(err.contains("No active goals found"));

    // Exactly one active goal: bare ready/list should resolve to it.
    let output = env
        .run(&["goal", "create", "Only active goal"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let ready_output = env.run(&["ready"]).expect("Bare ready failed");
    assert!(ready_output.contains(&goal_id) || ready_output.contains("No tasks ready"));

    let list_output = env.run(&["task", "list"]).expect("Bare task list failed");
    assert!(list_output.contains(&goal_id));

    // A second active goal makes the bare form ambiguous.
    env.run(&["goal", "create", "Second active goal"])
        .expect("Create second goal failed");
    let err = env
        .run(&["ready"])
        .expect_err("Expected ambiguous-goal error");
    assert!(err.contains("Multiple active goals found"));
    let err = env
        .run(&["task", "list"])
        .expect_err("Expected ambiguous-goal error");
    assert!(err.contains("Multiple active goals found"));

    // Explicit goal ID still works regardless of ambiguity.
    env.run(&["ready", &goal_id])
        .expect("Explicit ready failed");
    env.run(&["task", "list", &goal_id])
        .expect("Explicit task list failed");
}

#[test]
fn test_shell_completions() {
    let env = TestEnv::new();

    for shell in ["bash", "zsh", "fish", "elvish", "powershell"] {
        let output = env
            .run(&["completions", shell])
            .unwrap_or_else(|e| panic!("completions {shell} failed: {e}"));
        assert!(
            output.contains("rd"),
            "{shell} completions should reference the 'rd' binary name"
        );
    }

    let err = env
        .run(&["completions", "not-a-shell"])
        .expect_err("Expected invalid shell to fail");
    assert!(!err.is_empty());
}

#[test]
fn test_json_output_task_comment_and_comments() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    let output = env
        .run(&["goal", "create", "Comment JSON coverage"])
        .expect("Create goal failed");
    let goal_id = output
        .lines()
        .find(|line| line.contains("Created goal:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .to_string();

    let output = env
        .run(&["task", "create", &goal_id, "Comment me", "--json"])
        .expect("Create task failed");
    let task: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    let task_id = task["id"].as_str().unwrap().to_string();

    let output = env
        .run(&["task", "comment", &task_id, "a note", "--json"])
        .expect("Comment --json failed");
    let commented: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(commented["comments"][0]["text"], "a note");

    let output = env
        .run(&["task", "comments", &task_id, "--json"])
        .expect("Comments --json failed");
    let comments: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(comments["comments"][0]["text"], "a note");
}
