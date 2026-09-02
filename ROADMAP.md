# Roadmap

Where radial is headed, in order. Completed work is summarized at the bottom.

## 1. Open-source readiness & 0.1 launch

Get the repo in shape for public contribution, then publish `radial` 0.1 to crates.io.

- LICENSE file (Cargo.toml declares MIT; the repo needs the text)
- CHANGELOG.md and a changelog process (conventional commits are already in use — generate from history, maintain per release)
- Contribution lockdown: branch protection on `main`, PR template with a checklist (tests, changelog entry, conventional commit title), issue templates, CONTRIBUTING.md expanded to cover the workflow — for humans and agents alike (AGENTS.md carries the code-level guidance)
- Final README pass as the crates.io landing page
- `cargo publish` and tag `v0.1.0`

## 2. Executable verification

Contracts become enforceable. Radial learns exactly one new thing — exit codes — and stays a tracking tool.

- Optional `verify_cmd` on tasks (the precise check) and goals (a human-set floor); both must exit 0
- `rd task complete` runs the checks and refuses on failure: the task stays `in_progress`, the output tail is saved as a comment, and the error lists every way to get unstuck
- `rd task verify` as an advisory dry run
- Escape hatches, frictionless but never silent: `--skip-verify` on a completion bypasses checks once; `rd edit goal --skip-verify` disables verification goal-wide until re-enabled; any check is editable or droppable, with every change and skip recorded as a comment
- The lock is never held while a check runs; state is re-checked before recording
- `Verifying` stays reserved for a possible future doer/checker two-phase flow

Sequenced before the MCP server deliberately: `task_complete` should launch with enforcement built in, not gain it later as a behavior change.

## 3. MCP server

Radial as a first-class tool surface for any MCP client — the CLI becomes the human and debug interface.

- `rd mcp` subcommand (stdio transport) on the official Rust SDK; one binary, one-block client config
- Stateless reload-per-call under the existing lock — multiple agent sessions coordinate through the filesystem exactly like CLI invocations
- ~12 tools mirroring `ops`, including `task_cancel` and verification-gated `task_complete`; destructive operations (`clean`, `delete`, `--purge`) stay CLI-only
- prep guidance embedded as server `instructions` so agents are onboarded at connect time
- Response types shared with `--json` output via the core, so the two surfaces cannot drift
- Open questions to settle from dogfooding: assignee identity, project/cwd scoping, distribution for non-Rust users

## 4. Plan-change ergonomics review

After 0.1 has real usage: dogfood a goal, deliberately change direction mid-flight, and log every friction point. Editing, untangling, and reshaping tasks/goals should be easy — supported clearly, not encouraged silently.

Workflows to stress:

- Rewiring `blocked_by` on live tasks; usable errors when a rewire would cycle
- Moving tasks between goals; promoting/demoting subtasks; splitting a task while preserving history
- Amending `receives`/`produces` mid-stream, with downstream tasks getting a signal their inputs changed
- Bulk operations (cancel/edit several tasks in one command)
- Error-message-as-documentation everywhere: every "you can't do that" lists the commands that unstick you
- Undo-shaped gaps: which reversals (`uncancel`, un-complete) does real usage actually demand?

Output: a punch list ranked by observed pain, folded into the next minor release.

## Horizon (unscheduled)

- **TUI / watch mode** — `rd tui`: a live board of goals and tasks refreshing as agents claim, complete, and comment; the human supervision surface for agent swarms. A pure reader under the shared lock.
- **Two-phase verification** — wire the reserved `Verifying` state for doer/checker separation (worker verifies, a different party confirms) if demand shows up.
- **Cross-goal dependencies** — needs a cascade boundary rule for cancellation.
- **`radial-core` workspace split** — mechanical after the seam work; happens when a second in-repo consumer (likely the MCP server's dependencies) makes it worthwhile.
- **Per-goal `workdir`** for verification commands, if invocation-cwd proves painful in multi-crate workspaces.

## Open decisions

- Assignee identity over MCP: self-reported param vs. session-derived default (decide from dogfooding)
- `--skip-verify` reason: optional on the CLI, required at the MCP tool layer (current lean)
- Archive retention: does the attic ever need `--purge --older-than`? (defer until it hurts)
- `uncancel`: wait for evidence of accidental cancellations
- Prebuilt binaries / npm-wrapped distribution for non-Rust MCP users (decide near MCP launch)

## Done

- **Database-wide locking** — advisory flock held across the full load→mutate→write cycle (`DbLock`, `open_for_write`/`open_for_read`), with a multi-process claim-race integration test. Makes "atomic task claiming" true.
- **Humane IDs** — confusable-free lowercase alphabet, case-folding input, git-style unique-prefix resolution, and sequential display refs (`g1`, `g1.3`) computed at load; full nanoids remain the stable machine identity.
- **Cancellation & archive** — `Cancelled` as a terminal state on tasks and goals with reason comments, dependent auto-unblocking (+ `--cascade`), archive-by-default `clean` with `restore`, and `#[non_exhaustive]` state enums.
- **Core seam & public API** — presentation and persistence moved out of the models, `ops` as the supported library surface, curated re-exports with doc comments, a library-consumer integration test, and a CI seam check keeping shell dependencies out of the core.
- **Quick wins** — README/CLI drift fixed, `--json` on all mutations (including `unblocked_task_ids` from complete), default goal context for `ready`/`list`, shell completions, elapsed time derived from `started_at`.
