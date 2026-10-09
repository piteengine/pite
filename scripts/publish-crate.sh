#!/usr/bin/env bash
# Idempotent single-crate publish for the crates.io workflow.
# Usage: publish-crate.sh <crate> [--wait-for <dep-crate>]
# Skips when this workspace version is already live; optionally waits for
# a dependency crate to reach the index first (pite-scene → pite-core).
set -euo pipefail

crate=$1
ver=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
api="https://crates.io/api/v1/crates"
ua="pite-publish-workflow"

live() { # $1 = crate → true when $ver is on the index
  [ "$(curl -s -A "$ua" -o /dev/null -w '%{http_code}' "$api/$1/$ver")" = 200 ]
}

if live "$crate"; then
  echo "$crate $ver already live — skipping"
  exit 0
fi

if [ "${2:-}" = "--wait-for" ]; then
  dep=$3
  for i in $(seq 1 30); do
    live "$dep" && break
    echo "$dep $ver not in index yet (attempt $i/30) — waiting 60s"
    sleep 60
  done
  live "$dep" || { echo "$dep $ver never appeared in the index"; exit 1; }
fi

cargo publish -p "$crate"
