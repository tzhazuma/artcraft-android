# SolidCraft for Android arm64-v8a

SolidCraft 0.27.1 (`tangency-labs/solidcraft`) on Android, following the pattern the nine other
ArtCraft ports use: a thin app crate (`apps/solidcraft-android`) that reuses the desktop engine and
egui front end unchanged, plus a Gradle shell around a GameActivity-hosted `libmain.so`.

This is a **new port**, not a sync: SolidCraft is not under `storytold`, it is much larger than the
other Craft apps, and it ships a custom GPU renderer.

## Package id

`ai.storyteller.solidcraft`, deliberately **not** the upstream bundle `io.tangencylabs.solidcraft`.

The ArtCraft Launcher enumerates installed apps and matches on `CraftApp::android_bundle_id()`,
which returns `bundle_id()` unless the app is listed in `ANDROID_PACKAGE_IDS`. SolidCraft's declared
bundle is `io.tangencylabs.solidcraft`, but its `former_bundles` include `ai.storyteller.solidcraft`,
and the launcher's Java only probes names starting with `ai.storyteller.`. Shipping the upstream id
would make the app invisible to the launcher — no card, no launch, no update offer.

## Layout

```
ports/solidcraft-android/
  Cargo.toml                     [lib] name="main", crate-type=["cdylib"]
  src/lib.rs                     android_main + TouchShell + environment + fonts
  src/saf.rs                     Storage Access Framework bridge (JNI -> Java)
  android/                       the Gradle app (applicationId ai.storyteller.solidcraft)
    app/src/main/java/ai/storyteller/solidcraft/MainActivity.java   GameActivity + SAF
    app/src/main/res/            launcher icon, generated from the app's own icon
patches/solidcraft-android/
  upstream/arboard-android/      no-op arboard: the real crate has no Android backend
  upstream/setup-arboard-stub.sh adds [patch.crates-io] arboard to the clone (idempotent)
dist-repos/solidcraft-android/scripts/
  build-so.sh <app>              cargo ndk -> apps/solidcraft-android/jniLibs/arm64-v8a/libmain.so
  build-apk.sh <app> [version]   gradle assembleRelease -> out/SolidCraft-<version>-android-arm64.apk
```

`build-so.sh` mirrors `ports/solidcraft-android/` into `src/solidcraft/apps/solidcraft-android/`
before every build (`jniLibs` excluded, so a stale `.so` can never survive a failed build), runs the
arboard setup script, and builds. The upstream workspace already has `apps/*` in `members`, so no
membership edit is needed — the only upstream file this port touches is the root `Cargo.toml`'s
`[patch.crates-io]` table, and only through that script.

## Why a stub for arboard

`solidcraft-ui-egui` declares `arboard` (the system clipboard) under
`cfg(not(target_arch = "wasm32"))` — true on Android — and arboard 3.6 selects a platform module
for Windows, macOS and X11/Wayland only. On Android it does not compile at all:

```
error[E0425]: cannot find function `get` in module `platform`
error[E0425]: cannot find function `set_text` in module `platform`
```

That was the **only** thing in SolidCraft's dependency graph that did not build for
aarch64-linux-android: `solidcraft-engine` (kernel, truck geometry, IO, document model, render) and
the whole 203k-line `solidcraft-ui-egui` compile for Android unmodified.

The shim keeps arboard's public surface and always answers "the clipboard holds nothing", which is
what a build with no clipboard integration can honestly say. In-app Cut/Copy/Paste still works: that
is egui's own clipboard, not arboard. Cross-app copy/paste is a follow-up (JNI `ClipboardManager`).

## What the shell does that the desktop does

| Desktop (`apps/solidcraft`) | Android (this crate) |
|---|---|
| `std::env::args()`: designs, `--sample`, `--safe-mode`, `--control`, `--theme-file` | none: the app opens on its start page |
| `settings_dir()` from `HOME`/XDG | `SOLIDCRAFT_SETTINGS_DIR` = `<internal_data_path>/settings` |
| `crash::default_dir`, `recovery::default_dir` from `$XDG_DATA_HOME` | `SOLIDCRAFT_RECOVERY_DIR` = `<internal_data_path>/recovery` (crash reports land beside it) |
| `rfd` file dialogs | SAF: `ACTION_OPEN_DOCUMENT` / `ACTION_CREATE_DOCUMENT`; picked documents are copied into `<filesDir>/import/` and handed to the engine as real paths |
| `arboard` clipboard | not wired; the shim reports an empty clipboard |
| `keyring`, `ureq` update check, control server, EasyPrint upload, agent-chat HTTP | `Services` leaves those hooks `None`; the menus that need them do not offer them |
| 3D mice, native menu/crash handlers, macOS open-document events, X11/Wayland | the desktop-only modules are not compiled |
| `load_system_fonts_in_background` scanning desktop font folders | `SOLIDCRAFT_FONT_DIRS=/system/fonts`, so the device's `NotoSansCJK-Regular.ttc` is found and CJK/Hangul text is not blank boxes |
| 1600×1000 window, mouse, hover | touch targets raised to 36 pt; landscape orientation |

The viewport is the desktop's: `SolveApp::set_wgpu_split` installs the scene pipelines (depth 24,
stencil 8, MSAA 4× where the device supports it — the engine drops to 1× itself when the format
cannot multisample), `install_picture_renderer` gives thumbnails the GPU, and the window pass has no
depth of its own because the scene is drawn offscreen and blitted, exactly as on the desktop.

## Known gaps

* **CJK text draws as `◻`.** Latin, digits and shortcuts render normally; every Chinese/Japanese/
  Korean character is the replacement square. The device faces are found, parsed and installed
  (`/system/fonts/NotoSansCJK-Regular.ttc` face 2 for Simplified Chinese, `DroidSansFallbackBBK.ttf`
  as a TrueType fallback), and egui's own `Fonts::has_glyph` reports the glyph is there — but the
  app's text still lays out to the replacement character. The check in [`TouchShell::install_cjk`]
  looks at the `Proportional` family only; SolidCraft's body text uses the
  `FontFamily::Name("ui-medium"/"ui-semibold"/"ui-bold")` families, so the next step is to run that
  check per family and put the CJK face first in whichever family lacks it (`apply_cjk(ctx, true)`
  already does the placement, it is just never reached because the `Proportional` check passes).
  See STATUS.md for the logcat evidence.

* **Density.** The layout is the desktop's: a workspace ribbon, a browser tree, a viewport with an
  on-screen navigation bar, a command dialog column. A phone in landscape is ~914×411 pt where the
  layout is designed for ≥960 pt wide. Enlarging the touch targets is the only change made; a real
  touch rework (collapsed ribbon, bottom sheets) is not in this port.
* **No cross-app clipboard**, see above.
* **No update check, no agent-chat API agents, no control server**: `Services` leaves them `None`.
* **No file-association intent filters**: opening a `.sc` from a file manager does not start the app
  yet; use File › Open inside the app.
* **No `--safe-mode`.** The desktop's recovery path (a few failed launches start in safe mode with
  the software renderer) needs a settings folder the shell can inspect before the window opens; the
  shell does set `SOLIDCRAFT_SETTINGS_DIR`, so the pieces are there, but the startup checks are not
  wired.
