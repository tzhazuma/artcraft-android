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

## launcher-platform-android.patch

`Os::current()` returned `None` on Android: it tests `cfg!(target_os = "linux")`, which is false
when `target_os = "android"`, so the launcher had no platform at all and every app showed
"Not available here" (此平台不可用).

The patch adds `Os::Android` with the `android-arm64` / `android-aarch64` tokens the ports actually
publish, and `PackageKind::Apk` so `<stem>-<version>-android-arm64.apk` parses instead of being
skipped by `parse_asset_name`.

Not covered yet: detecting already-installed apps (needs a `PackageManager` query over JNI) and
pointing downloads at the ported releases.

### Second change in the same patch: the version segment is optional

The ported builds are uploaded as `Photocraft-android-arm64.apk`, with no version — but
`parse_asset_name` required `<stem>-<version>-<os>-<arch>`, so no asset ever matched and every app
still said "Not available here" even after the platform itself was recognised. The parser now
accepts `<stem>-<os>-<arch>` too.

With both changes the launcher's status bar reads "安装平台: ANDROID ARM64" instead of having no
platform at all. What remains is the release fetch: the cards sit at "离线" (offline), which points
at HTTPS in the Android shell — `ports/craft-launcher-android/src/lib.rs` notes that
rustls-platform-verifier needs a Kotlin component and a Maven dependency to reach the platform trust
store, and this build has neither.

### Third change: the APK had no INTERNET permission

The manifest declared none, so every release fetch failed and the launcher showed every app as
"离线" while the platform was recognised correctly. The symptom reads as a network or TLS fault;
the cause is one missing `<uses-permission>` line. Added to the same patch.

### Fourth change: asset stems are matched case-insensitively

The app ids are lowercase (`photocraft`) and the uploaded assets are capitalised
(`Photocraft-android-arm64.apk`), so `strip_prefix` skipped every asset. A case-insensitive
comparison fixes it.

Verified as far as the device allowed: with INTERNET added the launcher fetches the release feed
(cards show "版本 0.14.0 · 2026年10月23日"), reads 149 system root certificates, and the status bar
reads "安装平台: ANDROID ARM64". The case-insensitive build was installed but the device dropped off
wireless adb before its UI could be checked, so whether the cards now offer Install is unconfirmed.

### Fifth change: installed apps, detected

The launcher looks for `.app` bundles, desktop files and package-manager records; none exist on
Android, so every app read as "not installed" even after it had been installed. The Rust crate has
no JNI, and `PackageManager` is unreachable without it — but the activity already has both, so it
writes `<files>/installed-apps.txt` (`<package id>\t<version name>` per line) and
`artcraft-launcher-native`'s Android detection reads it, reporting `InstallSource::Apk`.

Compiles for aarch64-linux-android and the APK builds. Not yet verified on a device: wireless adb
went away mid-session.

### Sixth and seventh changes: releases from the port's own repository

The launcher was reading release metadata from the upstream ArtCraft feed, which advertises
desktop-only assets; the Android assets live in `tzhazuma/artcraft-android`. `feed_url()` now honours
`ARTCRAFT_LAUNCHER_NO_FEED`, and the port sets it, so every release is read from `RELEASE_REPO` over
GitHub.

Verified on the device: the cards read "可安装" with an 安装 button, the version line shows
"android-v1", and the status bar reads "来源: GITHUB / 安装平台: ANDROID ARM64".

Also declared `QUERY_ALL_PACKAGES`: Android 11+ hides other packages from getInstalledPackages, so
the installed suite was invisible and every card said "not installed". Whether that fixed the
"已安装 0" count is unconfirmed -- this launcher renders through a SurfaceView, and screencap on
this ROM keeps returning the same frame, so the UI cannot be read back reliably.

### Eighth change: installed apps, actually detected

`getInstalledPackages(0)` on this ROM answers with only the launcher itself, even with
QUERY_ALL_PACKAGES and a `<queries>` block naming the rest. The activity now probes each suite id
with `getPackageInfo(name, 0)` and merges whatever the listing does return.

Verified on the device: `files/installed-apps.txt` holds 14 lines with versions, and the launcher's
tabs read 全部 13 / 已安装 12 / 更新 0 / 未安装 1.
