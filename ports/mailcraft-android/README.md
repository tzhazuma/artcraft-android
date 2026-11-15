# MailCraft on Android (arm64-v8a)

The fourteenth port in `~/artcraft-android`, and the second community app (after SolidCraft).
Upstream: `UCS-Care-Group/mailcraft` v0.9.0 — a Rust + egui (eframe 0.37) mail, calendar and
contacts client, so the established pattern applies unchanged: a new `apps/mailcraft-android` crate
reuses `mailcraft-engine` and `mailcraft-ui-egui`, and this shell supplies only what the desktop app
gets from its operating system.

```
ports/mailcraft-android/
  Cargo.toml            [lib] name="main", crate-type=["cdylib"]
  src/lib.rs            android_main, mailbox opening, touch tuning, CJK face
  src/host.rs           Services: file dialogs, saver, IMAP/SMTP, OAuth, clipboard, attachments
  src/saf.rs            JNI bridge to MainActivity (SAF pickers, browser, clipboard, MIME)
  android/              Gradle shell (AGP 9.1 / Gradle 9.3.1 / JDK 21 / minSdk 31 / compileSdk 35)
```

`dist-repos/mailcraft-android/scripts/{build-so.sh,build-apk.sh}` mirror `ports/` over
`src/mailcraft/apps/mailcraft-android/` and build it; `patches/mailcraft-android/upstream/setup-tls-android.sh`
is run by `build-so.sh` before every build.

## The package id — why `ai.storyteller.mailcraft`

MailCraft's declared bundle is `io.github.billsecond.mailcraft`, and its `former_bundles` list was
**empty**: there is no historical `ai.storyteller.*` id to ship under. The ArtCraft Launcher's
Android activity only probes package names under `ai.storyteller.`
(`MainActivity.writeInstalledApps()`), so the port declares one and wires it everywhere:

| place | change |
|---|---|
| port's `applicationId` | `ai.storyteller.mailcraft` |
| `crates/core/catalog.toml` | MailCraft's `former_bundles = ["ai.storyteller.mailcraft"]` |
| `crates/core/src/catalog.rs` | `ANDROID_PACKAGE_IDS`: `("mailcraft", "ai.storyteller.mailcraft")` |
| launcher `MainActivity.java` | `suite` array gains `"ai.storyteller.mailcraft"` |
| launcher `AndroidManifest.xml` | `<queries>` gains `<package android:name="ai.storyteller.mailcraft" />` |

Installing the upstream id instead would leave the app invisible to the launcher no matter what.

## What the shell supplies

| desktop (`apps/mailcraft`) | Android |
|---|---|
| `rfd` dialogs / `file_dialogs::Launcher` | Storage Access Framework: picked documents are copied into `files/import/` and the app is handed real paths; a save hands the app a path in `files/export/` and copies it to the `content://` destination once it stops growing |
| `store::Saver` on a thread | the same `Saver`, over `files/mailbox.mailcraft` |
| Send/Receive over IMAP and SMTP | the same `mailcraft_net::sync_with`, on a background thread |
| OAuth sign-in on the provider's page | the same flow; the page is opened with `Intent.ACTION_VIEW` |
| `keyring` credential store | **none** — see the gaps |
| Microsoft Graph / Exchange Web Services / JMAP runners | **none** — mail is read over IMAP/SMTP |
| `arboard` clipboard | the system clipboard through JNI (text; not HTML) |
| opened attachments through the system's program | written to `files/opened/`, shared through a `FileProvider`, opened with `ACTION_VIEW` |
| desktop alerts, self-update, diagnostics log tail | none; `development_build = true`, so the app never offers a desktop release |

Fonts: the app installs its bundled Inter/Source Sans/JetBrains Mono and then looks for a system
face for a language left on Automatic. On Android the desktop font folders don't exist, so the shell
installs the device's CJK face (`DroidSansFallbackBBK.ttf`, else `NotoSansCJK-Regular.ttc` face 2 —
the Simplified Chinese one) into every family after the app's own `install_fonts`, and verifies with
`egui::Fonts::has_glyph` that it is actually consulted (egui draws `◻` for a missing glyph, so
"installed" and "used" are different facts).

Network: `INTERNET` is in the manifest. `mailcraft-net` checks certificates with
`rustls-platform-verifier`, whose Android backend is a JVM AAR from a Maven repository that this
Gradle shell does not depend on — so `patches/mailcraft-android/upstream/setup-tls-android.sh` makes
Android read `/system/etc/security/cacerts` (the same roots Android's TrustManager consults) and
verify with `WebPkiServerVerifier`. Desktop builds are untouched.

## Build and run

```bash
cd ~/artcraft-android
bash dist-repos/mailcraft-android/scripts/build-so.sh  mailcraft
bash dist-repos/mailcraft-android/scripts/build-apk.sh mailcraft 0.9.0
bash dist-repos/mailcraft-android/scripts/device-smoke.sh \
     ai.storyteller.mailcraft .MainActivity MailCraft mailcraft-real
```

## Known gaps (honest list)

1. **Nothing is remembered between runs.** There is no Android credential store in this build:
   passwords and refresh tokens live in memory for the session only. `/system` has no keyring, and
   writing secrets into a plain file in the app's private directory would be worse than asking
   again — so `Services::secrets` is `None`. Adding it means the Android Keystore, which is a
   separate piece of work.
2. **Microsoft Graph, Exchange Web Services and JMAP are not wired.** The engine and the transport
   support them; the desktop app's orchestration for them is ~2,000 lines across `main.rs`,
   `ews_runner.rs` and `ews_setup.rs`, and only some of it is in libraries. Mail over IMAP/SMTP —
   including Microsoft 365 over IMAP with OAuth — is the supported path. An account kept in step
   through Graph has no calendar and no contacts in this build.
3. **No folder import** (`FileDialogRequest::Folder`): walking a `content://` tree is not
   implemented; the picker returns "nothing picked". Single files (`.eml`, `.mbox`, `.ics`,
   `.vcf`) import fine.
4. **The clipboard carries text, not HTML**: copying formatted text from another program arrives
   as plain text.
5. **The layout is the desktop's.** Touch targets are 36 pt and the window is landscape, but the
   three-pane layout at ~1238×826 pt is dense on a phone.
6. **No desktop notifications** for reminders, and no self-update (that is deliberate: a desktop
   release installed here would replace this build).
