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
ALREADY_WIRED=0
if grep -q '^winit = { path = "vendor/winit" }' "$MANIFEST"; then
  ALREADY_WIRED=1
  # Wired, and the tree still carries the patch: nothing to do.
  if [ -d "$VENDOR" ] && grep -q "PATCH_REV: u32 = 2" "$VENDOR/src/ime_delta.rs" 2>/dev/null; then
    echo "setup-winit-ime: already in place"
    exit 0
  fi
fi

# The pin may be a git dependency (the usual case) or a crates.io version (a port that overrides it).
if [ "$ALREADY_WIRED" = 1 ]; then
  :   # the dependency is already a path; the source just needs restoring and patching
elif grep -qE '^winit *= *\{ *git *=' "$MANIFEST"; then
  :
elif grep -qE '^winit *= *\{ *version *=' "$MANIFEST"; then
  :
else
  echo "setup-winit-ime: no winit dependency in the workspace manifest, nothing to do"
  exit 0
fi

# Take the crate from cargo's own checkout when it is there -- it is byte-identical to a fresh
# clone and needs no network. Cloning again is the fallback for a machine that has never built.
# The revision every craft port pins. Kept here because once the manifest is rewritten to the path
# dependency it no longer carries a rev, and this script has to be re-runnable from that state.
DEFAULT_URL="https://github.com/storytold/winit.git"
DEFAULT_REV="a517b3846ee8457d84ee4975110bfda6797c44dc"

WINIT_LINE="$(grep -E '^winit *=' "$MANIFEST" | head -1)"
REV="$(printf '%s' "$WINIT_LINE" | grep -oE 'rev *= *"[0-9a-f]+"' | grep -oE '[0-9a-f]{7,40}' || true)"
URL="$(printf '%s' "$WINIT_LINE" | grep -oE 'git *= *"[^"]+"' | sed -E 's|git *= *"([^"]+)"|\1|' || true)"
[ -n "${REV:-}" ] || REV="$DEFAULT_REV"
[ -n "${URL:-}" ] || URL="$DEFAULT_URL"
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

if [ "$ALREADY_WIRED" = 1 ]; then
  echo "setup-winit-ime: refreshed $VENDOR"
  exit 0
fi
sed -i.bak -E 's|^winit *= *\{[^}]*\}|winit = { path = "vendor/winit" }|' "$MANIFEST"
rm -f "$MANIFEST.bak"
grep -q '^winit = { path = "vendor/winit" }' "$MANIFEST" \
  || { echo "FAIL: winit dependency was not rewritten in $MANIFEST"; exit 2; }
echo "setup-winit-ime: manifest now points at the vendored copy"
