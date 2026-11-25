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
cargo clippy --workspace --all-targets -- -D warnings -W clippy::pedantic
cargo nextest run --workspace --all-targets
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

Before pushing, check the commit(s) being pushed (`git log @{u}.. --format=%B` once
a remote is tracked, or `git log origin/main..HEAD --format=%B` otherwise) for any
such attribution line that crept back in. If one is found, amend it out (rewording
the message, not just deleting the line in isolation) before the push goes out.

## Dependencies

`radial-core` (the library):
- `serde` — serialization
- `toml` — TOML persistence
- `nanoid` — short IDs
- `jiff` — timestamps
- `anyhow` — error handling
- `fs2` — file locking for atomic writes
- `strsim` — fuzzy ID matching
- `strum` — enum derive macros
- `thiserror` — typed `CoreError`

`radial` (the `rd` CLI, depends on `radial-core`):
- `clap` (derive) + `clap_complete` — CLI parsing and shell completions
- `serde` + `serde_json` — JSON output
- `anyhow` — error handling
- `jiff` — timestamps (`rd task release --stale`)
- `console` — terminal colors and styling
- `rmcp` + `schemars` + `tokio` — the `rd mcp` stdio server

Dev dependencies (both crates):
- `tempfile` — temporary directories for tests
- `rstest` — parameterized tests

## File Structure

The workspace has two crates: `radial-core` (the library — models, persistence,
operations) and the root `radial` package (the `rd` CLI binary, a thin shell over
`radial-core`). `radial-core` cannot depend on `clap`, `console`, `rmcp`, or the CLI crate's
`cli.rs`/`output.rs`/`mcp.rs` — that boundary is enforced by the crate split itself, not a lint.

```
radial/
├── radial-core/
│   ├── src/
│   │   ├── lib.rs         # Public API surface, .radial dir resolution
│   │   ├── db.rs          # TOML persistence layer
│   │   ├── duration.rs    # Human-readable duration parsing
│   │   ├── error.rs       # Typed CoreError with stable codes
│   │   ├── id.rs          # ID generation
│   │   ├── helpers.rs     # Fuzzy ID matching
│   │   ├── models/
│   │   │   ├── mod.rs
│   │   │   ├── goal.rs     # Goal model and state machine
│   │   │   ├── task.rs     # Task model and state machine
│   │   │   ├── contract.rs # receives/produces/verify(_cmd) contract
│   │   │   ├── outcome.rs  # Task completion result
│   │   │   └── comment.rs  # Task comments
│   │   └── ops/
│   │       ├── mod.rs
│   │       ├── clean.rs    # rd clean / rd restore's counterpart
│   │       ├── compact.rs  # rd compact analyze/apply
│   │       ├── doctor.rs   # rd doctor health check
│   │       ├── edit.rs     # rd edit goal/task
│   │       ├── goal.rs     # rd goal create/list/cancel
│   │       ├── init.rs     # rd init
│   │       ├── list.rs     # rd list
│   │       ├── migrate.rs  # rd migrate: normalize files to canonical form
│   │       ├── prep.rs     # rd prep (+ prep.md, the embedded prompt text)
│   │       ├── ready.rs    # rd ready
│   │       ├── restore.rs  # rd restore
│   │       ├── show.rs     # rd show
│   │       ├── status.rs   # rd status
│   │       ├── task.rs     # rd task create/start/complete/fail/retry/.../comment
│   │       └── verify.rs   # verify_cmd execution
│   ├── tests/
│   │   └── library_api.rs # Drives radial-core directly, not through the CLI
│   └── Cargo.toml
├── src/
│   ├── main.rs            # Entry point
│   ├── lib.rs             # Command dispatch, thin over radial-core
│   ├── cli.rs             # Clap CLI definitions
│   ├── mcp.rs             # rd mcp: ops exposed as MCP tools over stdio
│   └── output.rs          # Terminal and JSON rendering
├── tests/
│   ├── integration_test.rs # Drives the compiled `rd` binary end to end
│   └── mcp_test.rs         # Drives `rd mcp` over stdio as a real subprocess
├── Cargo.toml              # Workspace manifest + the `radial` (rd) package
└── AGENTS.md
```

## Testing

- Unit tests go in the same file as the code (`#[cfg(test)]`)
- Integration tests in `tests/` directory
- Use `tempfile` crate for tests that need isolated state
- Use `rstest` for parameterized test cases
- Test the happy path first, then edge cases
