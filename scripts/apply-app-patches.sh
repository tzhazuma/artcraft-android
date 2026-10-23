#!/usr/bin/env bash
# Apply this port's edits to the upstream app clones.
#
# src/soundcraft and src/cadcraft are separate upstream repositories, so this parent repo can
# only record a commit pointer for them -- the menu-bar fix would be lost on a fresh clone. The
# diffs live in patches/ and are applied here, idempotently, before any build.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
for app in soundcraft cadcraft; do
  clone="$root/src/$app"
  patch_file="$root/patches/$app-menu-bar.patch"
  [ -d "$clone/.git" ] || { echo "skip $app: not a clone"; continue; }
  [ -f "$patch_file" ] || { echo "skip $app: no patch"; continue; }
  if git -C "$clone" apply --check "$patch_file" 2>/dev/null; then
    git -C "$clone" apply "$patch_file"
    echo "applied $app-menu-bar.patch"
  elif git -C "$clone" apply --reverse --check "$patch_file" 2>/dev/null; then
    echo "$app already patched"
  else
    echo "WARNING: $app patch neither applies nor is applied" >&2
  fi
done
