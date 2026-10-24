#!/usr/bin/env bash
# Report which ArtCraft app clones are behind their upstream.
#
# The ports carry an Android overlay per app, so a sync is not a plain pull: this script only
# reports. It is meant to run on a schedule, so it stays quiet unless something moved.
set -uo pipefail

ROOT="${ARTCRAFT_ROOT:-$HOME/artcraft-android}"
cd "$ROOT/src" || exit 1

changed=0
for d in */; do
  d=${d%/}
  [ -d "$d/.git" ] || continue
  git -C "$d" fetch -q origin 2>/dev/null || continue
  behind=$(git -C "$d" rev-list --count HEAD..origin/main 2>/dev/null || echo 0)
  if [ "${behind:-0}" -gt 0 ]; then
    changed=1
    printf '%-16s %5s commits behind (%s -> %s)\n' \
      "$d" "$behind" "$(git -C "$d" rev-parse --short HEAD)" \
      "$(git -C "$d" rev-parse --short origin/main)"
  fi
done

if [ "$changed" -eq 0 ]; then
  exit 0
fi
exit 10
