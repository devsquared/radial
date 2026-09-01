# AGENTS.md

Guidelines for working on Radial.

## Project Context

Radial is a task orchestration CLI for LLM agents. It uses contract-based coordination
to break goals into tracked, verifiable tasks. State is persisted as TOML files in a
`.radial/` directory.

## Code Style

### Error Handling
- Use `anyhow` for application errors (`anyhow::Result`, `anyhow::bail!`, `anyhow::Context`)
- Add context to errors: `.context("failed to open database")`
- Reserve `thiserror` for library-style typed errors only if needed later

### Idiomatic Rust
- Use iterators over manual loops
- Use `?` for early returns, not `.unwrap()` (except in tests)
- Favor `impl Into<T>` and `AsRef<T>` for flexible APIs
- Use `Default` trait where appropriate
- Destructure structs and enums explicitly
- Keep functions small and focused
- Create getters and setters for struct fields

### Comments
- Only comment *why*, not *what*
- Complex logic or non-obvious decisions get comments
- No commented-out code
- No obvious comments like `// create a new goal`

## Documentation
- Use clear and concise language in longer form documentation.
- Use examples to illustrate usage and expected behavior.
- Write in clean and proper markdown formatting. Do not use emojis.
- Follow the [Google Style Guide](https://developers.google.com/style) for documentation.
- Avoid language like "obvious", "simple", or "trivial" when describing code or process.

## Workflow

### CI Gate (required before every commit)

All three checks below must pass before any commit. Run them in this order:

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings -W clippy::pedantic
cargo nextest run --all-targets
```

If any check fails, fix the issue and re-run from that step. Do not commit until
all three pass. These are the same checks that run in CI — a commit that fails
locally will fail the pipeline.

### Notes on Each Check

- **Formatting** (`cargo fmt`): Run first so clippy and tests see formatted code.
- **Clippy** (`cargo clippy ... -W clippy::pedantic`): Fix warnings as they come up,
  not later. Common allows if too noisy:
  ```rust
  #![allow(clippy::module_name_repetitions)]
  ```
- **Tests** (`cargo nextest run`): Includes unit and integration tests. Do not move on
  until all pass.

### Commit Messages

Do not append "Co-Authored-By: <model name>" or any other Claude/AI attribution
lines (session links included) to commit messages or pull request descriptions
in this repository.

## Dependencies

- `clap` (derive) — CLI parsing
- `serde` + `serde_json` — serialization, JSON output for models
- `toml` — TOML persistence
- `nanoid` — short IDs
- `jiff` — timestamps
- `anyhow` — error handling
- `fs2` — file locking for atomic writes
- `strsim` — fuzzy ID matching
- `strum` — enum derive macros
- `console` — terminal colors and styling
- `textwrap` — text wrapping for output formatting

Dev dependencies:
- `tempfile` — temporary directories for tests
- `rstest` — parameterized tests

## File Structure

```
radial/
├── src/
│   ├── main.rs           # Entry point
│   ├── lib.rs            # Core logic, radial dir resolution, command dispatch
│   ├── cli.rs            # Clap CLI definitions
│   ├── db.rs             # TOML persistence layer
│   ├── duration.rs       # Human-readable duration parsing
│   ├── id.rs             # ID generation
│   ├── helpers.rs         # Fuzzy ID matching
│   ├── output.rs         # Terminal and JSON rendering
│   ├── models/
│   │   ├── mod.rs
│   │   ├── goal.rs       # Goal model and state machine
│   │   ├── task.rs       # Task model and state machine
│   │   ├── contract.rs   # receives/produces/verify contract
│   │   ├── outcome.rs    # Task completion result
│   │   └── comment.rs    # Task comments
│   └── commands/
│       ├── mod.rs
│       ├── init.rs       # rd init
│       ├── goal.rs       # rd goal create/list
│       ├── task.rs       # rd task create/list/start/complete/fail/retry/comment
│       ├── status.rs     # rd status
│       ├── ready.rs      # rd ready
│       └── prep.rs       # rd prep
├── tests/
│   └── integration_test.rs
├── Cargo.toml
└── AGENTS.md
```

## Testing

- Unit tests go in the same file as the code (`#[cfg(test)]`)
- Integration tests in `tests/` directory
- Use `tempfile` crate for tests that need isolated state
- Use `rstest` for parameterized test cases
- Test the happy path first, then edge cases
