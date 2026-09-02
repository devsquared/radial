# Changelog

All notable changes to this project are documented in this file.
Generated from [conventional commits](https://www.conventionalcommits.org/) with [git-cliff](https://git-cliff.org/).

## [0.1.0] - 2026-09-02

### Bug Fixes

- Goal started too soon
- Stale tasks on goal delete
- Blocked-by deadlock
- Refactor output with bugfixes
- Default Metrics counters and move Render impls out of models
- Resolve legacy mixed-case goal/task IDs case-insensitively
- Flaky integraton test

### Documentation

- Add CLI change guidance to AGENTS.md
- Update rd prep to cover all CLI commands
- Prohibit AI attribution lines in commits and PR descriptions
- Add doc comments to the public API surface
- Sync rd prep output with the general cleanup pass

### Features

- Add clean command for removing completed goals
- Add list command with dependency-ordered tasks
- Add edit command for goals and tasks
- Assignees
- Task-priorities
- Add delete command for pending tasks
- Better release/stale task recovery
- Summary-compact
- Subtasks
- Comments command
- Better compaction
- Add database-wide advisory locking to prevent TOCTOU races
- Add database-wide advisory locking to prevent TOCTOU races
- Implement safer ID alphabet (Layer 1)
- Implement prefix resolution (Layer 2)
- Git-like ref task and goal references
- Add task and goal cancellation
- Add archive system for completed and cancelled goals
- Add Database::save_task/save_goal, take discovery path as param
- Route all writes through Database::save_task/save_goal
- Move printing out of commands::clean and commands::init

### Miscellaneous Tasks

- Add GitHub Actions workflow with clippy pedantic and fmt checks
- Fix formatting and clippy pedantic warnings
- Rename CLAUDE.md to AGENTS.md and update contents
- Better cli
- Validate ids
- Validate block by task in edit flow
- Task cyclical detection
- Clean up prep output
- Fix ci clippy flagged issue
- Fix unsound rand by updating crate. found by dependabot
- Fix ci clippy flagged issue
- Fix fmt
- Rename commands module to ops
- Lock down the public API surface
- Add seam check to CI
- General cleanup pass on JSON coverage, completions, and docs
- Prepare repo for 0.1 open-source launch

### Testing

- Add library-consumer integration test
- Drop PR-plan cross-reference from library_api.rs doc comment

