//! MailCraft on Android — GameActivity + eframe/wgpu, reusing the desktop engine and egui UI.
//!
//! The mailbox model, the command registry, the IMAP/SMTP transport and the egui front end are
//! untouched: the same [`mailcraft_engine::Session`] and [`mailcraft_ui_egui::MailApp`] the desktop
//! shell builds are wired up here. What `apps/mailcraft` gets from the OS, this crate takes from
//! the activity (the table of differences is in [`host`]).
//!
//! Known gaps are listed in the port's README; none of them stop the window from coming up.

#![cfg(target_os = "android")]

mod host;
mod saf;

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

use mailcraft_engine::Session;
use mailcraft_engine::io::OpenProgress;
use mailcraft_model::Mailbox;
use mailcraft_ui_egui::opening::{Arrival, Opening, Trouble};
use mailcraft_ui_egui::{MailApp, UiState};
use winit::platform::android::activity::AndroidApp;

/// The package this build ships under. The ArtCraft Launcher only probes names under
/// `ai.storyteller.`, so a community app has to use an id the catalog declares for it (see the
/// README and `~/artcraft-android/STATUS.md`).
const PACKAGE: &str = "ai.storyteller.mailcraft";

/// The interface size of a touch target, in points. The layout is the desktop's; enlarging the
/// interactive size is the one change that helps everywhere without moving anything.
const TOUCH_TARGET: f32 = 36.0;

/// The Android entry point. winit's `android-activity` glue looks this symbol up in the cdylib, so
/// the name and the signature are fixed — hence `unsafe(no_mangle)` and the crate-level
/// `unsafe_code = "deny"` rather than the workspace's `forbid`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("MailCraft"),
    );
    log::info!("mailcraft-android: starting (MailCraft {})", env!("CARGO_PKG_VERSION"));

    let data_dir = app.internal_data_path().or_else(|| app.external_data_path());
    let Some(data_dir) = data_dir else {
        log::error!("mailcraft-android: no data directory; the mailbox cannot be kept");
        return;
    };
    log::info!("mailcraft-android: data dir {}", data_dir.display());
    for sub in ["import", "export", "opened"] {
        if let Err(e) = std::fs::create_dir_all(data_dir.join(sub)) {
            log::warn!("mailcraft-android: {}/{sub}: {e}", data_dir.display());
        }
    }
    // The Java activity is a `GameActivity` subclass; the bridges call back into it by pointer for
    // the life of the process.
    let activity = app.activity_as_ptr() as usize;

    let options = eframe::NativeOptions {
        android_app: Some(app),
        viewport: egui::ViewportBuilder::default().with_title("MailCraft").with_app_id(PACKAGE),
        // The window is the whole screen; there is no title bar to persist or restore.
        persist_window: false,
        ..Default::default()
    };

    let shell_dir = data_dir.clone();
    let result = eframe::run_native(
        "MailCraft",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let prefs = load_prefs(&shell_dir);
            let locale = saf::device_locale(activity);
            let mailbox_path = shell_dir.join(format!("mailbox.{}", mailcraft_engine::io::MAILBOX_EXT));
            let progress = Arc::new(OpenProgress::default());

            // The mailbox is read on a thread of its own so the window is there at once (#519):
            // it shows the opening screen while a large mailbox is read, exactly as on the desktop.
            let (tx, rx) = mpsc::channel();
            let wake = ctx.clone();
            let reading = Arc::clone(&progress);
            let path = mailbox_path.clone();
            let spawned = std::thread::Builder::new().name("opening the mailbox".into()).spawn(move || {
                let arrival = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| first_mailbox(&path, &reading)))
                    .unwrap_or_else(|_| {
                        log::error!("mailcraft-android: opening the mailbox stopped unexpectedly; starting empty and leaving the file alone");
                        Arrival::unreadable(Trouble::Stopped)
                    });
                let _ = tx.send(arrival);
                wake.request_repaint();
            });
            if let Err(e) = spawned {
                log::error!("mailcraft-android: couldn't start opening the mailbox: {e}");
            }

            let build_ctx = ctx.clone();
            let build_dir = shell_dir.clone();
            let build_prefs = prefs.clone();
            let build: mailcraft_ui_egui::opening::Build = Box::new(move |arrival| {
                let kept = arrival.path.clone();
                let mut session = Session::new(arrival.mailbox);
                session.path = kept.clone();
                // Search answers from a word index kept beside the mailbox, built in the background.
                session.index_words = true;
                let services = host::services(activity, build_dir.clone(), &build_ctx);
                let mut app = MailApp::new(session, services);
                // Nowhere to keep the mailbox: don't try.
                app.autosave = kept.is_some();
                // An ordinary start on a mailbox nobody has set up yet begins with adding an account.
                app.offer_setup = true;
                app.auto_sync = true;
                if let Some(path) = kept.clone() {
                    host::attach_saver(&mut app, &build_ctx, &path);
                }
                if let Some(prefs) = build_prefs.clone() {
                    app.apply_prefs(prefs);
                }
                if arrival.unavailable > 0 {
                    log::warn!("mailcraft-android: {} bodies or attachments are missing or damaged", arrival.unavailable);
                }
                app.index_words_later();
                app
            });

            let mut app = MailApp::opening(Opening::new(rx, progress, build), prefs, locale);
            app.integrated_titlebar = false;
            Ok(Box::new(Shell { app, data_dir: shell_dir, tuned: false, frames: 0, cjk: CjkSetup::new(), saved_ui: String::new(), checked: 0.0 }))
        }),
    );
    if let Err(e) = result {
        log::error!("mailcraft-android: eframe exited: {e}");
    }
}

