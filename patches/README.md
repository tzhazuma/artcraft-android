# Patches against the upstream ArtCraft sources

`src/soundcraft` and `src/cadcraft` are separate upstream git clones, so the parent repo only
records a commit pointer for them. The Android port's own edits live in those working trees and
would otherwise be lost on a fresh clone, so they are exported here as patches.

Apply from inside the respective clone:

```sh
cd src/soundcraft && git apply ../../patches/soundcraft-menu-bar.patch
cd src/cadcraft   && git apply ../../patches/cadcraft-menu-bar.patch
```

## soundcraft-menu-bar.patch / cadcraft-menu-bar.patch

Fix the clipped menu bar. The menu panel was a fixed `exact_size(24.0)` (SoundCraft) /
`exact_size(22.0)` (CADCraft) while the Android touch shell sets
`style.spacing.interact_size = (40, 40)`. `egui::MenuBar` sizes its row to `interact_size.y` and
centres the buttons, so the text centre landed ~20 pt below the bar's content top and the lower
part of every glyph fell outside the panel, hidden by the separator and the next panel.

The bar height now derives from the style:

```rust
// soundcraft: 24 desktop, 44 on Android (the +4 is the frame's 2 pt top and bottom margin)
let bar_height = 24.0_f32.max(ui.spacing().interact_size.y + 4.0);
// cadcraft: 22 desktop, 40 on Android
let bar_height = 22.0_f32.max(ui.spacing().interact_size.y);
```

Measured on device: SoundCraft's menu ink went from 18 clipped rows to the full 26; CADCraft's
is complete at 24. Taps still open the menus.
