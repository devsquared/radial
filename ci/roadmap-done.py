#!/usr/bin/env python3
"""Move a numbered ROADMAP.md section into Done and renumber what's left.

The mechanical part of retiring a roadmap item -- cutting the section,
renumbering everything after it, and appending a Done bullet -- is
tedious and error-prone by hand. Deciding a section is actually done,
and writing the one-line summary for it, stays a human/agent call:
this script only does the cut-paste-renumber.

Usage:
  ci/roadmap-done.py <section-number> "<Name>" "<summary of what shipped>"
"""

import re
import sys
from pathlib import Path

SECTION_RE = re.compile(r"^## (\d+)\. (.+)$")
DONE_HEADING = "## Done"


def fail(msg: str) -> None:
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(1)


def main() -> None:
    if len(sys.argv) != 4:
        fail('usage: roadmap-done.py <section-number> "<Name>" "<summary>"')

    try:
        target = int(sys.argv[1])
    except ValueError:
        fail(f"section number must be an integer, got {sys.argv[1]!r}")

    name = sys.argv[2].strip()
    summary = sys.argv[3].strip()
    if not name or not summary:
        fail("name and summary must not be empty")
    bullet_text = f"**{name}** — {summary}"

    roadmap_path = Path(__file__).resolve().parent.parent / "ROADMAP.md"
    lines = roadmap_path.read_text().splitlines(keepends=True)

    section_starts = [
        (i, int(m.group(1))) for i, line in enumerate(lines) if (m := SECTION_RE.match(line))
    ]
    if not any(n == target for _, n in section_starts):
        fail(f"no '## {target}. ...' section found in ROADMAP.md")

    done_idx = next((i for i, line in enumerate(lines) if line.startswith(DONE_HEADING)), None)
    if done_idx is None:
        fail(f"no '{DONE_HEADING}' heading found in ROADMAP.md")

    # Section body runs from its heading to the next `## ` heading (exclusive).
    target_start = next(i for i, n in section_starts if n == target)
    next_heading_idx = next(
        (i for i in range(target_start + 1, len(lines)) if lines[i].startswith("## ")),
        len(lines),
    )

    del lines[target_start:next_heading_idx]

    # Renumber sections after the removed one, both cut ranges shift the file.
    removed_span = next_heading_idx - target_start
    for i, line in enumerate(lines):
        m = SECTION_RE.match(line)
        if m and int(m.group(1)) > target:
            lines[i] = f"## {int(m.group(1)) - 1}. {m.group(2)}\n"

    # Recompute Done's position after the deletion.
    if done_idx >= target_start:
        done_idx -= removed_span
    insertion_idx = next(
        (i for i in range(done_idx + 1, len(lines)) if lines[i].startswith("## ")),
        len(lines),
    )
    while insertion_idx > done_idx + 1 and lines[insertion_idx - 1].strip() == "":
        insertion_idx -= 1

    lines.insert(insertion_idx, f"- {bullet_text}\n")

    roadmap_path.write_text("".join(lines))
    print(f"moved section {target} into Done and renumbered the rest")


if __name__ == "__main__":
    main()
