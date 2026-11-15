#![cfg(target_os = "android")]

//! What the Android shell can do for the window.
//!
//! The desktop host (`apps/mailcraft`) implements the whole [`Services`] surface: file dialogs,
//! the credential store, desktop alerts, the self-updater, Graph and Exchange runners, and so on.
//! This shell implements the part an email client needs to be an email client, and leaves the rest
//! `None` — the UI already handles a host that has no hook for something (the browser shell is the
//! same): the menus that need it do not offer it.
//!
//! | Desktop (`apps/mailcraft`) | Android (this crate) |
//! |---|---|
//! | `rfd` file dialogs, `file_dialogs::Launcher` | Storage Access Framework ([`crate::saf`]): the picked file is copied into the app's directory and handed over as a real path |
//! | `store::Saver` on a thread | the same `Saver`, in the app's private directory |
//! | Send/Receive over IMAP and SMTP (`mailcraft_net::sync_with`) | the same, on a background thread |
//! | the OAuth sign-in on the provider's page | the same, with the address opened by an Intent |
//! | `keyring` credential store | none: nothing is remembered between runs (see the README) |
//! | Microsoft Graph, Exchange Web Services, JMAP runners | none: mail is read over IMAP/SMTP |
//! | `arboard` clipboard | the system clipboard through JNI (text) |
//! | `open_file` through the system's program | `ACTION_VIEW` through a `FileProvider` |
//! | desktop alerts, self-update, diagnostics' log tail | none |

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use egui::Context;
use mailcraft_model::sync::{SyncRequest, SyncResult};
use mailcraft_ui_egui::{FileDialogRequest, MailApp, RichClipboard, Services, SignInRequest};

use crate::saf;

/// How much of a picked file name a save picker will take as a suggestion.
const MAX_SUGGESTED: usize = 96;

/// Run `job` on a thread of its own and hand its answer back through the receiver, waking the
/// window when it arrives. A thread that can't be started leaves the receiver closed, which every
/// caller reads as "it didn't answer" — and is logged here, not swallowed.
fn background<T: Send + 'static>(ctx: &Context, what: &str, job: impl FnOnce() -> T + Send + 'static) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel();
    let wake = ctx.clone();
    let name = what.to_string();
    let spawned = std::thread::Builder::new().name(what.into()).spawn(move || {
        let value = job();
        let _ = tx.send(value);
        wake.request_repaint();
    });
    if let Err(e) = spawned {
        log::error!("mailcraft-android: couldn't start {name}: {e}");
    }
    rx
}

