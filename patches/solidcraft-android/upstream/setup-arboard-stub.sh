#!/bin/bash
# Make SolidCraft's dependency graph compile for aarch64-linux-android.
#
# Why this exists: `solidcraft-ui-egui` declares `arboard` (the system clipboard) under
# `cfg(not(target_arch = "wasm32"))`, which is true on Android, and arboard 3.6 has no Android
# backend at all — its `src/platform/mod.rs` selects a module for Windows, macOS and the
# X11/Wayland desktops only, so the crate fails to build with
#
#   error[E0425]: cannot find function `get` in module `platform`
#
# The port has no cross-app clipboard either way (egui's own in-app clipboard is separate and
# unaffected). This script points `[patch.crates-io] arboard` at `upstream/arboard-android`, a
# shim with the same public surface that always reports "nothing on the clipboard", so the UI crate
# compiles and every clipboard read answers honestly instead of the build stopping.
#
# Idempotent. Run once after the upstream sync and before build-so.sh:
#   patches/solidcraft-android/upstream/setup-arboard-stub.sh [clone-dir]
set -euo pipefail

CLONE="${1:-$HOME/artcraft-android/src/solidcraft}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# The patch path is relative to the workspace root (the clone's Cargo.toml).
REL="../../patches/$(basename "$(dirname "$HERE")")/upstream/arboard-android"

[ -f "$CLONE/Cargo.toml" ] || { echo "FAIL: no Cargo.toml at $CLONE" >&2; exit 2; }
[ -f "$HERE/arboard-android/Cargo.toml" ] || { echo "FAIL: stub missing at $HERE/arboard-android" >&2; exit 2; }

/usr/bin/python3 - "$CLONE/Cargo.toml" "$REL" <<'PY'
import pathlib, sys
cargo, rel = pathlib.Path(sys.argv[1]), sys.argv[2]
text = cargo.read_text()
needle = "[patch.crates-io]"
line = f'arboard = {{ path = "{rel}" }}'
if line in text:
    print("arboard stub patch already present")
    raise SystemExit(0)
if needle not in text:
    print("FAIL: no [patch.crates-io] table in the workspace root", file=sys.stderr)
    raise SystemExit(2)
cargo.write_text(text.replace(needle, needle + "\n" + line, 1))
print("added arboard stub patch:", line)
PY
