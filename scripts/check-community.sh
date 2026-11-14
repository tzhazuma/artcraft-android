#!/usr/bin/env bash
# Report community apps the ArtCraft Launcher knows about but this port set does not cover.
#
# The launcher's catalog is the authority on what the suite contains. Official apps live under
# `storytold/*` and the port pipeline grew up around them; community apps come from other GitHub
# accounts and were silently left out -- SolidCraft shipped in the launcher as "unavailable" for
# that reason. This finds every `kind = "community"` app and compares it with `ports/`.
#
# Source of truth moved: until launcher 0.14 the catalog was a `SUITE` literal in `catalog.rs`, and
# this script scraped `AppKind::Community` out of it. 0.19 generates the suite from
# `crates/core/catalog.toml` at build time, so the literal no longer exists anywhere in the data and
# that scrape started reporting a `..app("artcraft")` from a unit test as an unported app. Parse the
# manifest when it is there, and fall back to the old scrape for older checkouts.
#
# Exits 10 when a community app has no Android port, 0 when every one of them does.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src/craft-launcher/crates/core/catalog.toml"
CATALOG="$ROOT/src/craft-launcher/crates/core/src/catalog.rs"

ids=""
if [ -f "$MANIFEST" ]; then
  ids=$(awk '
    # Emit BEFORE clearing: a combined "clear then emit" rule always emitted nothing, because the
    # clear ran first and the guard saw the id it had just blanked.
    /^\[\[apps\]\]/ && id != "" && kind == "community" { print id }
    /^\[\[apps\]\]/ { id=""; kind="" }
    /^id = "/    { gsub(/^id = "|"$/, ""); id=$0 }
    /^kind = "/  { gsub(/^kind = "|"$/, ""); kind=$0 }
    END { if (id != "" && kind == "community") print id }
  ' "$MANIFEST" | awk '!seen[$0]++')
elif [ -f "$CATALOG" ]; then
  ids=$(awk '
    /CraftApp \{/ { want=0; grab=0 }
    /AppKind::Community/ { want=1 }
    want && /\.\.app\(/ { grab=1; want=0; next }
    grab && match($0, /"[a-z0-9]+"/) { print substr($0, RSTART+1, RLENGTH-2); grab=0 }
  ' "$CATALOG")
else
  echo "no catalog at $MANIFEST or $CATALOG (clone craft-launcher first)" >&2
  exit 2
fi

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
