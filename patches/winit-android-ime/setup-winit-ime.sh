#!/usr/bin/env bash
# Give a port's winit an Android IME text path.
#
# The soft keyboard opened, composed, and typed nothing. The cause is in winit, not in any one app:
# Android's GameActivity delivers IME-committed text as `InputEvent::TextEvent`, and this winit had no
# arm for that variant, so every event fell into the `_ => warn!("Unknown android_activity input
# event")` catch-all. Hardware-key-style events still arrive as `InputEvent::KeyEvent`, which is why
# the defect hid behind "some keyboards work".
#
# Applications pin winit by git revision in a repository this project does not control, so the crate is
# vendored: one patched copy, `patches/winit-android-ime/winit`, copied into each clone and wired in
# with a path dependency. Idempotent; build-so.sh runs it before every build.
#
# usage: setup.sh <clone-dir>
set -euo pipefail

CLONE="${1:?usage: setup.sh <clone-dir>}"
HERE="$(cd "$(dirname "$0")" && pwd)"
PATCH_FILE="$HERE/0001-android-ime-text.patch"
VENDOR="$CLONE/vendor/winit"
MANIFEST="$CLONE/Cargo.toml"

[ -f "$PATCH_FILE" ] || { echo "FAIL: missing $PATCH_FILE"; exit 2; }
[ -f "$MANIFEST" ] || { echo "FAIL: missing $MANIFEST"; exit 2; }

# Already wired to the vendored copy? Then only refresh the tree.
if grep -q '^winit = { path = "vendor/winit" }' "$MANIFEST"; then
  # Already wired: only make sure the tree still carries the patch.
  if [ -d "$VENDOR" ] && grep -q "InputEvent::TextEvent" "$VENDOR/src/platform_impl/android/mod.rs" 2>/dev/null; then
    echo "setup-winit-ime: already in place"
    exit 0
  fi
fi

# The pin may be a git dependency (the usual case) or a crates.io version (a port that overrides it).
if grep -qE '^winit *= *\{ *git *=' "$MANIFEST"; then
  :
elif grep -qE '^winit *= *\{ *version *=' "$MANIFEST"; then
  :
else
  echo "setup-winit-ime: no winit dependency in the workspace manifest, nothing to do"
  exit 0
fi

# Take the crate from cargo's own checkout when it is there -- it is byte-identical to a fresh
# clone and needs no network. Cloning again is the fallback for a machine that has never built.
WINIT_LINE="$(grep -E '^winit *=' "$MANIFEST" | head -1)"
REV="$(printf '%s' "$WINIT_LINE" | grep -oE 'rev *= *"[0-9a-f]+"' | grep -oE '[0-9a-f]{7,40}')"
URL="$(printf '%s' "$WINIT_LINE" | grep -oE 'git *= *"[^"]+"' | sed -E 's|git *= *"([^"]+)"|\1|')"
SRC=""
if [ -n "${REV:-}" ]; then
  for d in "$HOME"/.cargo/git/checkouts/winit-*/*; do
    [ -d "$d" ] || continue
    [ "$(basename "$d")" = "$REV" ] && { SRC="$d"; break; }
  done
fi
if [ -z "$SRC" ] && [ -n "${URL:-}" ] && [ -n "${REV:-}" ]; then
  tmp="$(mktemp -d)"
  git clone --quiet "$URL" "$tmp/winit" && git -C "$tmp/winit" checkout --quiet "$REV" && SRC="$tmp/winit"
fi
[ -n "$SRC" ] || { echo "setup-winit-ime: winit is not a git dependency and no checkout was found"; exit 0; }

mkdir -p "$CLONE/vendor"
rsync -a --delete --exclude '.git' --exclude 'target' "$SRC/" "$VENDOR/"
patch -p1 -d "$VENDOR" --forward --silent < "$PATCH_FILE" \
  || { grep -q "InputEvent::TextEvent" "$VENDOR/src/platform_impl/android/mod.rs" \
       || { echo "FAIL: IME patch did not apply"; exit 2; }; }
echo "setup-winit-ime: vendored the patched winit into $VENDOR"

sed -i.bak -E 's|^winit *= *\{[^}]*\}|winit = { path = "vendor/winit" }|' "$MANIFEST"
rm -f "$MANIFEST.bak"
grep -q '^winit = { path = "vendor/winit" }' "$MANIFEST" \
  || { echo "FAIL: winit dependency was not rewritten in $MANIFEST"; exit 2; }
echo "setup-winit-ime: manifest now points at the vendored copy"