/// The services this shell provides. `data_dir` is the app's private `filesDir`; `activity` is
/// `AndroidApp::activity_as_ptr()`.
pub fn services(activity: usize, data_dir: PathBuf, ctx: &Context) -> Services {
    let mut services = Services::default();
    let import_dir = data_dir.join("import");
    let export_dir = data_dir.join("export");
    let opened_dir = data_dir.join("opened");
    for dir in [&import_dir, &export_dir, &opened_dir] {
        let _ = std::fs::create_dir_all(dir);
    }

    // ---- files in ----
    let (dialog_ctx, dialog_activity) = (ctx.clone(), activity);
    let (open_dir, save_dir) = (import_dir.clone(), export_dir.clone());
    services.file_dialog = Some(Box::new(move |request: FileDialogRequest| {
        let (tx, rx) = mpsc::channel();
        let ctx = dialog_ctx.clone();
        let (activity, open_dir, save_dir) = (dialog_activity, open_dir.clone(), save_dir.clone());
        let spawned = std::thread::Builder::new().name("file-dialog".into()).spawn(move || {
            let answer = match request {
                FileDialogRequest::Open { purpose } => {
                    let paths = saf::pick_media(activity, &open_dir, saf::PICK_TIMEOUT);
                    log::info!("mailcraft-android: picked {} path(s) to {purpose}", paths.len());
                    paths.into_iter().next()
                }
                FileDialogRequest::Save { name } => {
                    let leaf = safe_leaf(&name);
                    let target = save_dir.join(&leaf);
                    let _ = std::fs::remove_file(&target);
                    if saf::pick_save(activity, &leaf, saf::PICK_TIMEOUT) {
                        let path = target.to_string_lossy().into_owned();
                        // The app writes the file after this answer is taken; the copy to the
                        // destination the owner chose starts when it stops growing.
                        saf::publish_when_written(activity, path.clone());
                        Some(path)
                    } else {
                        None
                    }
                }
                // A folder of mbox files: the Storage Access Framework can pick a tree, but
                // reading it back means walking `content://` children, which this shell does not
                // do. The window says nothing was picked.
                FileDialogRequest::Folder => {
                    log::info!("mailcraft-android: importing a folder is not available in this build");
                    None
                }
            };
            let _ = tx.send(answer);
            ctx.request_repaint();
        });
        if let Err(e) = spawned {
            log::error!("mailcraft-android: couldn't start the file dialog: {e}");
        }
        rx
    }));

    // ---- the mailbox ----
    let cancel = Arc::new(AtomicBool::new(false));
    let sync_cancel = cancel.clone();
    let sync_ctx = ctx.clone();
    services.sync = Some(Box::new(move |request: SyncRequest| {
        let cancel = sync_cancel.clone();
        cancel.store(false, Ordering::Relaxed);
        // Pieces as they come (what was sent, then folder by folder): the channel is short so a
        // window that falls behind holds the run back instead of piling pieces up.
        let (tx, rx) = mpsc::sync_channel(8);
        let wake = sync_ctx.clone();
        let cancel_thread = cancel.clone();
        let spawned = std::thread::Builder::new().name("send/receive".into()).spawn(move || {
            mailcraft_net::sync_with(&request, &cancel_thread, &mut |piece: SyncResult| {
                if tx.send(piece).is_err() {
                    cancel_thread.store(true, Ordering::Relaxed);
                }
                wake.request_repaint();
            });
        });
        if let Err(e) = spawned {
            log::error!("mailcraft-android: couldn't start Send/Receive: {e}");
        }
        rx
    }));
    services.sync_cancel = cancel;

    // ---- signing in and the servers ----
    let sign_in_ctx = ctx.clone();
    services.sign_in = Some(Box::new(move |request: SignInRequest| {
        let ctx = sign_in_ctx.clone();
        background(&sign_in_ctx, "sign-in", move || sign_in(&ctx, activity, request))
    }));
    let test_ctx = ctx.clone();
    services.test_account = Some(Box::new(move |account, secret| {
        background(&test_ctx, "account check", move || mailcraft_net::test_account(&account, &secret).map_err(|e| e.to_string()))
    }));
    let mx_ctx = ctx.clone();
    services.lookup_mx = Some(Box::new(move |domain: String| {
        background(&mx_ctx, "mail exchangers", move || mailcraft_net::lookup_mx(&domain).map_err(|e| e.to_string()))
    }));
    // A picture a message keeps on the web, once the owner asked for that message's pictures. The
    // address is the message's choice; `fetch` checks where it leads before going there.
    let picture_ctx = ctx.clone();
    services.pictures = Some(Box::new(move |url: String| {
        background(&picture_ctx, "picture", move || mailcraft_net::pictures::fetch(&url).map_err(|e| e.to_string()))
    }));

    // ---- the system ----
    services.clipboard = Some(Box::new(SystemClipboard { activity }));
    let open_activity = activity;
    services.open_file = Some(Box::new(move |name: &str, bytes: &[u8]| {
        let leaf = safe_leaf(name);
        let path = opened_dir.join(&leaf);
        std::fs::write(&path, bytes).map_err(|e| format!("couldn't write {leaf}: {e}"))?;
        let path = path.to_string_lossy().into_owned();
        // The system's program for the kind is chosen by the Intent's MIME type.
        if saf::open_file(open_activity, &path, "") {
            Ok(())
        } else {
            Err(format!("no program on this device opens {leaf}"))
        }
    }));
    services.system_locale = saf::device_locale(activity);
    services.diagnostics = Some(Box::new(|| {
        format!("MailCraft {} on Android (arm64-v8a)\n", env!("CARGO_PKG_VERSION"))
    }));
    // This APK is not one of the project's own packages: it never asks about a newer release, and
    // installing one of those would replace this build with a desktop one.
    services.development_build = true;
    services
}

