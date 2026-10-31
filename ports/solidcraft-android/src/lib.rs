//! SolidCraft on Android — GameActivity + eframe/wgpu, reusing the desktop engine and egui UI.
//!
//! The kernel, the document model, the command registry, the egui front end and the wgpu viewport
//! are untouched: the same [`solidcraft_engine::Session`] and [`solidcraft_ui_egui::SolveApp`] the
//! desktop shell builds are wired up here. What `apps/solidcraft` gets from the OS, this crate
//! takes from the activity:
//!
//! | Desktop (`apps/solidcraft`) | Android (this crate) |
//! |---|---|
//! | `std::env::args()` (designs, `--sample`, `--safe-mode`, `--control`) | none: the app starts on its start page |
//! | `settings_dir()` from `HOME`/XDG, `eframe::storage_dir` | [`AndroidApp::internal_data_path`] via `SOLIDCRAFT_SETTINGS_DIR` |
//! | `crash::default_dir`, `recovery::default_dir` from `$XDG_DATA_HOME` | the app's private directory via `SOLIDCRAFT_RECOVERY_DIR` |
//! | `rfd` file dialogs | Storage Access Framework ([`saf`]): picked documents are copied into the app directory and handed over as real paths |
//! | `arboard` system clipboard | not wired: the shim in `patches/solidcraft-android` reports an empty clipboard |
//! | `keyring` API keys, `ureq` update check, control server | none: `Services` leaves those hooks `None` |
//! | 3D mice (`space-mouse`), native menu/crash handlers, macOS open-documents | none: the desktop-only modules are not compiled here |
//! | `load_system_fonts_in_background` scanning desktop font folders | `SOLIDCRAFT_FONT_DIRS=/system/fonts` so the CJK/Hangul faces on the device are found |
//! | 1600×1000 window, mouse and hover | [`TouchShell`] raises the touch target size |
//!
//! Known gaps are listed in the port's README; none of them stop the UI from coming up.
#![cfg(target_os = "android")]

mod saf;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use solidcraft_engine::Session;
use solidcraft_ui_egui::{FilePickerFuture, Services, SolveApp};
use winit::platform::android::activity::AndroidApp;

/// The scene's depth buffer, as `apps/solidcraft` uses it ([`DEPTH_BITS`] there). The window pass
/// itself has no depth: the viewport draws its scene offscreen and blits it.
const DEPTH_BITS: u8 = 24;
/// The scene's stencil buffer, as on the desktop ([`STENCIL_BITS`] there).
const STENCIL_BITS: u8 = 8;
/// The anti-aliasing the desktop asks for by default (`scene_msaa_request(None)`). The engine's own
/// check in `set_wgpu_split` drops it to 1× when the device cannot multisample that format, so a
/// phone that only supports 1× still gets a correct picture.
const SCENE_SAMPLES: u32 = 4;

