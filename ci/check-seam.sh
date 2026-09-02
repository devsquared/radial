#!/usr/bin/env bash
# Fails if core (library-bound) code imports a shell-only dependency.
#
# Matches the path anywhere in the file, not just after `use` -- a
# `use`-only check misses fully-qualified references such as a derive
# path (`#[derive(clap::ValueEnum)]`), which is exactly how this repo
# violated the seam before. Word boundaries keep `crate::cli` from
# matching `crate::client` or similar.
set -euo pipefail

CORE_PATHS=(
  src/models
  src/db.rs
  src/ops
  src/id.rs
  src/helpers.rs
  src/duration.rs
)

PATTERN='\b(console|clap)::|\bcrate::(output|cli)\b'

if grep -rn --include='*.rs' -E "$PATTERN" "${CORE_PATHS[@]}"; then
  echo "seam violation: core module depends on a shell-only crate or module (console, clap, crate::output, crate::cli)" >&2
  exit 1
fi

echo "seam check passed: no shell dependencies found in core modules"