/// The mailbox to start with and the file it is kept in, counting what is read of it in `progress`.
///
/// A first start keeps an empty mailbox and opens Add Account (the window offers it). A file that
/// can't be read is never written over: the window starts empty, keeps nothing, and says why.
fn first_mailbox(path: &Path, progress: &OpenProgress) -> Arrival {
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = mailcraft_engine::store::create_private_dir_all(dir);
        }
        return Arrival::opened(Mailbox::with_local_account("", ""), Some(path.to_path_buf()));
    }
    match mailcraft_engine::store::open_for_window(path, progress) {
        Ok(opened) => {
            if let Some(notice) = opened.notice() {
                log::warn!("{}: {notice}", path.display());
            }
            let damaged = opened.lost_saves > 0 || opened.unfinished > 0 || opened.lost_records > 0;
            let trouble = if damaged {
                Some(Trouble::Damaged)
            } else if opened.repaired > 0 {
                Some(Trouble::Repaired(opened.repaired))
            } else {
                None
            };
            log::info!("mailcraft-android: mailbox read ({} messages)", opened.mailbox.messages.len());
            Arrival { unavailable: opened.unavailable, unchecked: opened.unchecked, damaged, trouble, ..Arrival::opened(opened.mailbox, Some(path.to_path_buf())) }
        }
        Err(e) => {
            log::error!("mailcraft-android: the mailbox couldn't be read: {e}");
            Arrival::unreadable(Trouble::Unreadable(e))
        }
    }
}

/// The interface state the window keeps between runs, in the app's private directory.
fn prefs_path(data_dir: &Path) -> PathBuf {
    data_dir.join("ui.json")
}

fn load_prefs(data_dir: &Path) -> Option<UiState> {
    let path = prefs_path(data_dir);
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<UiState>(&text) {
        Ok(ui) => {
            log::info!("mailcraft-android: interface state read from {}", path.display());
            Some(ui)
        }
        Err(e) => {
            log::warn!("mailcraft-android: {} couldn't be read ({e}); using the defaults", path.display());
            None
        }
    }
}

fn save_prefs(data_dir: &Path, app: &MailApp) {
    let path = prefs_path(data_dir);
    let text = match serde_json::to_string(&app.prefs()) {
        Ok(text) => text,
        Err(e) => {
            log::error!("mailcraft-android: the interface state couldn't be written: {e}");
            return;
        }
    };
    if let Err(e) = std::fs::write(&path, text) {
        log::error!("mailcraft-android: {}: {e}", path.display());
    }
}

/// The eframe app: forward everything to [`MailApp`], tune for touch, keep the interface state,
/// and install the device's CJK face.
struct Shell {
    app: MailApp,
    data_dir: PathBuf,
    /// The one-shot touch tuning has run.
    tuned: bool,
    /// Frames laid out; the first is logged so a launch can be confirmed from logcat.
    frames: u32,
    /// Where the CJK face stands (see [`CjkSetup`]).
    cjk: CjkSetup,
    /// The interface state as last stored.
    saved_ui: String,
    /// When the interface state was last looked at (milliseconds).
    checked: f64,
}