/// The Android entry point. winit's `android-activity` glue looks this symbol up in the cdylib,
/// so the name and the signature are fixed — hence `unsafe(no_mangle)` and the crate-level
/// `unsafe_code = "deny"` rather than the workspace's `forbid`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("SolidCraft"),
    );
    log::info!("solidcraft-android: starting (SolidCraft {})", env!("CARGO_PKG_VERSION"));

    let data_dir = app.internal_data_path().or_else(|| app.external_data_path());
    if let Some(dir) = &data_dir {
        log::info!("solidcraft-android: data dir {}", dir.display());
        for sub in ["settings", "recovery", "import", "export"] {
            let _ = std::fs::create_dir_all(dir.join(sub));
        }
        prepare_environment(dir);
    }
    // The Java activity is a `GameActivity` subclass; the SAF bridge calls back into it by
    // pointer for the life of the process.
    let activity = app.activity_as_ptr() as usize;

    let options = eframe::NativeOptions {
        android_app: Some(app),
        viewport: egui::ViewportBuilder::default()
            .with_title("SolidCraft")
            .with_app_id("ai.storyteller.solidcraft"),
        // The viewport draws its scene offscreen with its own depth and MSAA, then blits it: the
        // window's own pass pays for neither (same as the desktop's `NativeOptions`).
        depth_buffer: 0,
        stencil_buffer: 0,
        multisampling: 1,
        // Preferences are kept in the app's private directory, in eframe's usual key.
        persistence_path: data_dir.as_ref().map(|d| d.join("app.ron")),
        persist_window: false,
        ..Default::default()
    };

    let shell_dir = data_dir.clone();
    let result = eframe::run_native(
        "SolidCraft",
        options,
        Box::new(move |cc| {
            // SolidCraft's bundled IBM Plex faces first; the device's CJK/Hangul/Thai faces are
            // read on a worker and installed when they arrive (the UI polls for them every frame).
            solidcraft_ui_egui::fonts::setup_bundled_fonts(&cc.egui_ctx);
            solidcraft_ui_egui::fonts::load_system_fonts_in_background(&cc.egui_ctx);
            let mut solve = SolveApp::new(Session::default(), services(activity, shell_dir.clone()));
            // Long feature evaluations run off the window thread instead of freezing the UI for
            // seconds: the same 150 ms the desktop app waits before it moves a recompute to a worker.
            solve.apply_off_thread = Some(solidcraft_ui_egui::dialogs::APPLY_AT_ONCE);
            // No desktop window chrome: the app draws its own application bar, and there is nothing
            // to resize from a title bar edge.
            solve.custom_titlebar = false;
            solve.integrated_titlebar = false;
            // No design was given on a command line: show the start page, as the desktop does.
            let rev = solve.session.revision;
            solve.home.show(rev);
            if let Some(stored) = cc.storage.and_then(|s| solidcraft_ui_egui::prefs::stored(|k| s.get_string(k))) {
                solve.load_prefs(&stored);
            }
            if let Some(rs) = &cc.wgpu_render_state {
                let info = rs.adapter.get_info();
                log::info!(
                    "solidcraft-android: adapter {} ({:?}, {:?}), MSAA supported {:?}",
                    info.name.trim(),
                    info.backend,
                    info.device_type,
                    rs.msaa_supported
                );
                let samples = if rs.msaa_supported.contains(&SCENE_SAMPLES) { SCENE_SAMPLES } else { 1 };
                let scene = solve.set_wgpu_split(rs, DEPTH_BITS, STENCIL_BITS, samples);
                log::info!("solidcraft-android: viewport pipelines installed, scene MSAA {scene}x");
                // Headless pictures (thumbnails, save-as-mesh previews) draw on the GPU when it can.
                solidcraft_ui_egui::gpu::install_picture_renderer(rs);
            } else {
                log::error!("solidcraft-android: no wgpu render state; the viewport cannot draw");
            }
            Ok(Box::new(TouchShell::new(solve, shell_dir)))
        }),
    );
    if let Err(e) = result {
        log::error!("solidcraft-android: eframe exited: {e}");
    }
}

/// Point SolidCraft's per-user folders at the app's private directory.
///
/// The engine resolves these from the environment (`solidcraft_geom::env`), which on Android has no
/// `HOME`/`XDG_DATA_HOME` to resolve them from: without this, crash reports and the autosave that
/// offers to bring back an interrupted design are simply disabled, and the system CJK faces in
/// `/system/fonts` are never looked at (the desktop candidate list has no Android path).
#[allow(unsafe_code)]
fn prepare_environment(data_dir: &Path) {
    // SAFETY: called once, at the very top of `android_main`, before eframe (or anything else)
    // starts a thread. Nothing reads the process environment concurrently at this point.
    unsafe {
        std::env::set_var("SOLIDCRAFT_SETTINGS_DIR", data_dir.join("settings"));
        std::env::set_var("SOLIDCRAFT_RECOVERY_DIR", data_dir.join("recovery"));
        // `fonts::find_system_cjk_font` looks in these first, by name, before its desktop
        // candidate list.
        std::env::set_var("SOLIDCRAFT_FONT_DIRS", "/system/fonts");
    }
}

/// System faces that carry CJK, in preference order. Measured on the vivo PA2573: `NotoSansCJK`
/// is an OpenType/CFF collection, the two Droid faces are plain TrueType — all three draw 简.
const CJK_CANDIDATES: &[&str] = &[
    "/system/fonts/NotoSansCJK-Regular.ttc",
    "/system/fonts/NotoSerifCJK-Regular.ttc",
    "/system/fonts/DroidSansFallbackBBK.ttf",
    "/system/fonts/DroidSansFallbackMonster.ttf",
    "/system/fonts/DroidSansFallbackFull.ttf",
    "/system/fonts/DroidSansFallback.ttf",
    "/system/fonts/NotoSansSC-Regular.otf",
];

