#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Update zensical.toml navigation to include all node reference pages.

Scans docs/reference/nodes/*.md and synchronizes the { "Nodes" = [ ... ] }
section under Reference in zensical.toml.

Usage:
    python3 scripts/update-nav.py          # Update zensical.toml in-place
    python3 scripts/update-nav.py --check  # Check if zensical.toml is up to date (CI)
"""

import argparse
import re
import sys
from pathlib import Path

NODES_SECTION_PATTERN = re.compile(
    r'([ \t]*)\{\s*"Nodes"\s*=\s*\[[^\]]*\]\s*\},?',
    re.DOTALL
)

REFERENCE_SECTION_PATTERN = re.compile(
    r'(\{\s*"Reference"\s*=\s*\[)(.*?)(\]\s*\},?)',
    re.DOTALL
)


def find_repo_root() -> Path:
    current = Path(__file__).resolve().parent
    while current != current.parent:
        if (current / "zensical.toml").is_file() and (current / "Cargo.toml").is_file():
            return current
        current = current.parent
    raise FileNotFoundError("Could not find repository root containing zensical.toml and Cargo.toml")


def scan_node_files(nodes_dir: Path) -> list[str]:
    if not nodes_dir.is_dir():
        raise FileNotFoundError(f"Nodes directory not found: {nodes_dir}")

    files = [
        p.name
        for p in nodes_dir.glob("*.md")
        if p.name != "index.md"
    ]
    files.sort(key=str.lower)
    return files


def build_nodes_toml_block(node_files: list[str], indent: str = "      ") -> str:
    lines = [f'{indent}{{ "Nodes" = [']
    inner_indent = indent + "    "
    lines.append(f'{inner_indent}"reference/nodes/index.md",')
    for name in node_files:
        lines.append(f'{inner_indent}"reference/nodes/{name}",')
    lines.append(f"{indent}]}},")
    return "\n".join(lines)


def update_zensical_content(content: str, node_files: list[str]) -> str:
    nodes_block = build_nodes_toml_block(node_files)

    if NODES_SECTION_PATTERN.search(content):
        # Replace existing Nodes section
        return NODES_SECTION_PATTERN.sub(nodes_block, content, count=1)

    # Insert into Reference section if not present
    ref_match = REFERENCE_SECTION_PATTERN.search(content)
    if not ref_match:
        raise ValueError("Could not find { \"Reference\" = [ ... ] } section in zensical.toml")

    start, body, end = ref_match.group(1), ref_match.group(2).rstrip(), ref_match.group(3)
    updated_body = f"{body}\n{nodes_block}\n  "
    return content[:ref_match.start()] + start + updated_body + end + content[ref_match.end():]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Verify whether zensical.toml is up-to-date without modifying it.",
    )
    args = parser.parse_args()

    root = find_repo_root()
    zensical_path = root / "zensical.toml"
    nodes_dir = root / "docs" / "reference" / "nodes"

    node_files = scan_node_files(nodes_dir)
    original_text = zensical_path.read_text(encoding="utf-8")
    updated_text = update_zensical_content(original_text, node_files)

    if args.check:
        if original_text != updated_text:
            print(
                "Error: zensical.toml navigation is out of date with docs/reference/nodes/*.md.",
                file=sys.stderr,
            )
            print("Run 'python3 scripts/update-nav.py' to update it.", file=sys.stderr)
            return 1
        print("zensical.toml navigation is up-to-date.")
        return 0

    if original_text != updated_text:
        zensical_path.write_text(updated_text, encoding="utf-8")
        print(f"Updated {zensical_path.relative_to(root)} with {len(node_files)} node references.")
    else:
        print(f"{zensical_path.relative_to(root)} is already up-to-date.")

    return 0


if __name__ == "__main__":
    sys.exit(main())