/// Put the mailbox's saver in place, now that the file it keeps is known.
pub fn attach_saver(app: &mut MailApp, ctx: &Context, path: &std::path::Path) {
    use mailcraft_engine::io::BackgroundSave as _;
    let wake = ctx.clone();
    match mailcraft_engine::store::Saver::spawn(move || wake.request_repaint()) {
        Ok(saver) => {
            saver.follow(path);
            if path.exists() {
                saver.sweep_later(path);
            }
            app.services.saver = Some(Box::new(saver));
        }
        Err(e) => log::error!("mailcraft-android: couldn't start the mailbox saver: {e}"),
    }
}

/// A file name a document picker and the filesystem both accept: no separators, never empty, and
/// not so long that the picker's own title is cut off.
fn safe_leaf(name: &str) -> String {
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = leaf.chars().map(|c| if c == '/' || c == '\\' || c == '\0' || c.is_control() { '_' } else { c }).collect();
    let cleaned = cleaned.trim();
    let mut out: String = cleaned.chars().take(MAX_SUGGESTED).collect();
    if out.is_empty() {
        out.push_str("mailcraft-file");
    }
    out
}

/// The system's clipboard. egui's own carries text within the window; this one is what another
/// program sees, and what Paste in a text's menu reads.
struct SystemClipboard {
    activity: usize,
}

impl RichClipboard for SystemClipboard {
    /// Only the text half: an Android clipboard entry holds HTML as a second `ClipData` item, which
    /// this bridge does not write. The text still arrives in another program.
    fn put(&self, _html: &str, text: &str) -> Result<(), String> {
        if saf::set_clipboard_text(self.activity, text) { Ok(()) } else { Err("the system clipboard didn't take the text".into()) }
    }

    fn html(&self) -> Option<String> {
        None
    }

    fn text(&self) -> Option<String> {
        saf::clipboard_text(self.activity)
    }
}

// ---- signing in ----

/// Sign an account's owner in on the provider's own page, with the address opened by an Intent.
fn sign_in(ctx: &Context, activity: usize, request: SignInRequest) -> Result<mailcraft_model::sync::Tokens, String> {
    sign_in_with(request, &|url| {
        let opened = saf::open_url(activity, url);
        ctx.request_repaint();
        if opened {
            log::info!("mailcraft-android: opened the sign-in page in the browser");
        } else {
            log::warn!("mailcraft-android: no browser could be opened for the sign-in");
        }
        opened
    })
}

