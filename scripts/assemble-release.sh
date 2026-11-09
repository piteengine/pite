#!/usr/bin/env bash
# Assemble the engine + player release bundles from the artifacts CI downloaded.
# Usage: assemble-release.sh <tag> [dist-dir] [work-dir]
#
# Expects <dist-dir>/{dist-linux,dist-windows,dist-macos}/ with the raw staged
# binaries from the build matrix (pite-<platform>, pite-player-<platform>).
# Writes pite-<tag>-<platform>.* and pite-player-<tag>-*.* into <dist-dir>.
set -euo pipefail

tag=${1:?usage: assemble-release.sh <tag> [dist-dir] [work-dir]}
dist=${2:-dist}
work=${3:-bundle}

mkdir -p "$work"/{linux,windows,macos,player-linux,player-windows,player-macos,player-allplatforms}

# --- Engine bundles (engine + player binaries for every platform) ---
cp "$dist/dist-linux/pite-linux"                 "$work/linux/pite"
cp "$dist/dist-linux/pite-player-linux"          "$work/linux/pite-player-linux"
cp "$dist/dist-windows/pite-player-windows.exe"  "$work/linux/pite-player-windows.exe"
cp "$dist/dist-macos/pite-player-macos"          "$work/linux/pite-player-macos"

cp "$dist/dist-windows/pite-windows.exe"         "$work/windows/pite.exe"
cp "$dist/dist-windows/pite-player-windows.exe"  "$work/windows/pite-player-windows.exe"
cp "$dist/dist-linux/pite-player-linux"          "$work/windows/pite-player-linux"
cp "$dist/dist-macos/pite-player-macos"          "$work/windows/pite-player-macos"

cp "$dist/dist-macos/pite-macos"                 "$work/macos/pite"
cp "$dist/dist-macos/pite-player-macos"          "$work/macos/pite-player-macos"
cp "$dist/dist-windows/pite-player-windows.exe"  "$work/macos/pite-player-windows.exe"
cp "$dist/dist-linux/pite-player-linux"          "$work/macos/pite-player-linux"

tar -czf "$dist/pite-$tag-linux.tar.gz" -C "$work/linux" pite pite-player-linux pite-player-windows.exe pite-player-macos
zip -j "$dist/pite-$tag-windows.zip" \
  "$work/windows/pite.exe" \
  "$work/windows/pite-player-windows.exe" \
  "$work/windows/pite-player-linux" \
  "$work/windows/pite-player-macos"
tar -czf "$dist/pite-$tag-macos.tar.gz" -C "$work/macos" pite pite-player-macos pite-player-windows.exe pite-player-linux

# --- Standalone player bundles ---
cp "$dist/dist-linux/pite-player-linux"          "$work/player-linux/pite-player-linux"
chmod +x "$work/player-linux/pite-player-linux"
tar -czf "$dist/pite-player-$tag-linux.tar.gz" -C "$work/player-linux" pite-player-linux

cp "$dist/dist-windows/pite-player-windows.exe"  "$work/player-windows/pite-player-windows.exe"
zip -j "$dist/pite-player-$tag-windows.zip" "$work/player-windows/pite-player-windows.exe"

cp "$dist/dist-macos/pite-player-macos"          "$work/player-macos/pite-player-macos"
chmod +x "$work/player-macos/pite-player-macos"
tar -czf "$dist/pite-player-$tag-macos.tar.gz" -C "$work/player-macos" pite-player-macos

# --- All-platforms player bundles ---
cp "$dist/dist-linux/pite-player-linux"          "$work/player-allplatforms/pite-player-linux"
cp "$dist/dist-windows/pite-player-windows.exe"  "$work/player-allplatforms/pite-player-windows.exe"
cp "$dist/dist-macos/pite-player-macos"          "$work/player-allplatforms/pite-player-macos"
chmod +x "$work/player-allplatforms/pite-player-linux" "$work/player-allplatforms/pite-player-macos"

tar -czf "$dist/pite-player-$tag-allplatforms.tar.gz" -C "$work/player-allplatforms" pite-player-linux pite-player-windows.exe pite-player-macos
zip -j "$dist/pite-player-$tag-allplatforms.zip" \
  "$work/player-allplatforms/pite-player-linux" \
  "$work/player-allplatforms/pite-player-windows.exe" \
  "$work/player-allplatforms/pite-player-macos"
