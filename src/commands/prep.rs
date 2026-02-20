/// Returns the preparation guide for LLM agents using radial.
pub fn run() -> &'static str {
    r#"## rd preparation

rd is a task orchestration tool for LLM agents. It tracks goals and tasks with dependencies, letting
agents work on what's ready.

### Setup

```bash
rd init              # Initialize in current project
rd init --stealth    # Initialize without committing .radial to repo
```

### Goals

Goals are high-level objectives containing tasks.

```bash
rd goal create "Implement user authentication"   # Create a goal
rd goal list                                      # List all goals
```

### Tasks

Tasks are units of work under a goal. They can have dependencies and contracts.

```bash
# Create a task
rd task create <goal_id> "Write login handler"

# With contracts and dependencies
rd task create <goal_id> "Parse config" \
  --receives "config.yaml file path" \
  --produces "Config struct" \
  --verify "Unit tests pass" \
  --blocked-by task_abc,task_def

# List tasks for a goal
rd task list <goal_id>
```

### Task Lifecycle

```bash
rd task start <task_id>                          # Mark as started
rd task complete <task_id> --result "Added login endpoint with JWT"
rd task complete <task_id> --result "Done" --artifacts "src/auth.rs,src/jwt.rs"
rd task fail <task_id>                           # Mark as failed
rd task retry <task_id>                          # Retry a failed task
```

### Comments

Comments allow you to attach notes or progress updates to tasks. They are timestamped and
preserved in order.

```bash
rd task comment <task_id> "Started investigating the auth flow"
rd task comment <task_id> "Found the issue - missing token validation"
```

Comments are shown when viewing full task details:

```bash
rd show <task_id>
```

Use the `--verbose` flag to show comments when listing tasks:

```bash
rd task list <goal_id> --verbose
```

### Status & Ready

```bash
rd status                    # Compact overview of all goals
rd status --goal <goal_id>   # Compact status of a goal and its tasks
rd status --task <task_id>   # Compact status of a task
rd show <id>                 # Full details of a goal or task (auto-detects)
rd ready <goal_id>           # Show tasks ready to work on (unblocked)
```

### Typical Workflow

1. `rd goal create "Build feature X"` -> get goal_id
2. `rd task create <goal_id> "Task A"` -> create tasks with dependencies
3. `rd ready <goal_id>` -> see what's unblocked
4. `rd task start <task_id>` -> claim a task
5. `rd task complete <task_id> --result "..."` -> finish it
6. Repeat from step 3"#
}
