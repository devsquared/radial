## rd preparation

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

# With priority (p0, p1, p2, p3 — defaults to p2)
rd task create <goal_id> "Fix critical bug" --priority p0

# With contracts and dependencies
rd task create <goal_id> "Parse config" \
  --receives "config.yaml file path" \
  --produces "Config struct" \
  --verify "Unit tests pass" \
  --blocked-by task_abc,task_def

# List tasks for a goal
rd task list <goal_id>

# Filter tasks by priority
rd task list <goal_id> --priority p0
rd task list <goal_id> --verbose    # Include comments
rd task list <goal_id> --json       # Output as JSON
```

### Task Lifecycle

```bash
rd task start <task_id> --assignee "agent-1"     # Claim and start (--assignee required)
rd task complete <task_id> --result "Added login endpoint with JWT"
rd task complete <task_id> --result "Done" --artifacts "src/auth.rs,src/jwt.rs"
rd task complete <task_id> --result "Done" --tokens 1500 --elapsed 30000
rd task fail <task_id>                           # Mark as failed
rd task retry <task_id>                          # Retry a failed task
rd task release <task_id>                        # Release claim, back to pending
rd task release --stale 1h                       # Release tasks in progress > 1 hour
rd task release --all-in-progress                # Release all in-progress tasks
rd task delete <task_id>                         # Delete a pending task
```

The `--assignee` flag is required when starting a task. It records who claimed the task,
preventing two agents/users from working on the same thing. Use `release` to unclaim a task
from any state (e.g. if you get stuck) so another agent can pick it up.

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
rd ready <goal_id>                # Show ready tasks, sorted by priority (p0 first)
rd ready <goal_id> --priority p0  # Ready tasks filtered by priority
rd ready <goal_id> --json    # Output as JSON
```

Filter by assignee to see only your tasks:

```bash
rd task list <goal_id> --assignee "agent-1"
rd status --goal <goal_id> --assignee "agent-1"
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

### Stale Task Recovery

If an agent session crashes, its claimed tasks stay in progress forever. Use these commands to recover:

- `rd task release --stale 1h` — release tasks that have been in progress for over 1 hour
- `rd task release --all-in-progress` — release all in-progress tasks (hard reset)

### JSON Output

Most commands accept `--json` for machine-readable output.

### Typical Workflow

1. `rd goal create "Build feature X"` -> get goal_id
2. `rd task create <goal_id> "Task A"` -> create tasks with dependencies
3. `rd ready <goal_id>` -> see what's unblocked
4. `rd task start <task_id> --assignee "agent-1"` -> claim and start a task
5. `rd task complete <task_id> --result "..."` -> finish it
6. Repeat from step 3

If you get stuck on a task, add a comment explaining why and release it:

```bash
rd task comment <task_id> "Blocked on missing API credentials"
rd task release <task_id>
```