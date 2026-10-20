#!/usr/bin/env python3
"""Generate Android launcher icons from an ArtCraft app's ORIGINAL artwork.

The ArtCraft repos all ship assets/app-icon/<app>-1024.png (plus a hicolor/ tree and
an SVG). Those are the real brand icons, so use them instead of inventing one.

Output per app:
  mipmap-{mdpi,hdpi,xhdpi,xxhdpi,xxxhdpi}/ic_launcher.png     legacy density buckets
  mipmap-anydpi-v26/ic_launcher.xml + ic_launcher_round.xml    adaptive icon
  mipmap-anydpi/ic_launcher.xml + _round.xml                   vector-only fallback
  drawable/ic_launcher_foreground.png                          the artwork, 66% safe zone
  values/ic_launcher_background.xml                            corner colour of the artwork

Adaptive icons crop to a shape and only the centre 66% is guaranteed visible, so the
artwork goes in as the foreground at 66% scale over its own corner colour, which keeps
the original look while surviving every mask.
"""
import os, sys, subprocess
from PIL import Image

SIZES = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
SAFE = 0.66          # adaptive-icon safe zone
FG = 432             # foreground bitmap size (xxxhdpi-safe, 108dp * 4)


def find_source(app_dir, app):
    cand = [
        os.path.join(app_dir, "assets", "app-icon", f"{app}-1024.png"),
        os.path.join(app_dir, "assets", "app-icon", "hicolor", "512x512", "apps", f"{app}.png"),
        os.path.join(app_dir, "assets", "app-icon", f"{app}.png"),
    ]
    for c in cand:
        if os.path.exists(c):
            return c
    # any *-1024.png in assets/app-icon, then any hicolor png
    d = os.path.join(app_dir, "assets", "app-icon")
    if os.path.isdir(d):
        for root, _, files in os.walk(d):
            for f in files:
                if f.endswith("-1024.png"):
                    return os.path.join(root, f)
        for root, _, files in os.walk(d):
            for f in sorted(files):
                if f.endswith(".png"):
                    return os.path.join(root, f)
    return None


def hex_of(px):
    return "#ff%02x%02x%02x" % px[:3]


def build(app_dir, app, res_dir):
    src = find_source(app_dir, app)
    if not src:
        print(f"  ❌ {app}: no source artwork found")
        return False
    im = Image.open(src).convert("RGBA")
    # corner colour = a representative brand colour for the adaptive background
    bg = im.getpixel((6, 6))
    if bg[3] < 200:
        bg = im.getpixel((im.width - 6, im.height - 6))
    if bg[3] < 200:
        bg = (11, 11, 12, 255)

    os.makedirs(os.path.join(res_dir, "drawable"), exist_ok=True)
    os.makedirs(os.path.join(res_dir, "values"), exist_ok=True)

    # 1) legacy density buckets: the artwork as-is
    for dpi, px in SIZES.items():
        d = os.path.join(res_dir, f"mipmap-{dpi}")
        os.makedirs(d, exist_ok=True)
        im.resize((px, px), Image.LANCZOS).save(os.path.join(d, "ic_launcher.png"))
        im.resize((px, px), Image.LANCZOS).save(os.path.join(d, "ic_launcher_round.png"))

    # 2) adaptive foreground: artwork inset into the 66% safe zone
    fg = Image.new("RGBA", (FG, FG), (0, 0, 0, 0))
    inner = int(FG * SAFE)
    scaled = im.resize((inner, inner), Image.LANCZOS)
    fg.paste(scaled, ((FG - inner) // 2, (FG - inner) // 2), scaled)
    fg.save(os.path.join(res_dir, "drawable", "ic_launcher_foreground.png"))

    # 3) adaptive + legacy-vector XML
    for d in ("mipmap-anydpi-v26", "mipmap-anydpi"):
        p = os.path.join(res_dir, d)
        os.makedirs(p, exist_ok=True)
        for name in ("ic_launcher.xml", "ic_launcher_round.xml"):
            open(os.path.join(p, name), "w").write(
                '<?xml version="1.0" encoding="utf-8"?>\n'
                '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
                '    <background android:drawable="@color/ic_launcher_background"/>\n'
                '    <foreground android:drawable="@drawable/ic_launcher_foreground"/>\n'
                '</adaptive-icon>\n')

    open(os.path.join(res_dir, "values", "ic_launcher_background.xml"), "w").write(
        '<?xml version="1.0" encoding="utf-8"?>\n<resources>\n'
        f'    <color name="ic_launcher_background">{hex_of(bg)}</color>\n</resources>\n')

    print(f"  ✅ {app}: {os.path.basename(src)} -> {res_dir}  (bg {hex_of(bg)})")
    # A leftover vector placeholder shadows the bitmap we just wrote: aapt2 resolves
    # drawable/ic_launcher_foreground.xml in preference to .png, so the launcher keeps drawing the
    # generated grid instead of the app's real artwork. Drop the XML so the PNG wins.
    for stale in ("ic_launcher_foreground.xml", "ic_launcher_monochrome.xml"):
        f = os.path.join(res_dir, "drawable", stale)
        if os.path.exists(f):
            os.remove(f)
            print(f"  removed stale {stale}")

    return True


if __name__ == "__main__":
    app, app_dir, res_dir = sys.argv[1], sys.argv[2], sys.argv[3]
    sys.exit(0 if build(app_dir, app, res_dir) else 1)
