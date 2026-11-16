#!/usr/bin/env bash
# Give MailCraft's winit an Android IME text path.
#
# The soft keyboard opened and composed, but no character ever reached the window. The cause is in
# winit, not MailCraft: on Android, GameActivity delivers text the IME committed as
# `InputEvent::TextEvent`, and winit 0.30 had no arm for that variant, so every one fell into the
# `_ => warn!("Unknown android_activity input event")` catch-all. Hardware-key-style events still
# arrive as `InputEvent::KeyEvent`, which is why this looked like a keyboard-specific problem.
#
# The fix translates the event into `WindowEvent::Ime(Ime::Commit)`, which egui inserts text from —
# the owner is winit's Android backend, so that is where it goes.
#
# MailCraft pins winit by git revision in a repository this project does not control, so the crate is
# vendored: a copy carrying the patch, wired in with `[patch]` in the workspace manifest. Idempotent,
# so build-so.sh can run it before every build.
#
# usage: setup-winit-ime.sh <clone-dir>
set -euo pipefail

CLONE="${1:?usage: setup-winit-ime.sh <clone-dir>}"
HERE="$(cd "$(dirname "$0")" && pwd)"
PATCH_FILE="$HERE/0001-android-ime-text.patch"
VENDOR="$CLONE/vendor/winit"
MANIFEST="$CLONE/Cargo.toml"

[ -f "$PATCH_FILE" ] || { echo "FAIL: missing $PATCH_FILE"; exit 2; }
[ -f "$MANIFEST" ] || { echo "FAIL: missing $MANIFEST"; exit 2; }

# The revision mailcraft's manifest pins, and the URL cargo clones it from.
REV="$(sed -nE 's|.*winit *= *\{ *git *= *"([^"]+)".*rev *= *"([0-9a-f]{7,40})".*|\2|p' "$MANIFEST" | head -1)"
URL="$(sed -nE 's|.*winit *= *\{ *git *= *"([^"]+)".*|\1|p' "$MANIFEST" | head -1)"
[ -n "$REV" ] || { echo "setup-winit-ime: winit is not a git dependency, nothing to do"; exit 0; }
[ -n "$URL" ] || { echo "FAIL: could not read the winit git url"; exit 2; }

if [ ! -d "$VENDOR" ]; then
  echo "setup-winit-ime: vendoring winit $REV from $URL"
  # Prefer the copy cargo already has: cloning again needs network, and the checkout is identical.
  SRC=""
  for d in "$HOME"/.cargo/git/checkouts/winit-*/*; do
    [ -d "$d" ] || continue
    [ "$(basename "$d")" = "$REV" ] && { SRC="$d"; break; }
  done
  if [ -z "$SRC" ]; then
    tmp="$(mktemp -d)"
    git clone --quiet "$URL" "$tmp/winit"
    git -C "$tmp/winit" checkout --quiet "$REV"
    SRC="$tmp/winit"
  fi
  mkdir -p "$CLONE/vendor"
  # Only the crate's own sources; cargo's checkout carries a .git cargo re-points at its own copy of,
  # which would confuse a vendored build.
  rsync -a --exclude '.git' --exclude 'target' "$SRC/" "$VENDOR/"
  echo "setup-winit-ime: copied to $VENDOR"
fi

if grep -q "Ime::Commit" "$VENDOR/src/platform_impl/android/mod.rs"; then
  echo "setup-winit-ime: IME text path already present"
else
  echo "setup-winit-ime: applying the IME text patch"
  patch -p1 -d "$VENDOR" --forward --silent < "$PATCH_FILE" \
    || { echo "FAIL: patch did not apply -- has the pinned winit changed?"; exit 2; }
fi

# The crate is not on crates.io, so it is patched by its git url, not by `[patch.crates-io]`.
if grep -q "^winit = { path = \"vendor/winit\" }" "$MANIFEST"; then
  echo "setup-winit-ime: manifest already points at the vendored copy"
else
  echo "setup-winit-ime: wiring vendor/winit into $MANIFEST"
  sed -i.bak -E 's|^winit *= *\{ *git *= *"[^"]+".*$|winit = { path = "vendor/winit" }|' "$MANIFEST"
  rm -f "$MANIFEST.bak"
fi

grep -q '^winit = { path = "vendor/winit" }' "$MANIFEST" \
  || { echo "FAIL: winit dependency was not rewritten"; exit 2; }
echo "setup-winit-ime: done"