/// The name the shell's own CJK face is installed under.
const CJK_FACE: &str = "android-cjk";

/// One face of one font file, vetted: it parses, it draws Chinese, and its index in the file (a
/// `.ttc` holds a dozen faces) is known.
struct CjkFace {
    path: &'static str,
    index: u32,
    bytes: Vec<u8>,
}

/// The face of a collection to use: the Simplified Chinese one when the file names it, else the
/// first face that draws a Chinese character.
///
/// `egui::FontData` carries the index for exactly this reason, and the default 0 is the *Japanese*
/// face of `NotoSansCJK-Regular.ttc`: it draws 简, but with Japanese glyph forms.
fn simplified_chinese_index(bytes: &[u8]) -> Option<u32> {
    let mut first_drawing = None;
    for index in 0..64u32 {
        let Ok(face) = ttf_parser::Face::parse(bytes, index) else { break };
        if face.glyph_index('简').is_none() || face.glyph_index('中').is_none() {
            continue;
        }
        first_drawing.get_or_insert(index);
        let sc = face
            .names()
            .into_iter()
            .filter(|name| name.name_id == ttf_parser::name_id::FAMILY)
            .filter_map(|name| name.to_string())
            .any(|name| name.contains(" SC") || name.contains("Simplified") || name.contains("GB"));
        if sc {
            return Some(index);
        }
    }
    first_drawing
}

/// Read the system face that draws Chinese, preferring a plain TrueType file over an OpenType/CFF
/// collection: the collection is the better font, but a face epaint cannot rasterise draws boxes
/// just the same, and a TrueType file is the one shape of font every egui build handles.
fn system_cjk_face() -> Option<CjkFace> {
    let mut fallback: Option<CjkFace> = None;
    for path in CJK_CANDIDATES {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::info!("solidcraft-android: {path}: {e}");
                continue;
            }
        };
        let Some(index) = simplified_chinese_index(&bytes) else {
            log::info!("solidcraft-android: {path}: no face draws 简");
            continue;
        };
        let glyphs = ttf_parser::Face::parse(&bytes, index).map(|face| face.number_of_glyphs()).unwrap_or(0);
        log::info!(
            "solidcraft-android: {path}: {} bytes, face {index}, {glyphs} glyphs, CJK yes",
            bytes.len()
        );
        let face = CjkFace { path, index, bytes };
        // A `.ttf`/`.otf` holds one face; a `.ttc` is a collection (and Noto's is CFF-outlined).
        if path.ends_with(".ttf") || path.ends_with(".otf") {
            return Some(face);
        }
        fallback.get_or_insert(face);
    }
    fallback
}


/// What the platform shell can do for the UI.
///
/// File picking is the Storage Access Framework ([`saf`]); the picked document is copied into the
/// app's own directory and the engine is handed a real path, because a `content://` URI is not a
/// path. Everything else stays `None` (no keychain, no update check, no EasyPrint upload), and the
/// UI already handles that: the menus that need them do not offer them.
fn services(activity: usize, data_dir: Option<PathBuf>) -> Services {
    let mut services = Services::default();
    let Some(dir) = data_dir else {
        log::warn!("solidcraft-android: no data directory; file picking is unavailable");
        return services;
    };
    let open_dir = dir.clone();
    services.pick_open = Some(Box::new(move |_extensions: &[&str]| {
        // `extensions` would become the SAF `EXTRA_MIME_TYPES` filter; the port asks for every
        // type instead, because the device's document browser is happier with a broad list and
        // SolidCraft identifies a design by its contents anyway.
        pick_in_background(activity, open_dir.clone())
    }));
    let file_dir = dir.clone();
    services.pick_file = Some(Box::new(move |_label: &str, _extensions: &[&str]| {
        pick_in_background(activity, file_dir.clone())
    }));
    let save_dir = dir;
    services.pick_save = Some(Box::new(move |name: &str, _extensions: &[&str]| {
        save_in_background(activity, save_dir.clone(), name.to_string())
    }));
    services
}