/// [`sign_in`], with `open_page` in the browser's place: it opens the address it is given, and
/// says whether it could. (This is the desktop app's own flow, unchanged; only the way a page is
/// opened differs.)
fn sign_in_with(request: SignInRequest, open_page: &dyn Fn(&str) -> bool) -> Result<mailcraft_model::sync::Tokens, String> {
    use mailcraft_net::oauth::{Endpoints, Resource, SignInEnd};
    use mailcraft_ui_egui::approval::{NEEDS_APPROVAL, NO_BROWSER, SIGN_IN_DECLINED, SIGN_IN_TIMED_OUT};

    // No browser could be started: nothing can come back, so the wait is called off at once, and
    // that is what is said.
    let no_browser = AtomicBool::new(false);
    let open = |url: &str| {
        if !open_page(url) {
            no_browser.store(true, Ordering::Relaxed);
            request.cancel.store(true, Ordering::Relaxed);
        }
    };
    let unopened = |said: String| if no_browser.load(Ordering::Relaxed) { NO_BROWSER.to_string() } else { said };
    if request.graph && request.favorites {
        let asked = mailcraft_net::oauth::sign_in_for_favorites(&request.oauth, &request.email, &open, &request.cancel);
        return asked.map_err(|refusal| unopened(favorites_refusal(refusal)));
    }
    if request.graph && request.shared && request.shared_calendars {
        let asked = mailcraft_net::oauth::sign_in_for_shared_calendars(&request.oauth, &request.email, &open, &request.cancel);
        return asked.map_err(|refusal| unopened(favorites_refusal(refusal)));
    }
    if request.graph && request.shared && request.shared_send {
        let asked = mailcraft_net::oauth::sign_in_for_shared_send(&request.oauth, &request.email, &open, &request.cancel);
        return asked.map_err(|refusal| unopened(favorites_refusal(refusal)));
    }
    if request.graph && request.tasks {
        let asked = mailcraft_net::oauth::sign_in_for_tasks(&request.oauth, &request.email, &open, &request.cancel);
        return asked.map_err(|refusal| unopened(favorites_refusal(refusal)));
    }
    if request.graph && request.shared_calendars {
        let asked = mailcraft_net::oauth::sign_in_for_opened_calendars(&request.oauth, &request.email, &open, &request.cancel);
        return asked.map_err(|refusal| unopened(favorites_refusal(refusal)));
    }
    if request.graph && request.shared {
        let asked = mailcraft_net::oauth::sign_in_for_shared(&request.oauth, &request.email, &open, &request.cancel);
        return asked.map_err(|refusal| unopened(favorites_refusal(refusal)));
    }
    let on_the_page = |endpoints: &Endpoints| {
        let asked = mailcraft_net::oauth::sign_in_explained(endpoints, &request.oauth, &request.email, &open, &request.cancel);
        asked.map_err(|end| {
            let (how, said) = match end {
                SignInEnd::Approval => ("an administrator has to approve the application first", NEEDS_APPROVAL.to_string()),
                _ if no_browser.load(Ordering::Relaxed) => ("no browser could be started", NO_BROWSER.to_string()),
                SignInEnd::Declined => ("the page came back with a no", SIGN_IN_DECLINED.to_string()),
                SignInEnd::TimedOut => ("nothing came back in time", SIGN_IN_TIMED_OUT.to_string()),
                SignInEnd::Cancelled => ("it was called off", mailcraft_net::NetError::SignInCancelled.to_string()),
                SignInEnd::Other(e) => ("it failed", e.to_string()),
            };
            log::info!("sign-in on the provider's page, account {}: {how}", request.account);
            said
        })
    };
    if request.graph {
        let resource = if request.mail_write { Resource::GraphMail } else { Resource::Graph };
        return on_the_page(&Endpoints::for_resource(&request.oauth, resource));
    }
    if !request.refresh.is_empty() {
        match mailcraft_net::oauth::refresh(&request.oauth, &request.refresh) {
            Ok(tokens) => return Ok(tokens),
            Err(e) if e.is_refused_sign_in() => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    if request.quiet {
        return Err(format!("The sign-in for {} has run out. Use Send/Receive to sign in again.", request.email));
    }
    if request.mail_only {
        let mail = Endpoints { consent: String::new(), lesser: String::new(), ..Endpoints::of(&request.oauth) };
        return on_the_page(&mail);
    }
    on_the_page(&Endpoints::of(&request.oauth))
}

/// How a refusal of the Favorites' permissions is said, in the words the window knows it by.
fn favorites_refusal(refusal: mailcraft_net::oauth::FavoritesRefusal) -> String {
    use mailcraft_net::oauth::FavoritesRefusal;
    use mailcraft_ui_egui::favorites_sync::{NEEDS_APPROVAL, REFUSED_BY_ORGANISATION, REFUSED_FOR_ACCOUNT};
    match refusal {
        FavoritesRefusal::Approval => NEEDS_APPROVAL.to_string(),
        FavoritesRefusal::Organisation => REFUSED_BY_ORGANISATION.to_string(),
        FavoritesRefusal::Account => REFUSED_FOR_ACCOUNT.to_string(),
        FavoritesRefusal::Other(e) => e.to_string(),
    }
}