impl Shell {
    /// Touch targets for a phone. The layout is the desktop's; this is the one change that helps
    /// everywhere without moving anything (the density of a phone against a desktop layout is a
    /// real gap, listed in the port's README).
    fn tune(ctx: &egui::Context) {
        ctx.all_styles_mut(|style| {
            style.spacing.interact_size = egui::vec2(TOUCH_TARGET, TOUCH_TARGET);
            style.spacing.button_padding = egui::vec2(8.0, 6.0);
            style.spacing.item_spacing = egui::vec2(6.0, 6.0);
            // A resting finger must not raise a tooltip over the control it covers.
            style.interaction.show_tooltips_only_when_still = true;
        });
        let rect = ctx.viewport_rect();
        log::info!(
            "mailcraft-android: touch mode applied (ppp {}, logical {}×{} pt)",
            ctx.pixels_per_point(),
            rect.width().round(),
            rect.height().round()
        );
    }
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.logic(ctx);
        // Interface state: written a moment after it changes, checked every couple of seconds (the
        // same rhythm the browser shell uses).
        let now = mailcraft_ui_egui::now_ms();
        if now - self.checked > 2000.0 {
            self.checked = now;
            if let Ok(text) = serde_json::to_string(&self.app.prefs())
                && text != self.saved_ui
            {
                let dir = self.data_dir.clone();
                let writing = text.clone();
                let _ = std::thread::Builder::new().name("saving the interface state".into()).spawn(move || {
                    if let Err(e) = std::fs::write(prefs_path(&dir), writing) {
                        log::error!("mailcraft-android: {}: {e}", prefs_path(&dir).display());
                    }
                });
                self.saved_ui = text;
            }
        }
        // File › Close asks the app to quit; on Android that ends the activity.
        if self.app.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        // Keyboard events are forwarded as they come. (What an input method commits is not among
        // them: winit 0.30's Android backend drops `InputEvent::TextEvent` — see the README.)
        self.app.raw_input_hook(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.tuned {
            self.tuned = true;
            Self::tune(&ctx);
        }
        self.app.ui(ui);
        // The app installs its interface fonts on its first frame; the device's CJK face goes in
        // after that, and stays (nothing replaces the font set later).
        self.cjk.tick(&ctx);
        self.frames += 1;
        if self.frames == 1 {
            log::info!("mailcraft-android: first frame laid out, the UI is up");
        }
        if self.frames % 600 == 0 {
            self.cjk.report(&ctx);
        }
    }

    fn on_exit(&mut self) {
        if !self.app.is_opening() {
            save_prefs(&self.data_dir, &self.app);
            // The last changes, if the timer hadn't got to them yet, and whatever the background
            // saver is still writing.
            if let Err(e) = self.app.save_on_exit() {
                log::error!("mailcraft-android: saving the mailbox on exit failed: {e}");
            }
        }
        // Written and done with: the mailbox's memory goes with the process instead of being handed
        // back message by message (the desktop's `on_exit` does the same).
        std::mem::forget(std::mem::take(&mut self.app.session.mailbox));
        log::info!("mailcraft-android: exiting");
    }
}

// ---- the device's CJK face ----

/// System faces that carry Chinese, best first. Measured on the vivo PA2573: `NotoSansCJK` is an
/// OpenType/CFF collection (face 2 is the Simplified Chinese one), the Droid faces are plain
/// TrueType.
const CJK_CANDIDATES: &[(&str, u32)] = &[
    ("/system/fonts/DroidSansFallbackBBK.ttf", 0),
    ("/system/fonts/DroidSansFallbackFull.ttf", 0),
    ("/system/fonts/DroidSansFallback.ttf", 0),
    ("/system/fonts/NotoSansCJK-Regular.ttc", 2),
    ("/system/fonts/NotoSansSC-Regular.otf", 0),
];

/// The name the shell's own CJK face is installed under.
const CJK_FACE: &str = "android-cjk";

/// Where the CJK face stands. egui draws `◻` for a character no face in the family claims, so
/// installing a face and *using* it are two separate facts; this tracks which one holds.
struct CjkSetup {
    /// The next candidate to try, and the bytes of the one being tried.
    candidate: usize,
    /// The face on trial: its bytes and index in the file.
    face: Option<(String, u32, Vec<u8>)>,
    /// Frames to let a font change settle before checking whether it took effect.
    settle: u8,
    /// Whether a face is installed at all.
    installed: bool,
    /// Whether it had to be put first in every family.
    promoted: bool,
    /// The check found Chinese text drawn by the face: done.
    done: bool,
    /// Nothing on the device worked.
    failed: bool,
    /// The number of candidate changes made, for the log.
    tries: u32,
}