/// Run one SAF pick on a worker thread and hand the answer back through a future.
///
/// The picker cannot run on the UI thread: it waits for the user, and the frame loop must keep
/// drawing. [`FilePickerFuture`] is polled once a frame, so the future only has to watch a slot.
fn pick_in_background(activity: usize, data_dir: PathBuf) -> FilePickerFuture {
    let answer = Arc::new(Mutex::new(None::<Option<String>>));
    let slot = Arc::clone(&answer);
    let spawned = std::thread::Builder::new().name("saf-open".into()).spawn(move || {
        let paths = saf::pick_media(activity, &data_dir, saf::PICK_TIMEOUT);
        let first = paths.into_iter().next();
        match &first {
            Some(path) => log::info!("solidcraft-android: opening {path}"),
            None => log::info!("solidcraft-android: open cancelled"),
        }
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(first);
    });
    if let Err(e) = spawned {
        log::error!("solidcraft-android: could not start the picker thread: {e}");
        *answer.lock().unwrap_or_else(|e| e.into_inner()) = Some(None);
    }
    Box::pin(SafAnswer { answer })
}

/// Ask where a save should land, then let the engine write there.
///
/// The engine saves to a path, and the SAF destination is a `content://` URI, so the shell hands it
/// a file in its own `export/` folder and copies that file to the destination once the engine has
/// written it (see [`saf::publish_when_written`]).
fn save_in_background(activity: usize, data_dir: PathBuf, name: String) -> FilePickerFuture {
    let answer = Arc::new(Mutex::new(None::<Option<String>>));
    let slot = Arc::clone(&answer);
    let spawned = std::thread::Builder::new().name("saf-save".into()).spawn(move || {
        let target = data_dir.join("export").join(sanitise(&name));
        let Some(path) = target.to_str().map(str::to_owned) else {
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(None);
            return;
        };
        // A file left by an earlier save would otherwise be published twice.
        let _ = std::fs::remove_file(&target);
        if saf::pick_save(activity, &name, saf::PICK_TIMEOUT) {
            log::info!("solidcraft-android: saving to {path} and publishing it");
            saf::publish_when_written(activity, path.clone());
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(Some(path));
        } else {
            log::info!("solidcraft-android: save cancelled");
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(None);
        }
    });
    if let Err(e) = spawned {
        log::error!("solidcraft-android: could not start the save thread: {e}");
        *answer.lock().unwrap_or_else(|e| e.into_inner()) = Some(None);
    }
    Box::pin(SafAnswer { answer })
}

/// A file name a document picker will accept: no separators, and never empty.
fn sanitise(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if c == '/' || c == '\\' || c == '\0' { '_' } else { c }).collect();
    if cleaned.trim().is_empty() { "design.sc".to_owned() } else { cleaned }
}

/// The future a picker answers through: `Pending` until the worker fills the slot.
struct SafAnswer {
    answer: Arc<Mutex<Option<Option<String>>>>,
}

impl std::future::Future for SafAnswer {
    type Output = Option<String>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<String>> {
        // Taking the answer out of the slot keeps the future idempotent: once it is `Ready` the
        // picker is done and the UI drops it.
        if let Some(answer) = self.answer.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return std::task::Poll::Ready(answer);
        }
        // The frame loop already repaints every 50 ms while a picker is open, so a missed wake
        // cannot hang the dialog; asking for one anyway keeps it instant.
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    }
}

/// Where the shell's CJK face stands. egui answers "no face has this glyph" by drawing `◻`, so
/// installing a face and *using* it are two separate facts; this tracks which one holds.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum CjkState {
    /// Nothing installed yet (or the UI replaced its font set).
    Idle,
    /// Installed last in every family, waiting to see whether egui consults it.
    Appended,
    /// Installed first in every family, waiting to see whether egui consults it.
    Promoted,
    /// egui's own glyph lookup finds Chinese text: done.
    Works,
    /// No usable device face, or none egui will use.
    Failed,
}

/// The eframe app: forward everything to [`SolveApp`] and keep the preferences on exit.
struct TouchShell {
    solve: SolveApp,
    data_dir: Option<PathBuf>,
    /// The one-shot touch tuning has run (after SolidCraft's own theme is in place).
    tuned: bool,
    /// Frames laid out; the first one is logged so a launch can be confirmed from logcat.
    frames: u32,
    /// The system CJK face, read once. `None` means none of the device's faces was usable.
    cjk: Option<CjkFace>,
    /// Where the CJK face stands (see [`CjkState`]).
    cjk_state: CjkState,
    /// Frames to let a font change settle before checking it took effect.
    cjk_settle: u8,
}

