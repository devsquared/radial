# Radial

Task orchestration for LLM agents. Break down goals into tracked, contract-bound tasks with clear inputs, outputs, and verification.

## Table of Contents

- [Highlights](#highlights)
- [Overview](#overview)
- [Usage](#usage)
- [Installation](#installation)
- [Contributing](#contributing)

## Highlights

- **Contract-driven tasks** — every task defines what it receives, what it produces, and how to verify success
- **Dependency tracking** — tasks can block on others, with automatic cycle detection
- **Multi-agent coordination** — atomic task claiming prevents conflicts when multiple agents work in parallel
- **Git-friendly persistence** — state lives in `.radial/` as TOML files, easy to commit and diff
- **Stealth mode** — keep `.radial/` out of version control with `rd init --stealth`
- **Shared state** — multiple checkouts can share a radial database via redirect files
- **JSON output** — every command supports `--json` for machine-readable output
- **Task-driven memory** — goals and tasks persist across sessions, giving agents durable context for long-running workflows
- **Agent onboarding** — `rd prep` outputs a usage guide you can drop straight into a prompt

## Overview

Radial is a CLI tool that brings structure to agentic workflows. Instead of handing an LLM a vague goal, you break work into tasks with contracts: what goes in, what comes out, and how to check it worked. This gives agents clear boundaries, verifiable completion criteria, and better handoffs between steps.

I built Radial after noticing that LLM agents produce significantly better results when given explicit verification instructions tied back to a main goal. Inspired by [Beads](https://github.com/steveyegge/beads), Radial takes contracts as its core primitive.

## Usage

### Quick start

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

### Letting agents drive

If you want an agent to handle the full workflow on its own, tell it to use `rd` as its task management system.
Tell the agent to run `rd prep` to prepare the agent. It gives the agent everything it needs to discover goals, 
pick tasks, and complete work autonomously.

For a more hands-on approach, you can write your own prompt. Here's an example (replace `<goal-id>` with the actual ID):

```
You are a senior developer implementing a basic REST API.

Use rd to coordinate. Run rd ready <goal-id> to see available tasks. Pick one, run rd task start <task-id>,
do the work, then run rd task complete <task-id> --result '<summary>'. Check rd ready again for more work.
Stop when nothing is ready. If a task start fails because another agent claimed it, pick a different ready task.
```

### Commands

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
| `rd task release <task-id>` | Release a claimed task |
| `rd ready <goal-id>` | List tasks ready to start |
| `rd status [--goal <id>] [--task <id>] [--concise]` | Show status |
| `rd compact analyze` | Find tasks eligible for compaction |
| `rd compact apply <task-id> --summary <text>` | Compact a completed task |
| `rd clean` | Remove completed goals |
| `rd prep` | Output preparation guide for LLM agents |

All commands accept `--json` for machine-readable output.

### Contracts

A contract has three parts:

- **receives** — what this task gets as input (files, state, context)
- **produces** — what this task must output
- **verify** — how to confirm success (command to run, condition to check)

Contracts are optional at creation but required before a task can start:

```bash
# Create task without contract
rd task create $GOAL "Set up database"

# Add contract later
rd task contract <task-id> \
  --receives "Express app with user routes" \
  --produces "PostgreSQL schema, db.js connection pool, migrated tables" \
  --verify "psql -c 'SELECT * FROM users' succeeds"
```

### Project structure

Radial stores state in `.radial/` as TOML files. It walks up parent directories to find this, so commands work from subdirectories.

```
your-project/
├── .radial/
│   ├── goals/
│   └── tasks/
├── src/
└── ...
```

## Installation

Build from source with Cargo:

```bash
git clone https://github.com/devsquared/radial
cd radial
cargo install --path .
```

This places the `rd` binary in your Cargo bin directory.

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on building, testing, and submitting changes.

## License

MIT
