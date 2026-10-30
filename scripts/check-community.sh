#!/usr/bin/env bash
# Report community apps the ArtCraft Launcher knows about but this port set does not cover.
#
# The launcher's catalog is the authority on what the suite contains. Official apps live under
# `storytold/*` and the port pipeline grew up around them; community apps come from other GitHub
# accounts and were silently left out -- SolidCraft shipped in the launcher as "unavailable" for
# that reason. This reads `AppKind::Community` out of the launcher's catalog and compares it with
# `ports/`, so a new community app shows up as a gap instead of staying invisible.
#
# Exits 10 when a community app has no Android port, 0 when every one of them does.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CATALOG="$ROOT/src/craft-launcher/crates/core/src/catalog.rs"

[ -f "$CATALOG" ] || { echo "no catalog at $CATALOG (clone craft-launcher first)" >&2; exit 2; }

# Each community entry is a `CraftApp { kind: AppKind::Community, ... ..app("id", ...) }` block.
# Walk the file and pick up the id from the `..app("id",` that closes each such block.
# The id sits on the line after `..app(`, so this has to look ahead one line rather than match the
# same line -- the first version of this script silently found nothing for exactly that reason.
ids=$(awk '
  /AppKind::Community/ { want=1 }
  want && /\.\.app\(/ { grab=1; want=0; next }
  grab && match($0, /"[a-z0-9]+"/) {
    id=substr($0, RSTART+1, RLENGTH-2); print id; grab=0
  }
' "$CATALOG")

if [ -z "$ids" ]; then
  echo "no community apps in the catalog"
  exit 0
fi

missing=0
for id in $ids; do
  if [ -d "$ROOT/ports/$id-android" ]; then
    printf '%-14s ported\n' "$id"
  else
    printf '%-14s NOT PORTED\n' "$id"
    missing=1
  fi
done

[ "$missing" = 1 ] && exit 10
exit 0