impl TouchShell {
    fn new(solve: SolveApp, data_dir: Option<PathBuf>) -> Self {
        Self { solve, data_dir, tuned: false, frames: 0, cjk: system_cjk_face(), cjk_state: CjkState::Idle, cjk_settle: 0 }
    }

    /// Touch targets for a phone.
    ///
    /// The layout is the desktop's: a ribbon of workspace tabs, a browser tree, a viewport with an
    /// on-screen navigation bar. Enlarging the interactive size is the one change that helps
    /// everywhere without moving anything; the density (a phone shows ~411×914 pt where the
    /// desktop layout is designed for ≥960 pt) is a real gap, listed in the port's README.
    fn tune(ctx: &egui::Context) {
        ctx.all_styles_mut(|style| {
            style.spacing.interact_size = egui::vec2(36.0, 36.0);
            style.spacing.button_padding = egui::vec2(8.0, 6.0);
            style.spacing.item_spacing = egui::vec2(6.0, 6.0);
            // A resting finger must not raise a tooltip over the button it covers.
            style.interaction.show_tooltips_only_when_still = true;
        });
        let rect = ctx.viewport_rect();
        log::info!(
            "solidcraft-android: touch mode applied (ppp {}, logical {}×{} pt)",
            ctx.pixels_per_point(),
            rect.width().round(),
            rect.height().round(),
        );
    }
}

impl TouchShell {
    /// Put the device's CJK face into whatever font definitions are current.
    ///
    /// The engine's own search (`fonts::find_system_cjk_font`) knows the Windows, macOS and Linux
    /// font folders, and on Android has no path of its own; its `SOLIDCRAFT_FONT_DIRS` hook is set
    /// above, but a face it finds is only installed once, on the frame after its background search
    /// finishes — and the UI replaces the whole font set again whenever the language or the
    /// interface font changes. Doing it here, from the frame loop, survives all of that: the check
    /// is a key lookup, and the set is only replaced when the face is missing.
    fn install_cjk(&mut self, ctx: &egui::Context) {
        let present = ctx.fonts(|f| f.definitions().font_data.contains_key(CJK_FACE));
        // TEMPORARY: trace the first frames so the state machine can be followed in logcat.
        if self.frames < 30 {
            let families = ctx.fonts(|f| f.definitions().families.len());
            let first_entries: Vec<String> = ctx.fonts(|f| {
                f.definitions()
                    .families
                    .get(&egui::FontFamily::Proportional)
                    .map(|list| list.iter().take(4).cloned().collect())
                    .unwrap_or_default()
            });
            let works = self.cjk_works(ctx);
            log::info!(
                "solidcraft-android: frame {}: present={present} settle={} state={:?} families={families} has_glyph={works} prop={first_entries:?}",
                self.frames,
                self.cjk_settle,
                self.cjk_state as u8,
            );
        }
        if !present {
            // The UI replaced its font set (a language or interface-font change): start over.
            self.cjk_state = CjkState::Idle;
        } else if matches!(self.cjk_state, CjkState::Works) && !self.cjk_works(ctx) {
            // The face survived but the UI has since added a family that does not consult it, or
            // reordered one. `present` alone cannot see that; only the per-family probe can.
            self.cjk_state = CjkState::Appended;
        }
        if self.cjk_settle > 0 {
            self.cjk_settle -= 1;
            return;
        }
        match self.cjk_state {
            CjkState::Idle => {
                if self.cjk.is_none() {
                    self.cjk_state = CjkState::Failed;
                    return;
                }
                // Last: the bundled faces win for the glyphs they have.
                self.apply_cjk(ctx, false);
                self.cjk_state = CjkState::Appended;
                self.cjk_settle = 2;
            }
            CjkState::Appended => {
                if self.cjk_works(ctx) {
                    self.cjk_state = CjkState::Works;
                } else {
                    // egui 0.36 draws a character no face in the family claims as `◻` (the
                    // white medium square), so an appended face that is not consulted looks
                    // exactly like a missing font. Put it first instead: SolidCraft's own
                    // installer does the same for CJK interface languages.
                    self.apply_cjk(ctx, true);
                    self.cjk_state = CjkState::Promoted;
                    self.cjk_settle = 2;
                }
            }
            CjkState::Promoted => {
                if self.cjk_works(ctx) {
                    self.cjk_state = CjkState::Works;
                    log::info!("solidcraft-android: CJK face had to go first in every family to be used");
                } else {
                    self.cjk_state = CjkState::Failed;
                    log::error!("solidcraft-android: the CJK face is installed but egui does not use it");
                }
            }
            CjkState::Works | CjkState::Failed => {}
        }
    }

