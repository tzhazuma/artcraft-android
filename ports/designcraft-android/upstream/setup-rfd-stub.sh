#!/bin/bash
# Make DesignCraft 0.14.0's dependency graph compile for aarch64-linux-android.
#
# Why this exists: DesignCraft 0.14.0 reaches rfd through `craft-dialogs` (craft-libs), and rfd
# 0.17.2 has no Android backend — `cargo ndk` fails inside rfd itself:
#
#   error[E0277]: the trait bound `FileDialog: AsyncFilePickerDialogImpl` is not satisfied
#      --> rfd-0.17.2/src/file_dialog.rs:269
#
# The port's real file picking is the Storage Access Framework (`src/saf.rs`), not rfd. This
# script points `[patch.crates-io] rfd` at `upstream/rfd-android`, a no-op rfd with the same
# public surface, so craft-dialogs compiles and every desktop dialog answers "cancelled".
#
# Idempotent. Run once after the upstream sync and before build-so.sh:
#   ports/designcraft-android/upstream/setup-rfd-stub.sh [clone-dir]
set -euo pipefail

CLONE="${1:-$HOME/artcraft-android/src/designcraft}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# The patch path is relative to the workspace root (the clone's Cargo.toml).
REL="../../ports/$(basename "$(dirname "$HERE")")/upstream/rfd-android"

[ -f "$CLONE/Cargo.toml" ] || { echo "FAIL: no Cargo.toml at $CLONE" >&2; exit 2; }
[ -f "$HERE/rfd-android/Cargo.toml" ] || { echo "FAIL: stub missing at $HERE/rfd-android" >&2; exit 2; }

/usr/bin/python3 - "$CLONE/Cargo.toml" "$REL" <<'PY'
import pathlib, sys
cargo, rel = pathlib.Path(sys.argv[1]), sys.argv[2]
text = cargo.read_text()
needle = "[patch.crates-io]"
line = f'rfd = {{ path = "{rel}" }}'
if line in text:
    print("rfd stub patch already present")
    raise SystemExit(0)
if needle not in text:
    print("FAIL: no [patch.crates-io] table in the workspace root", file=sys.stderr)
    raise SystemExit(2)
cargo.write_text(text.replace(needle, needle + "\n" + line, 1))
print("added rfd stub patch:", line)
PY
