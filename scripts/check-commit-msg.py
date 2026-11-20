#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Validate a commit message against the Pite commit convention.

The subject must start with one of the project's Conventional Commit prefixes,
an optional lowercase scope, and a lowercase summary:

    refactor(editor): split app.rs into dedicated submodules

Merge, revert, and fixup/squash commits are exempt.

Usage:
    python3 scripts/check-commit-msg.py .git/COMMIT_EDITMSG
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

PREFIXES = (
    "feat",
    "fix",
    "docs",
    "style",
    "refactor",
    "perf",
    "test",
    "chore",
    "dep",
    "a11y",
)

SUBJECT_PATTERN = re.compile(
    rf"^(?:{'|'.join(PREFIXES)})"
    r"(?:\((?P<scope>[a-z0-9][a-z0-9._/-]*)\))?"
    r"!?: "
    r"(?P<summary>\S.*)$"
)

EXEMPT_PREFIXES = ("Merge ", "Revert ", "fixup! ", "squash! ")

EXAMPLE = "refactor(editor): split app.rs into dedicated submodules"


def first_content_line(message: str) -> tuple[int, str]:
    """Return the index and text of the first line that is not a comment."""
    for index, line in enumerate(message.splitlines()):
        if line.strip() and not line.startswith("#"):
            return index, line.rstrip()
    return -1, ""


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: check-commit-msg.py <commit-message-file>", file=sys.stderr)
        return 2

    message = Path(argv[1]).read_text(encoding="utf-8", errors="replace")
    index, subject = first_content_line(message)
    if not subject:
        print("commit-msg: the commit message is empty.", file=sys.stderr)
        return 1

    if subject.startswith(EXEMPT_PREFIXES):
        return 0

    match = SUBJECT_PATTERN.match(subject)
    if match is None:
        print(
            "commit-msg: the subject does not follow the commit convention.\n"
            f"  got:      {subject}\n"
            "  expected: <type>(<scope>): <summary>, for example\n"
            f"            {EXAMPLE}\n"
            f"  types:    {', '.join(PREFIXES)}",
            file=sys.stderr,
        )
        return 1

    summary = match.group("summary")
    if summary[0].isupper():
        print(
            "commit-msg: start the summary with a lowercase letter.\n"
            f"  got: {subject}",
            file=sys.stderr,
        )
        return 1

    lines = message.splitlines()
    if index + 1 < len(lines) and lines[index + 1].strip():
        print(
            "commit-msg: separate the subject from the body with a blank line.",
            file=sys.stderr,
        )
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