    /// Whether egui's own glyph lookup finds a Chinese character in the proportional family.
    fn cjk_works(&self, ctx: &egui::Context) -> bool {
        // Every family, not just `Proportional`. SolidCraft's body text is laid out with the named
        // families `ui-medium` / `ui-semibold` / `ui-bold`, and a probe that only asks the
        // proportional family answers "the glyph is there" while the screen still draws `◻` --
        // which is exactly why the promote path below never fired. A family that cannot draw the
        // character is a failure even when another one can.
        ctx.fonts_mut(|f| {
            let families: Vec<egui::FontFamily> = f.definitions().families.keys().cloned().collect();
            families.into_iter().all(|family| {
                f.has_glyph(&egui::FontId::new(16.0, family), '简')
            })
        })
    }

    /// Put the device's CJK face into the current font definitions, first or last in each family.
    fn apply_cjk(&mut self, ctx: &egui::Context, first: bool) {
        let Some(face) = self.cjk.as_ref() else { return };
        let mut fonts = ctx.fonts(|f| f.definitions().clone());
        let mut data = egui::FontData::from_owned(face.bytes.clone());
        // The face inside the collection, not its first one (see `simplified_chinese_index`).
        data.index = face.index;
        fonts.font_data.insert(CJK_FACE.to_owned(), std::sync::Arc::new(data));
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
        if self.cjk_state != CjkState::Promoted {
            log::info!(
                "solidcraft-android: CJK face {} from {} (face {})",
                if first { "put first" } else { "appended" },
                face.path,
                face.index
            );
        }
    }

    /// Say once in a while whether the CJK face is installed and whether egui finds Chinese text.
    ///
    /// `◻` is what egui 0.36 draws for a character no face in the family claims, so "installed" and
    /// "used" are two different facts and both are logged.
    fn report_cjk(&self, ctx: &egui::Context) {
        let present = ctx.fonts(|f| f.definitions().font_data.contains_key(CJK_FACE));
        let works = self.cjk_works(ctx);
        let galley = ctx.fonts_mut(|f| {
            f.layout_no_wrap("简体中文".to_owned(), egui::FontId::proportional(16.0), egui::Color32::WHITE)
        });
        log::info!(
            "solidcraft-android: CJK status: installed {present}, egui finds the glyph {works}, 简体中文 at 16 pt is {}×{} pt",
            galley.size().x.round(),
            galley.size().y.round()
        );
    }
}

impl eframe::App for TouchShell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.solve.logic(ctx);
        // File › Close asks the app to quit; on Android that ends the activity. A save still being
        // written is finished by `on_exit`.
        if self.solve.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.solve.raw_input_hook(raw);
        let _ = ctx;
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.solve.ui(ui);
        if !self.tuned {
            self.tuned = true;
            Self::tune(ui.ctx());
        }
        self.install_cjk(ui.ctx());
        self.frames += 1;
        if self.frames % 600 == 0 {
            self.report_cjk(ui.ctx());
        }
        if self.frames == 1 {
            log::info!("solidcraft-android: first frame laid out, the UI is up");
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(solidcraft_ui_egui::prefs::STORAGE_KEY, self.solve.prefs());
    }

    fn on_exit(&mut self) {
        // A save still being written ends before the app does.
        solidcraft_ui_egui::save_job::tick(&mut self.solve, true);
        // Pictures still being drawn off the window thread end before the GPU is torn down: one
        // drawn while the process exits can crash the driver (the desktop's `on_exit` does this).
        if !solidcraft_engine::capture::close(std::time::Duration::from_secs(5)) {
            log::warn!("solidcraft-android: a picture was still being drawn at exit");
        }
        if let Some(dir) = &self.data_dir {
            log::info!("solidcraft-android: exiting, data in {}", dir.display());
        }
    }
}
