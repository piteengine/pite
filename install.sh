#!/bin/sh
# Install Pite (engine + both players) from a release bundle, verified.
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
BUNDLE="pite-$TAG-linux.tar.gz"
BASE="https://github.com/$REPO/releases/download/$TAG"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT INT TERM
cd "$work"
curl -fsSL -O "$BASE/$BUNDLE"
curl -fsSL -O "$BASE/SHA256SUMS"
grep -F " $BUNDLE" SHA256SUMS | sha256sum -c - >/dev/null

mkdir -p "$BINDIR"
tar -xzf "$BUNDLE" -C "$BINDIR" pite pite-player pite-player.exe
chmod +x "$BINDIR/pite" "$BINDIR/pite-player"

echo "installed $TAG to $BINDIR (pite, pite-player, pite-player.exe)"
case ":$PATH:" in
  *":$BINDIR:"*) ;;
  *) echo "note: $BINDIR is not on PATH; add: export PATH=\"$BINDIR:\$PATH\"" ;;
esac
