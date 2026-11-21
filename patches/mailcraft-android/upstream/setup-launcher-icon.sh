#!/usr/bin/env bash
# Give upstream's Android app the MailCraft launcher icon.
#
# 0.13.0's manifest carries no android:icon, so the APK ships `icon=''`: the app installs, but every
# launcher -- ours included -- falls back to the generic Android placeholder. Upstream builds this app
# for their CI and has never shipped it, so nothing surfaced the omission.
#
# The icon itself is not invented here: it is the MailCraft icon the egui port has used since 0.9.0,
# including the adaptive-icon variant, so the app looks the same in the suite as it always did.
# Idempotent, and it never overwrites a file upstream owns.
set -euo pipefail

R="${ARTCRAFT_HOME:-$HOME/artcraft-android}"
SRC_RES="$R/ports/mailcraft-android/android/app/src/main/res"
APP="$R/src/mailcraft/apps/mailcraft-android/app/src/main"
DST_RES="$APP/res"
MANIFEST="$APP/AndroidManifest.xml"

[ -d "$SRC_RES" ] || { echo "FAIL: no icon sources at $SRC_RES"; exit 2; }
[ -f "$MANIFEST" ] || { echo "FAIL: no manifest at $MANIFEST"; exit 2; }

# Raster mipmaps for API < 26 plus the adaptive-icon XML for 26+.
for d in "$SRC_RES"/mipmap-*; do
  [ -d "$d" ] || continue
  mkdir -p "$DST_RES/$(basename "$d")"
  rsync -a --ignore-existing "$d/" "$DST_RES/$(basename "$d")/"
done
for d in "$SRC_RES"/drawable-*; do
  [ -d "$d" ] || continue
  # Only the icon foreground; upstream has its own drawables and must keep them.
  mkdir -p "$DST_RES/$(basename "$d")"
  for f in "$d"/ic_launcher_foreground.*; do
    [ -f "$f" ] || continue
    [ -e "$DST_RES/$(basename "$d")/$(basename "$f")" ] || cp "$f" "$DST_RES/$(basename "$d")/"
  done
done
if [ -f "$SRC_RES/values/ic_launcher_background.xml" ] && [ ! -f "$DST_RES/values/ic_launcher_background.xml" ]; then
  cp "$SRC_RES/values/ic_launcher_background.xml" "$DST_RES/values/ic_launcher_background.xml"
fi

if ! grep -q 'android:icon=' "$MANIFEST"; then
  /usr/bin/python3 - "$MANIFEST" <<'PYEOF'
import sys, pathlib
p = pathlib.Path(sys.argv[1]); s = p.read_text(encoding="utf-8")
needle = '        android:label="@string/app_name"'
assert needle in s, "manifest has no android:label line to anchor the icon on"
s = s.replace(needle,
              '        android:icon="@mipmap/ic_launcher"\n'
              '        android:roundIcon="@mipmap/ic_launcher_round"\n' + needle, 1)
p.write_text(s, encoding="utf-8")
PYEOF
fi
grep -q 'android:icon="@mipmap/ic_launcher"' "$MANIFEST" \
  || { echo "FAIL: icon not wired into the manifest"; exit 3; }
echo "setup-launcher-icon: MailCraft icon in place"
