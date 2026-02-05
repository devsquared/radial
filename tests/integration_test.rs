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

    /// Check if database files exist
    fn db_exists(&self) -> bool {
        self.work_dir.join(".radial").join("goals.jsonl").exists()
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
    assert!(output.contains("[pending]"));
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
    assert!(output.contains("[pending]"));
    assert!(output.contains("Receives: Nothing"));
    assert!(output.contains("Produces: Something"));
    assert!(output.contains("Verify: It exists"));

    // Goal should now be in_progress
    let output = env.run(&["goal", "list"]).expect("List goals failed");
    assert!(output.contains("[pending]") || output.contains("[in_progress]"));
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
    assert!(output.contains("[pending]"));

    // Start the task
    env.run(&["task", "start", task_id])
        .expect("Start task failed");
    let output = env
        .run(&["status", "--task", task_id])
        .expect("Status failed");
    assert!(output.contains("[in_progress]"));

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
    assert!(output.contains("[completed]"));
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

    env.run(&["task", "start", task_id]).expect("Start failed");
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

    let output = env
        .run(&["status", "--task", task_id])
        .expect("Status failed");
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
    assert!(output.contains("[blocked]"));
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

    env.run(&["task", "start", task_id]).expect("Start failed");
    env.run(&["task", "complete", task_id, "--result", "All done"])
        .expect("Complete failed");

    // Goal should now be completed
    let output = env
        .run(&["status", "--goal", goal_id])
        .expect("Status failed");
    assert!(output.contains("[completed]"));
    assert!(output.contains("Tasks: 1 total, 1 completed, 0 failed"));
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
    assert!(output.contains("All Goals:"));
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
        path.push("radial");
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

    // Check that status shows comments
    let output = env
        .run(&["status", "--task", task_id])
        .expect("Status failed");
    assert!(output.contains("Comments:"));
    assert!(output.contains("First comment on task"));
    assert!(output.contains("Second comment with more detail"));
}

#[test]
fn test_task_comments_concise_flag() {
    let env = TestEnv::new();
    env.run(&["init"]).expect("Init failed");

    // Create goal and task
    let output = env
        .run(&["goal", "create", "Concise test"])
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
            "Task for concise test",
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
    env.run(&["task", "comment", task_id, "This comment should be hidden"])
        .expect("Add comment failed");

    // Without --concise, comments should appear
    let output = env
        .run(&["status", "--task", task_id])
        .expect("Status failed");
    assert!(output.contains("Comments:"));
    assert!(output.contains("This comment should be hidden"));

    // With --concise, comments should NOT appear
    let output = env
        .run(&["status", "--task", task_id, "--concise"])
        .expect("Status --concise failed");
    assert!(!output.contains("Comments:"));
    assert!(!output.contains("This comment should be hidden"));
    // But the task info should still be there
    assert!(output.contains(task_id));
    assert!(output.contains("Task for concise test"));
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

    // JSON output should include comments even with --concise
    let output = env
        .run(&["status", "--task", task_id, "--json", "--concise"])
        .expect("Status --json --concise failed");
    let parsed: Value = serde_json::from_str(&output).expect("Should be valid JSON");
    let comments = parsed["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2, "JSON should always include comments");
}
