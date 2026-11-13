#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Package starter templates from examples/ into crates/pite-project/templates/*.zip."""

import io
import os
import sys
import zipfile
from pathlib import Path

TEMPLATES = ["minimal-2d", "gem-collector"]


def find_repo_root() -> Path:
    current = Path(__file__).resolve().parent
    while current != current.parent:
        if (current / "Cargo.toml").is_file() and (current / "examples").is_dir():
            return current
        current = current.parent
    raise FileNotFoundError("Could not find repository root containing Cargo.toml and examples/")


def package_templates(check_only: bool = False) -> bool:
    repo_root = find_repo_root()
    templates_dir = repo_root / "crates" / "pite-project" / "templates"
    templates_dir.mkdir(parents=True, exist_ok=True)

    all_up_to_date = True

    for name in TEMPLATES:
        example_dir = repo_root / "examples" / name
        if not example_dir.is_dir():
            print(f"Warning: example directory not found: {example_dir}", file=sys.stderr)
            continue

        zip_path = templates_dir / f"{name}.zip"

        file_list = []
        for root, dirs, files in os.walk(example_dir):
            dirs[:] = sorted(d for d in dirs if not d.startswith(".") and d != "_cache" and d != "dist")
            for f in sorted(files):
                if not f.startswith("."):
                    full_path = Path(root) / f
                    rel_path = full_path.relative_to(example_dir).as_posix()
                    file_list.append((rel_path, full_path))

        file_list.sort(key=lambda x: x[0])

        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_DEFLATED) as zf:
            for rel, full in file_list:
                with open(full, "rb") as f:
                    data = f.read()
                info = zipfile.ZipInfo(filename=rel, date_time=(2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o644 << 16
                zf.writestr(info, data)

        new_bytes = buf.getvalue()

        existing_bytes = None
        if zip_path.is_file():
            existing_bytes = zip_path.read_bytes()

        if existing_bytes != new_bytes:
            all_up_to_date = False
            if check_only:
                print(f"Template {name}.zip is out of date", file=sys.stderr)
            else:
                zip_path.write_bytes(new_bytes)
                print(f"Packaged {zip_path.relative_to(repo_root)} ({len(new_bytes)} bytes)")
        else:
            if not check_only:
                print(f"Template {zip_path.relative_to(repo_root)} is up to date")

    return all_up_to_date


def main() -> None:
    check_only = "--check" in sys.argv
    up_to_date = package_templates(check_only=check_only)
    if check_only and not up_to_date:
        sys.exit(1)


if __name__ == "__main__":
    main()
