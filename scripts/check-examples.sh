#!/usr/bin/env bash
# Run `pite check` inside every example project.
#
# Each `examples/*/` directory that carries a `pite.toml` is checked from
# within its own folder, exactly like a user would: no `--path` is passed,
# so the project root is discovered from the current directory.
# Every example runs; the script exits non-zero if any check fails.
# CI invokes this (see `.github/workflows/ci.yml`); safe to run locally.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

status=0
for manifest in "$repo_root"/examples/*/pite.toml; do
    [ -f "$manifest" ] || continue
    example_dir="$(dirname "$manifest")"
    name="$(basename "$example_dir")"
    echo "==> pite check: $name"
    if ! ( cd "$example_dir" && cargo run --locked -p pite -- check ); then
        status=1
    fi
done

exit "$status"
