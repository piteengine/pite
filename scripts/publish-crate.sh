#!/usr/bin/env bash
# Idempotent single-crate publish for the crates.io workflow.
# Usage: publish-crate.sh <crate> [--wait-for <dep-crate>]...
# Skips when this workspace version is already live; optionally waits for
# dependency crates to reach the index first.
set -euo pipefail

crate=$1
shift
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

while [ $# -gt 0 ]; do
  case "$1" in
    --wait-for)
      dep="$2"
      shift 2
      for i in $(seq 1 30); do
        live "$dep" && break
        echo "$dep $ver not in index yet (attempt $i/30) — waiting 60s"
        sleep 60
      done
      live "$dep" || { echo "$dep $ver never appeared in the index"; exit 1; }
      ;;
    *)
      echo "unknown argument: $1"
      exit 1
      ;;
  esac
done

for attempt in $(seq 1 6); do
  set +e
  out=$(cargo publish -p "$crate" 2>&1)
  status=$?
  set -e
  echo "$out"
  if [ $status -eq 0 ]; then
    break
  fi
  if echo "$out" | grep -qi "429 Too Many Requests\|rate limit"; then
    echo "$crate hit crates.io rate limit (attempt $attempt/6) — waiting 5m to retry"
    sleep 300
  else
    exit $status
  fi
done
