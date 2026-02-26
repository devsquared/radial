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
rd goal create "..." --json                      # Output as JSON
rd goal list                                     # List all goals
rd goal list --json                              # List as JSON
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
rd task list <goal_id> --verbose    # Include comments
rd task list <goal_id> --json       # Output as JSON
```

### Task Lifecycle

```bash
rd task start <task_id>                          # Mark as started
rd task complete <task_id> --result "Added login endpoint with JWT"
rd task complete <task_id> --result "Done" --artifacts "src/auth.rs,src/jwt.rs"
rd task complete <task_id> --result "Done" --tokens 1500 --elapsed 30000
rd task fail <task_id>                           # Mark as failed
rd task retry <task_id>                          # Retry a failed task
rd task delete <task_id>                         # Delete a pending task
```

### Comments

Comments allow you to attach notes or progress updates to tasks. They are timestamped and
preserved in order.

```bash
rd task comment <task_id> "Started investigating the auth flow"
rd task comment <task_id> "Found the issue - missing token validation"
```

### Editing

Edit goals or tasks after creation.

```bash
rd edit goal <goal_id> --description "Updated description"
rd edit task <task_id> --description "New description"
rd edit task <task_id> --receives "..." --produces "..." --verify "..."
rd edit task <task_id> --blocked-by task_abc,task_def
```

### Viewing & Status

```bash
rd list                      # All goals and tasks in dependency order
rd list --json               # Output as JSON
rd status                    # Compact overview of all goals
rd status --goal <goal_id>   # Compact status of a goal and its tasks
rd status --task <task_id>   # Compact status of a task
rd status --json             # Output as JSON
rd show <id>                 # Full details of a goal or task (auto-detects)
rd show <id> --json          # Output as JSON
rd ready <goal_id>           # Show tasks ready to work on (unblocked)
rd ready <goal_id> --json    # Output as JSON
```

### Cleanup

Remove completed or all goals and their tasks.

```bash
rd clean                     # Prompt to remove completed goals
rd clean --all               # Remove all completed goals without prompting
rd clean --force             # Remove all goals regardless of status
```

### Task Rules

- A contract (`--receives`, `--produces`, `--verify`) is required before a task can be started.
- Tasks with `--blocked-by` start in `blocked` state and move to `pending` when all blockers complete.
- Only `pending` tasks can be started or deleted.
- Only `in_progress` tasks can be completed.
- Only `in_progress` or `verifying` tasks can be failed.
- Only `failed` tasks can be retried.

### JSON Output

Most commands accept `--json` for machine-readable output.

### Typical Workflow

1. `rd goal create "Build feature X"` -> get goal_id
2. `rd task create <goal_id> "Task A"` -> create tasks with dependencies
3. `rd ready <goal_id>` -> see what's unblocked
4. `rd task start <task_id>` -> claim a task
5. `rd task complete <task_id> --result "..."` -> finish it
6. Repeat from step 3"#
}
