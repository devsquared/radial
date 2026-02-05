# Radial

Task orchestration system for LLM agents. Radial provides structure and state management for breaking down large goals into tracked, contract-bound tasks.

## Motivation

Radial was designed to address some early challenges that I ran into when working with LLMs. I noticed I had a lot of success when
I gave clear verification instructions and tied things back to a main goal. This helped me focus on how to drive the work but also 
seemed to result in better results. I started early with repeatable prompts that I would reuse and compose into workflows.You can tell 
that Beads was a heavy influence on Radial. I wanted to understand how Beads worked and that meant building a similar system. 

## Why contracts?

For me, building something has always been anchored with a main goal. You may slice goals into smaller tasks, but it is always important
to have a contract that defines the expected outcome and how that pushes the work forward towards the goal. I took this idea and applied
it early with agentic workflows. 

Radial tracks what you have, what you must produce, and how we know it worked.

This design helps agents with clear boundaries, verifiable completion, and better handoffs. The contract is also designed to be flexible,
adaptable, and small.

## Install

Currently, the best install path is a build from source. Clone the repository and utilize cargo to build the project.


## Quick Start
```bash
# Initialize in your project
rd init

# Create a goal
rd goal create "Build a REST API in Go"

# Add tasks with contracts
rd task create <goal-id> "Scaffold Go HTTP server" \
  --receives "Empty directory" \
  --produces "go.mod, main.go with http server on :8080 returning 'ok' at /" \
  --verify "curl localhost:8080 returns 'ok'"

rd task create <goal-id> "Add users endpoint" \
  --receives "Go HTTP server running on :8080" \
  --produces "GET /users endpoint returning JSON array of hardcoded names" \
  --verify "curl localhost:8080/users returns JSON with names" \
  --blocked-by <previous-task-id>
```

Then use a prompt such as the following with your agent of choice (Make sure to replace <goal-id> with the actual ID):
```
You are a senior developer implementing a basic REST API.

Use rd to coordinate. Run rd ready <goal-id> to see available tasks. Pick one, run rd task start <task-id>, do the work, then 
run rd task complete <task-id> --result '<summary>'. Check rd ready again for more work. Stop when nothing is ready. If a task 
start fails because another agent claimed it, pick a different ready task.
```

## Commands

| Command | Description |
|---------|-------------|
| `rd init` | Initialize radial in current directory |
| `rd goal create <description>` | Create a new goal |
| `rd goal list` | List all goals |
| `rd task create <goal-id> <description> [--receives, --produces, --verify, --blocked-by]` | Create a task |
| `rd task list <goal-id> [-v\|--verbose]` | List tasks for a goal |
| `rd task start <task-id>` | Claim a task (atomic) |
| `rd task complete <task-id> --result <summary> [--artifacts]` | Mark task complete |
| `rd task fail <task-id>` | Mark task as failed |
| `rd task retry <task-id>` | Retry a failed task |
| `rd task comment <task-id> <text>` | Add a comment to a task |
| `rd ready <goal-id>` | List tasks ready to start |
| `rd status [--goal <id>] [--task <id>] [--concise]` | Show status |
| `rd prep` | Output preparation guide for LLM agents |

All commands accept `--json` for machine-readable output.

## Contracts

A contract has three parts:

- **receives** — what this task gets as input (files, state, context)
- **produces** — what this task must output
- **verify** — how to confirm success (command to run, condition to check)

Contracts are optional at task creation but required before a task can start. This lets you sketch out tasks first, then fill in details.

```bash
# Create task without contract
rd task create $GOAL "Set up database"

# Add contract later
rd task contract <task-id> \
  --receives "Express app with user routes" \
  --produces "PostgreSQL schema, db.js connection pool, migrated tables" \
  --verify "psql -c 'SELECT * FROM users' succeeds"
```

## Project structure

Radial stores state in `.radial/` as JSONL files (one JSON object per line). This format is human-readable and git-friendly. It walks up parent directories to find this, so commands work from subdirectories.

```
your-project/
├── .radial/
│   ├── goals.jsonl
│   └── tasks.jsonl
├── src/
└── ...
```

### Stealth mode

Don't want to commit `.radial/`? Use stealth mode:

```bash
rd init --stealth
```

This adds `.radial/` to `.git/info/exclude` (local gitignore).

### Shared state

Multiple checkouts can share a radial database:

```bash
# In your checkout
echo "/path/to/shared/.radial" > .radial/redirect
```

Radial will follow the redirect to the shared database.

## Acknowledgments

Inspired by [Beads](https://github.com/anthropics/beads), with a focus on contracts as the core primitive.

## License

MIT
