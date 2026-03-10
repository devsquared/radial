# Contributing to Radial

Thanks for your interest in contributing! Here's how to get started.

## Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable)

## Building

```bash
git clone https://github.com/devsquared/radial
cd radial
cargo build
```

## Testing

```bash
cargo test
```

## Code style

This project uses standard Rust tooling:

- **Format**: `cargo fmt` — run before committing
- **Lint**: `cargo clippy` — all warnings should be resolved

CI checks both of these on every pull request.

## Submitting changes

1. Fork the repository
2. Create a branch from `main`
3. Make your changes
4. Run `cargo fmt`, `cargo clippy`, and `cargo test`
5. Open a pull request against `main`

Keep PRs focused — one feature or fix per PR makes review easier.

## Reporting issues

Open an issue on GitHub with:

- What you expected to happen
- What actually happened
- Steps to reproduce
- Radial version (`rd --version`) and OS

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
