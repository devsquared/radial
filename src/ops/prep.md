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
rd task list <goal_id> --verbose    # Include comments (truncated to terminal width)
rd task list <goal_id> --full       # Show full descriptions without truncating
rd task list <goal_id> --json       # Output as JSON
```

`--verbose` truncates long comments to fit the terminal. If you see a hint that comments were
truncated, use `rd task comments <task_id>` to read the full text.

### Subtasks

A task can have subtasks by passing `--parent` when creating. Subtasks let you break a large task
into smaller tracked units. The parent reflects the aggregate state of its children.

```bash
# Create a subtask under an existing task
rd task create <goal_id> "Write unit tests" --parent <parent_task_id>

# Subtasks follow the same lifecycle as regular tasks
rd task start <subtask_id> --assignee "agent-1"
rd task complete <subtask_id> --result "Tests written and passing"
```

Rules for subtasks:

- Subtasks cannot themselves have subtasks (one level only).
- Subtasks cannot be added to a completed or failed parent.
- A parent task cannot be started, completed, failed, retried, or released directly — work on its
  subtasks instead.
- The parent transitions to `completed` automatically when all subtasks complete.

### Task Lifecycle

```bash
rd task start <task_id> --assignee "agent-1"        # Claim and start (--assignee required)
rd task start <task_id> --assignee "agent-1" --force # Start even if blocked (override deps)
rd task complete <task_id> --result "Added login endpoint with JWT"
rd task complete <task_id> --result "Done" --artifacts "src/auth.rs,src/jwt.rs"
rd task complete <task_id> --result "Done" --tokens 1500 --elapsed 30000
rd task fail <task_id>                           # Mark as failed
rd task fail <task_id> --reason "Why it failed" # Mark failed with reason
rd task fail <task_id> --reason "..." --compact  # Compact immediately (requires --reason)
rd task retry <task_id>                          # Retry a failed task
rd task release <task_id>                        # Release claim, back to pending
rd task release --stale 1h                       # Release tasks in progress > 1 hour
rd task release --all-in-progress                # Release all in-progress tasks
rd task delete <task_id>                         # Delete a pending task
```

The `--assignee` flag is required when starting a task. It records who claimed the task,
preventing two agents/users from working on the same thing. Use `release` to unclaim a task
from any state (e.g. if you get stuck) so another agent can pick it up.

Use `--force` to start a blocked task without waiting for its blockers to complete. This
overrides dependency enforcement — use it only when you are certain the blocker's output is
already available by other means.

### Comments

Comments allow you to attach notes or progress updates to tasks. They are timestamped and
preserved in order.

```bash
rd task comment <task_id> "Started investigating the auth flow"
rd task comment <task_id> "Found the issue - missing token validation"
```

To view all comments for a task in full, without truncation:

```bash
rd task comments <task_id>
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
rd list --full               # Show full descriptions without truncating
rd list --json               # Output as JSON
rd status                    # Compact overview of all goals
rd status --goal <goal_id>   # Compact status of a goal and its tasks
rd status --task <task_id>   # Compact status of a task
rd status --json             # Output as JSON
rd show <id>                 # Full details of a goal or task (auto-detects)
rd show <id> --json          # Output as JSON
rd ready <goal_id>                # Show ready tasks, sorted by priority (p0 first)
rd ready <goal_id> --priority p0  # Ready tasks filtered by priority
rd ready <goal_id> --json    # Output as JSON (no advisories in JSON mode)
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
- Tasks with `--blocked-by` start in `blocked` state and move to `pending` when all blockers
  complete. If a blocker is already completed when the task is created, it is filtered out and
  does not block the new task.
- `--blocked-by` accepts task IDs as comma-separated or space-separated values.
- Only `pending` tasks can be started normally; `blocked` tasks can be started with `--force`.
- Only `pending` tasks can be deleted. Deleting a task removes it from any downstream
  `blocked_by` lists — tasks that were waiting only on the deleted task become `pending`.
- Only `in_progress` tasks can be completed.
- Only `in_progress` or `verifying` tasks can be failed.
- Only `failed` tasks can be retried.
- Completed tasks are auto-compacted; `rd compact apply` will reject them as already compacted.
- Failed tasks with `retry_count >= 3` are auto-compacted on the failing call.

### Compaction

Completed tasks are automatically compacted on completion using the `--result` summary. This
strips verbose history (contract, comments) while preserving the result summary and any artifact
paths, keeping context small for future agents.

Failed tasks preserve their full history by default, including comments documenting what was
tried. They become compaction candidates for manual review.

```bash
rd task fail <task_id> --reason "..." --compact  # Compact immediately on fail
rd compact apply <task_id> --summary "..."       # Compact a failed task manually
rd compact analyze                               # List tasks eligible for compaction
rd compact analyze --goal <goal_id>              # Filter to a specific goal
rd compact analyze --json                        # Output as JSON
```

Failed tasks are automatically compacted when `retry_count` reaches 3, since the failure history
has been consumed by multiple agents at that point.

`rd ready` surfaces a stale task advisory in terminal output when any tasks have been in progress
for over 2 hours.

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