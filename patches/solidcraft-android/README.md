# SolidCraft: the one upstream patch this port needs

The port itself is `ports/solidcraft-android/` (the shell crate and the Gradle app). This directory
holds the only change the *upstream* tree needs, plus the script that applies it.

## `upstream/arboard-android/` — a no-op `arboard`

`solidcraft-ui-egui` declares the system clipboard crate `arboard` under
`cfg(not(target_arch = "wasm32"))`, which is true on Android. arboard 3.6 has no Android backend:
`src/platform/mod.rs` selects a module for Windows, macOS and the X11/Wayland desktops only, so the
crate fails to compile for `aarch64-linux-android` with

```
error[E0425]: cannot find function `get` in module `platform`
error[E0425]: cannot find function `set_text` in module `platform`
```

Nothing else in SolidCraft's graph had this problem. `solidcraft-engine` (geometry kernel on truck,
sketch solver, document model, IO, render) and the whole `solidcraft-ui-egui` front end compile for
Android unmodified — measured with `cargo +stable ndk -t arm64-v8a check`, 2 m 58 s and 25 s
respectively.

`arboard-android` keeps arboard's public surface — `Clipboard::new`, `get_text`, `get_image`,
`get`, `Get::{text, image, file_list, html}`, `set_text`, `set_image`, `clear` — and answers like a
clipboard with nothing on it. That is what a build with no clipboard integration can honestly say:
the four call sites in `solidcraft-ui-egui` are the theme import's Paste button, the text field's
right-click menu ("is there anything to paste?"), and the agent chat's attach-picture /
attach-files actions. egui's own in-app Cut/Copy/Paste is a separate mechanism and is unaffected.

## `upstream/setup-arboard-stub.sh` — apply it

Idempotent; it inserts one line into the clone's root `[patch.crates-io]` table:

```toml
[patch.crates-io]
arboard = { path = "../../patches/solidcraft-android/upstream/arboard-android" }
```

`dist-repos/solidcraft-android/scripts/build-so.sh` runs it before every build, so a fresh clone
needs no manual step:

```sh
bash patches/solidcraft-android/upstream/setup-arboard-stub.sh ~/artcraft-android/src/solidcraft
```

## What this does *not* touch

The upstream workspace already lists `apps/*` in `members`, so adding `apps/solidcraft-android` needs
no edit to `Cargo.toml` beyond the patch line above. No upstream source file is modified, and no
upstream behaviour changes: the shim only exists on Android, where the alternative is not building
at all.
