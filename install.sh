#!/bin/sh
# Install Pite (engine + player binaries for all platforms) from a release bundle, verified.
# Usage: ./install.sh [tag]   (tag defaults to the latest release)
# Env: PREFIX (default $HOME/.local), PITE_VERSION (overrides the tag arg).
set -e

REPO="piteengine/pite"
TAG="${PITE_VERSION:-${1:-}}"
if [ -z "$TAG" ]; then
  TAG="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/^ *"tag_name": "\(.*\)",$/\1/p')"
fi
[ -n "$TAG" ] || { echo "error: cannot resolve the latest release" >&2; exit 1; }

PREFIX="${PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"
OS="$(uname -s)"
case "$OS" in
  Linux*)  PLATFORM="linux";;
  Darwin*) PLATFORM="macos";;
  *) echo "error: unsupported operating system $OS" >&2; exit 1;;
esac
BUNDLE="pite-$TAG-$PLATFORM.tar.gz"
BASE="https://github.com/$REPO/releases/download/$TAG"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT INT TERM
cd "$work"
curl -fsSL -O "$BASE/$BUNDLE"
curl -fsSL -O "$BASE/SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
  grep -F " $BUNDLE" SHA256SUMS | sha256sum -c - >/dev/null
elif command -v shasum >/dev/null 2>&1; then
  grep -F " $BUNDLE" SHA256SUMS | shasum -a 256 -c - >/dev/null
else
  echo "error: neither sha256sum nor shasum found" >&2
  exit 1
fi

mkdir -p "$BINDIR"
tar -xzf "$BUNDLE" -C "$BINDIR"
chmod +x "$BINDIR/pite" "$BINDIR"/pite-player*

echo "installed $TAG to $BINDIR"
case ":$PATH:" in
  *":$BINDIR:"*) ;;
  *) echo "note: $BINDIR is not on PATH; add: export PATH=\"$BINDIR:\$PATH\"" ;;
esac