impl CjkSetup {
    fn new() -> Self {
        CjkSetup { candidate: 0, face: None, settle: 2, installed: false, promoted: false, done: false, failed: false, tries: 0 }
    }

    /// A key lookup on every frame; work only until the face is in place and used.
    fn tick(&mut self, ctx: &egui::Context) {
        if self.done || self.failed {
            return;
        }
        if self.settle > 0 {
            self.settle -= 1;
            return;
        }
        if self.face.is_none() {
            // The app's own `install_fonts` has run by the first frame (its `logic` comes first).
            match self.next_face() {
                Some((path, index, bytes)) => {
                    log::info!("mailcraft-android: trying the CJK face {path} (face {index})");
                    self.face = Some((path, index, bytes));
                    self.tries += 1;
                    self.install(ctx, false);
                    self.installed = true;
                    self.settle = 2;
                }
                None => {
                    self.failed = true;
                    log::error!("mailcraft-android: no CJK face on this device draws Chinese; the interface will show boxes");
                }
            }
            return;
        }
        if !self.installed {
            return;
        }
        if self.cjk_works(ctx) {
            self.done = true;
            log::info!("mailcraft-android: CJK face in place after {} try(ies){}", self.tries, if self.promoted { ", promoted to the front" } else { "" });
            return;
        }
        if !self.promoted {
            // An appended face that egui never consults looks exactly like a missing font: put it
            // first in every family instead.
            self.install(ctx, true);
            self.promoted = true;
            self.settle = 2;
            return;
        }
        // This face doesn't work even first: try the next one from the top.
        self.face = None;
        self.installed = false;
        self.promoted = false;
        self.settle = 1;
    }

    fn next_face(&mut self) -> Option<(String, u32, Vec<u8>)> {
        while let Some((path, index)) = CJK_CANDIDATES.get(self.candidate).copied() {
            self.candidate += 1;
            match std::fs::read(path) {
                Ok(bytes) if !bytes.is_empty() => return Some((path.to_string(), index, bytes)),
                Ok(_) => log::info!("mailcraft-android: {path} is empty"),
                Err(e) => log::info!("mailcraft-android: {path}: {e}"),
            }
        }
        None
    }

    /// Whether egui's own glyph lookup finds a Chinese character in every family (the interface
    /// lays its text out in `medium`/`semibold`/`bold` as well as `Proportional`).
    fn cjk_works(&self, ctx: &egui::Context) -> bool {
        ctx.fonts_mut(|f| {
            let families: Vec<egui::FontFamily> = f.definitions().families.keys().cloned().collect();
            families.into_iter().all(|family| f.has_glyph(&egui::FontId::new(16.0, family), '简'))
        })
    }

    /// Put the face into the current font definitions, first or last in each family.
    fn install(&mut self, ctx: &egui::Context, first: bool) {
        let Some((path, index, bytes)) = self.face.as_ref() else { return };
        let mut fonts = ctx.fonts(|f| f.definitions().clone());
        let mut data = egui::FontData::from_owned(bytes.clone());
        data.index = *index;
        fonts.font_data.insert(CJK_FACE.to_owned(), Arc::new(data));
        for family in fonts.families.keys().cloned().collect::<Vec<_>>() {
            let list = fonts.families.entry(family).or_default();
            list.retain(|name| name != CJK_FACE);
            if first {
                list.insert(0, CJK_FACE.to_owned());
            } else {
                list.push(CJK_FACE.to_owned());
            }
        }
        ctx.set_fonts(fonts);
        log::info!("mailcraft-android: CJK face {path} (face {index}) {}", if first { "put first" } else { "appended" });
    }

    fn report(&self, ctx: &egui::Context) {
        let present = ctx.fonts(|f| f.definitions().font_data.contains_key(CJK_FACE));
        let works = self.cjk_works(ctx);
        let galley = ctx.fonts_mut(|f| {
            f.layout_no_wrap("简体中文".to_owned(), egui::FontId::proportional(16.0), egui::Color32::WHITE)
        });
        log::info!(
            "mailcraft-android: CJK status: installed {present}, egui finds the glyph {works}, 简体中文 at 16 pt is {}×{} pt",
            galley.size().x.round(),
            galley.size().y.round()
        );
    }
}
