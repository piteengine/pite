#!/bin/sh
# Install Pite (engine + player binaries for all platforms) from a release bundle, verified.
# Usage: ./install.sh [tag]   (tag defaults to the latest release)
# Env: PREFIX (default $HOME/.local), PITE_VERSION (overrides the tag arg).
set -e

command -v curl >/dev/null 2>&1 || { echo "error: curl is required to install Pite" >&2; exit 1; }
command -v tar >/dev/null 2>&1 || { echo "error: tar is required to install Pite" >&2; exit 1; }

REPO="piteengine/pite"
TAG="${PITE_VERSION:-${1:-}}"
if [ -z "$TAG" ]; then
  LATEST_JSON="$(curl -fsSL -H "User-Agent: pite-installer" "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null || true)"
  TAG="$(echo "$LATEST_JSON" | sed -n 's/^ *"tag_name": "\(.*\)",$/\1/p')"
  if [ -z "$TAG" ]; then
    if echo "$LATEST_JSON" | grep -qi "rate limit"; then
      echo "error: GitHub API rate limit reached. Specify the release version manually: ./install.sh vX.Y.Z" >&2
    else
      echo "error: cannot resolve the latest release from GitHub (pass a release tag explicitly: ./install.sh vX.Y.Z)" >&2
    fi
    exit 1
  fi
fi

OS="$(uname -s)"
case "$OS" in
  Linux*)  PLATFORM="linux";;
  Darwin*) PLATFORM="macos";;
  *) echo "error: unsupported operating system $OS" >&2; exit 1;;
esac

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) ;;
  arm64|aarch64)
    if [ "$PLATFORM" = "linux" ]; then
      echo "error: prebuilt binaries are only provided for x86_64 on Linux (detected $ARCH)" >&2
      exit 1
    fi
    ;;
  *)
    echo "error: unsupported architecture $ARCH" >&2
    exit 1
    ;;
esac

PREFIX="${PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"
mkdir -p "$BINDIR" || {
  echo "error: failed to create target directory $BINDIR (check permissions or set PREFIX)" >&2
  exit 1
}
if [ ! -w "$BINDIR" ]; then
  echo "error: target directory $BINDIR is not writable (check permissions or set PREFIX)" >&2
  exit 1
fi

BUNDLE="pite-$TAG-$PLATFORM.tar.gz"
BASE="https://github.com/$REPO/releases/download/$TAG"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT INT TERM
cd "$work"

if [ -t 2 ]; then
  CURL_PROGRESS="--progress-bar"
else
  CURL_PROGRESS="-s"
fi

echo "info: downloading Pite $TAG bundle for $PLATFORM..."
curl -fLS $CURL_PROGRESS -o "$BUNDLE" "$BASE/$BUNDLE"
curl -fsSL -H "User-Agent: pite-installer" -o "SHA256SUMS" "$BASE/SHA256SUMS"

VERIFIED=0
if command -v sha256sum >/dev/null 2>&1; then
  if grep -F " $BUNDLE" SHA256SUMS | sha256sum -c - >/dev/null 2>&1; then
    VERIFIED=1
  fi
elif command -v shasum >/dev/null 2>&1; then
  if grep -F " $BUNDLE" SHA256SUMS | shasum -a 256 -c - >/dev/null 2>&1; then
    VERIFIED=1
  fi
else
  echo "error: neither sha256sum nor shasum found" >&2
  exit 1
fi

if [ "$VERIFIED" -ne 1 ]; then
  echo "error: checksum verification failed for $BUNDLE" >&2
  exit 1
fi

tar -xzf "$BUNDLE" -C "$BINDIR"
chmod +x "$BINDIR/pite" "$BINDIR"/pite-player*

echo "installed $TAG to $BINDIR"
case ":$PATH:" in
  *":$BINDIR:"*) ;;
  *) echo "note: $BINDIR is not on PATH; add: export PATH=\"$BINDIR:\$PATH\" (e.g. in ~/.bashrc or ~/.zshrc)" ;;
esac
