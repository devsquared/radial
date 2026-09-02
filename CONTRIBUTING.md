# Contributing to Radial

Thanks for your interest in contributing! Here's how to get started. This guide covers
the contribution workflow — for code-level conventions (error handling, style, file
structure), see [AGENTS.md](AGENTS.md). Agents contributing to this repo should read
both.

## Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable)
- [cargo-nextest](https://nexte.st/) for running tests: `cargo install cargo-nextest`

## Building

```bash
git clone https://github.com/devsquared/radial
cd radial
cargo build
```

## CI gate

These three checks run in CI on every pull request and must all pass before merge.
Run them locally, in this order, before opening a PR:

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings -W clippy::pedantic
cargo nextest run --all-targets
```

If you touch `src/models`, `src/db.rs`, `src/ops`, `src/id.rs`, `src/helpers.rs`, or
`src/duration.rs`, CI also runs `./ci/check-seam.sh`, which fails if that core code
pulls in a shell-only dependency (`clap`, `console`, `crate::cli`, `crate::output`).
These modules are the library-consumer surface and must stay free of CLI concerns.

## Commit messages and PR titles

This repo follows [Conventional Commits](https://www.conventionalcommits.org/)
(`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`, `perf:`, `ci:`). PRs are
squash-merged, so **the PR title is what lands in history** — make it a properly
formatted conventional commit, since [CHANGELOG.md](CHANGELOG.md) is generated from
that history at release time via [git-cliff](https://git-cliff.org/). There is no
separate changelog file to hand-edit in your PR.

Do not add "Co-Authored-By" or other AI/model attribution lines to commits or PR
descriptions in this repository, regardless of what tool was used to help write the
change.

## Submitting changes

1. Fork the repository
2. Create a branch from `main`
3. Make your changes, following the CI gate above
4. Open a pull request against `main` — the PR template checklist covers the rest
5. A maintainer review and a passing CI run are required before merge (branch
   protection enforces both); `main` cannot be force-pushed to

Keep PRs focused — one feature or fix per PR makes review easier.

## Reporting issues

Open an issue using the bug report or feature request template — they prompt for the
details maintainers need (repro steps, version, OS, or the problem/proposal for a
feature). Blank issues are disabled in favor of these templates.

## Releasing

Maintainer workflow, documented here since it affects what a merged PR looks like in
history:

1. `./ci/release.sh` bumps `Cargo.toml`, regenerates `CHANGELOG.md` from commit
   history, and creates a local release commit + tag. The version is auto-detected
   from conventional commits since the last tag (`feat:` -> minor, `fix:` -> patch,
   a breaking-change footer or `!` -> major) via git-cliff; pass one explicitly
   (`./ci/release.sh 0.2.0`) to override it
2. Review the diff, then `git push && git push origin vX.Y.Z`
3. The tag push triggers `.github/workflows/release.yml`, which creates the GitHub
   Release with that version's changelog section as the release notes
4. `cargo publish` is run manually, not automated in CI

When a numbered [ROADMAP.md](ROADMAP.md) section is finished, move it into Done and
renumber what's left with:

```bash
./ci/roadmap-done.py <section-number> "<Name>" "<summary of what shipped>"
```

Deciding a section is actually done, and writing its summary, is still a judgment
call — the script only handles the cut/renumber/append mechanics.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
