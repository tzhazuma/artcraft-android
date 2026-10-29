#!/usr/bin/env bash
# Report ArtCraft apps whose *version* moved upstream, not merely their commit count.
#
# The daily commit-count check was the wrong signal: this ecosystem lands hundreds of commits a day,
# so every run reported "behind" and the sync could never finish. What actually matters is whether
# upstream published a new version -- that is what changes the APK the user downloads.
#
# Compares the `[workspace.package] version` (or `[package] version`) in the clone's Cargo.toml
# against the same field at origin/main. Exits 10 when any version moved, 0 when none did.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
moved=0

read_version() {   # $1 = directory
  awk '
    /^\[workspace\.package\]/ {ws=1; next}
    /^\[package\]/            {ws=1; next}
    /^\[/                     {ws=0}
    ws && /^version[[:space:]]*=/ {
      gsub(/.*=[[:space:]]*"/, ""); gsub(/".*/, ""); print; exit
    }
  ' "$1/Cargo.toml" 2>/dev/null
}

for dir in "$ROOT"/src/*/; do
  app="$(basename "$dir")"
  [ -f "$dir/Cargo.toml" ] || continue
  [ -d "$dir/.git" ] || continue
  # Only directories that have a matching Android port. `src/` also holds vendored crates such as
  # winit-storytold, which have no APK and would otherwise be reported as an app that moved.
  [ -d "$ROOT/ports/$app-android" ] || continue

  local_ver="$(read_version "$dir")"
  git -C "$dir" fetch -q origin 2>/dev/null || true
  remote_ver="$(git -C "$dir" show origin/main:Cargo.toml 2>/dev/null | awk '
    /^\[workspace\.package\]/ {ws=1; next}
    /^\[package\]/            {ws=1; next}
    /^\[/                     {ws=0}
    ws && /^version[[:space:]]*=/ {
      gsub(/.*=[[:space:]]*"/, ""); gsub(/".*/, ""); print; exit
    }')"

  [ -z "$local_ver" ] && continue
  if [ "$local_ver" != "$remote_ver" ]; then
    behind="$(git -C "$dir" rev-list --count HEAD..origin/main 2>/dev/null || echo '?')"
    printf '%-14s %s -> %s  (%s commits)\n' "$app" "$local_ver" "$remote_ver" "$behind"
    moved=1
  fi
done

[ "$moved" = 1 ] && exit 10
exit 0
