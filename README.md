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
- **Subtasks** — break a task into one level of tracked child tasks; the parent's state follows its children automatically
- **Multi-agent coordination** — atomic task claiming prevents conflicts when multiple agents work in parallel
- **Cancellation and archiving** — cancel goals or tasks (optionally cascading to dependents), archive or delete finished goals with `rd clean`, and bring an archived goal back with `rd restore`
- **Git-friendly persistence** — state lives in `.radial/` as TOML files, easy to commit and diff
- **Stealth mode** — keep `.radial/` out of version control with `rd init --stealth`
- **Shared state** — multiple checkouts can share a radial database via redirect files
- **JSON output** — nearly every command supports `--json` for machine-readable output, including which tasks a completion or cancellation unblocked
- **Shell completions** — `rd completions <shell>` covers bash, zsh, fish, elvish, and PowerShell
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

Use rd to coordinate. Run rd ready <goal-id> to see available tasks. Pick one, run
rd task start <task-id> --assignee <your-name>, do the work, then run
rd task complete <task-id> --result '<summary>'. Check rd ready again for more work.
Stop when nothing is ready. If a task start fails because another agent claimed it, pick a different ready task.
```

### Commands

Every command below also accepts `--help` for its full flag reference; `rd prep` prints a longer
walkthrough aimed at agents. Almost every command also accepts `--json` for machine-readable output
(the only exception is `rd completions`, whose output already is the intended format).

#### Setup

| Command | Description |
|---------|-------------|
| `rd init [--stealth]` | Initialize radial in the current directory. `--stealth` excludes `.radial/` from git instead of committing it |
| `rd completions <shell>` | Print a completion script for `bash`, `zsh`, `fish`, `elvish`, or `powershell` |

#### Goals

| Command | Description |
|---------|-------------|
| `rd goal create <description>` | Create a new goal |
| `rd goal list` | List all active (non-archived) goals |
| `rd goal cancel <goal-id> [--reason <text>]` | Cancel a goal and every non-terminal task under it |

#### Tasks

| Command | Description |
|---------|-------------|
| `rd task create <goal-id> <description> [--priority p0-p3] [--parent <task-id>] [--receives ... --produces ... --verify ...] [--blocked-by <ids>]` | Create a task, optionally as a subtask, with a contract and/or dependencies |
| `rd task list [<goal-id>] [--priority] [--assignee] [--verbose] [--full]` | List tasks for a goal. The goal ID can be omitted if you have exactly one active goal |
| `rd task comment <task-id> <text>` | Add a timestamped comment to a task |
| `rd task comments <task-id>` | View all comments on a task, untruncated |

#### Task lifecycle

| Command | Description |
|---------|-------------|
| `rd task start <task-id> --assignee <name> [--force]` | Claim and start a task (atomic). `--assignee` is required; `--force` overrides a `blocked` state |
| `rd task complete <task-id> --result <summary> [--artifacts <paths>] [--tokens <n>] [--elapsed <ms>]` | Mark a task complete. If `--elapsed` is omitted, it's derived from how long the task was in progress |
| `rd task fail <task-id> [--reason <text>] [--compact]` | Mark a task as failed |
| `rd task cancel <task-id> [--reason <text>] [--cascade]` | Cancel a task that's no longer needed. `--cascade` also cancels its downstream dependents |
| `rd task retry <task-id>` | Retry a failed task |
| `rd task release <task-id>` | Release a claimed task back to `pending` |
| `rd task release --stale <duration>` | Release tasks that have been in progress longer than `<duration>` (e.g. `1h`, `30m`) |
| `rd task release --all-in-progress` | Release every in-progress task, regardless of duration |
| `rd task delete <task-id>` | Delete a `pending` task |

#### Editing

| Command | Description |
|---------|-------------|
| `rd edit goal <goal-id> --description <text>` | Update a goal's description |
| `rd edit task <task-id> [--description] [--priority] [--receives --produces --verify] [--blocked-by]` | Update a task's description, priority, contract, and/or dependencies |

#### Viewing and status

| Command | Description |
|---------|-------------|
| `rd list [--full] [--archived]` (alias `rd ls`) | All goals and their tasks, in dependency order |
| `rd show <id>` | Full details of a goal or task; auto-detects which one `<id>` refers to |
| `rd status [--goal <id>] [--task <id>] [--assignee <name>]` | Compact status overview, optionally scoped to one goal, one task, or one assignee |
| `rd ready [<goal-id>] [--priority p0-p3]` | Tasks ready to start, sorted by priority. The goal ID can be omitted if you have exactly one active goal |

#### Cleanup and archiving

| Command | Description |
|---------|-------------|
| `rd clean [--all] [--force] [--purge]` | Archive completed/cancelled goals, prompting per goal unless `--all` is set. `--force` includes goals in any state; `--purge` deletes instead of archiving |
| `rd restore <goal-id>` | Restore an archived goal back into the active database |

#### Compaction

| Command | Description |
|---------|-------------|
| `rd compact analyze [--goal <id>]` | List tasks eligible for compaction |
| `rd compact apply <task-id> --summary <text>` | Replace a completed/failed task's detailed history with a summary |

#### Agent onboarding

| Command | Description |
|---------|-------------|
| `rd prep` | Print a full usage guide for LLM agents, suitable for pasting straight into a prompt |

### Contracts

A contract has three parts:

- **receives** — what this task gets as input (files, state, context)
- **produces** — what this task must output
- **verify** — how to confirm success (command to run, condition to check)

A contract is required before a task can be started. Set it at creation time, or add it later with `rd edit task`:

```bash
# Create a task without a contract
rd task create $GOAL "Set up database"

