#!/usr/bin/env bash
# Cuts a release: bumps Cargo.toml, regenerates CHANGELOG.md from
# conventional-commit history via git-cliff, and creates the release
# commit + tag locally. Nothing is pushed -- review the diff first,
# then `git push && git push origin vX.Y.Z` to trigger the release
# workflow (which builds GitHub release notes from the tag).
#
# Version defaults to git-cliff's own bump detection (feat -> minor,
# fix -> patch, a breaking-change footer/! -> major, per conventional
# commits since the last tag) -- pass one explicitly to override it.
set -euo pipefail

if [[ $# -gt 1 ]]; then
  echo "usage: $0 [version]   (e.g. $0 0.2.0; omit to auto-detect from commits)" >&2
  exit 1
fi

if ! command -v git-cliff >/dev/null 2>&1; then
  echo "error: git-cliff is not installed (brew install git-cliff)" >&2
  exit 1
fi

if [[ $# -eq 1 ]]; then
  tag="${1#v}"
  tag="v${tag}"
else
  tag="$(git-cliff --bumped-version 2>/dev/null)"
fi
version="${tag#v}"
repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if [[ -n "$(git status --porcelain)" ]]; then
  echo "error: working tree is not clean" >&2
  exit 1
fi

if git rev-parse "$tag" >/dev/null 2>&1; then
  echo "error: tag $tag already exists" >&2
  exit 1
fi

echo "releasing ${tag}"

sed -i.bak "0,/^version = \".*\"/s//version = \"${version}\"/" Cargo.toml
rm -f Cargo.toml.bak
cargo update --workspace --offline --quiet || cargo update --workspace --quiet

git-cliff --tag "$tag" -o CHANGELOG.md

git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore(release): ${tag}"
git tag -a "$tag" -m "${tag}"

cat <<EOF

Release commit and tag created locally.

Review with:
  git show HEAD
  git show ${tag}

Then publish with:
  git push
  git push origin ${tag}
  cargo publish
EOF
