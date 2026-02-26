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
- Prefer iterators over manual loops
- Use `?` for early returns, not `.unwrap()` (except in tests)
- Favor `impl Into<T>` and `AsRef<T>` for flexible APIs
- Use `Default` trait where appropriate
- Destructure structs and enums explicitly
- Keep functions small and focused

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

### Build and Test
After making changes:
```bash
cargo build
cargo nextest run
```

Do not move on until both pass.

### Clippy
Run frequently:
```bash
cargo clippy --all-targets -- -D warnings -W clippy::pedantic
```

Fix warnings as they come up, not later. Common allows if too noisy:
```rust
#![allow(clippy::module_name_repetitions)]
```

### Formatting
Always format before committing:
```bash
cargo fmt
```

### CLI Changes
When adding or changing a CLI command, check whether `rd prep` output needs to be
updated. The prep command generates a guide for LLM agents, so it must stay in sync
with the available commands.

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