# Add the contract before starting it
rd edit task <task-id> \
  --receives "Express app with user routes" \
  --produces "PostgreSQL schema, db.js connection pool, migrated tables" \
  --verify "psql -c 'SELECT * FROM users' succeeds"
```

### Subtasks

A task can have subtasks by passing `--parent` at creation. Subtasks are one level deep — a subtask
cannot itself have subtasks — and follow the same lifecycle as any other task. A parent task can't be
started, completed, failed, retried, or released directly; work happens on its subtasks, and the
parent transitions to `completed` automatically once all of them are resolved.

```bash
rd task create <goal-id> "Write unit tests" --parent <parent-task-id>
```

### Display refs

Goals and tasks get short, human-friendly refs alongside their IDs: a goal is `g<N>` (e.g. `g1`) and a
task is `g<N>.<M>` (e.g. `g1.3`), numbered in creation order within the goal. Refs work anywhere an ID
is accepted:

```bash
rd show g1
rd task start g1.3 --assignee agent-1
```

### Cancellation, archiving, and cleanup

Cancelling and archiving are distinct: `rd goal cancel`/`rd task cancel` change state without touching
the database layout, while `rd clean` is what actually moves a goal's files into `.radial/archive/` (or
deletes them, with `--purge`). Archived goals are excluded from `rd goal list` and `rd list`; pass
`--archived` to `rd list` to see them, and `rd restore <goal-id>` to bring one back.

### Shell completions

```bash
# zsh
rd completions zsh > "${fpath[1]}/_rd"

# bash
rd completions bash > /etc/bash_completion.d/rd

# fish
rd completions fish > ~/.config/fish/completions/rd.fish
```

### Project structure

Radial stores state in `.radial/` as TOML files. It walks up parent directories to find this, so commands work from subdirectories.

```
your-project/
├── .radial/
│   ├── .lock                  # write-lock file
│   ├── <goal-id>/
│   │   ├── goal.toml
│   │   └── <task-id>.toml     # one file per task under the goal
│   ├── archive/                # goals moved here by `rd clean` (without --purge)
│   │   └── <goal-id>/...
│   └── redirect                # optional: points at a shared .radial/ elsewhere
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
