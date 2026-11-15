#![cfg(target_os = "android")]

//! The Java side of the shell: the Storage Access Framework file pickers, the system browser for a
//! sign-in, the clipboard for what egui's own can't carry, and opening an attachment.
//!
//! Everything here runs on a background thread: the pickers are posted to the UI thread by Java and
//! this side polls the `isDone` / `isSaveDone` flags, so the egui UI thread never blocks. Import
//! results travel through the filesystem (`<filesDir>/import-manifest.txt`, written by Java) rather
//! than JNI marshalling, so the JNI surface stays at a handful of calls. A JNI failure is logged and
//! reported as such, never turned into "the user cancelled": silence is what hid a missing method in
//! the other ports for several rounds.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jni::objects::{JObject, JValue};
use jni::{JavaVM, jni_sig, jni_str};

/// How long a picker may stay open before the shell gives up waiting. The user may take their
/// time choosing a file; the flag is only polled, so this costs nothing while it waits.
pub const PICK_TIMEOUT: Duration = Duration::from_secs(600);

/// How long a save waits for the app to write the file before it gives up publishing it.
const WRITE_TIMEOUT: Duration = Duration::from_secs(600);

/// The manifest the Java side writes; one picked path per line.
const MANIFEST: &str = "import-manifest.txt";

/// Attach the current thread, look up the activity object and run `body`.
#[allow(unsafe_code)]
fn call<T>(activity: usize, body: impl FnOnce(&mut jni::Env<'_>, &JObject<'_>) -> jni::errors::Result<T>) -> Option<T> {
    let ctx = ndk_context::android_context();
    // SAFETY: `ndk_context` is set up by android-activity before `android_main` runs and stays
    // valid for the life of the process; `from_raw` only wraps the pointer in the jni crate's type
    // (it caches the JVM, which the JVM itself ref-counts for the process lifetime).
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
    match vm.attach_current_thread(|env| -> jni::errors::Result<T> {
        // SAFETY: the pointer is `AndroidApp::activity_as_ptr()`, a reference to the running
        // activity that is valid for the whole process; `JObject::from_raw` borrows it for this JNI
        // frame.
        let activity = unsafe { JObject::from_raw(env, activity as *mut _) };
        body(env, &activity)
    }) {
        Ok(value) => Some(value),
        Err(err) => {
            log::error!("JNI call failed: {err:?}");
            None
        }
    }
}

/// Open the system document picker and return the paths copied into the app directory (empty when
/// the user cancels, or on any failure — the caller just does nothing).
///
/// `activity` is `AndroidApp::activity_as_ptr()` (the Java activity object; `ndk_context` hands out
/// the *Application* instead, which has no `startActivityForResult`), and `data_dir` is
/// `AndroidApp::internal_data_path()`, the same `filesDir` the Java side uses.
#[allow(unsafe_code)]
pub fn pick_media(activity: usize, data_dir: &Path, timeout: Duration) -> Vec<String> {
    let manifest: PathBuf = data_dir.join(MANIFEST);
    let _ = std::fs::remove_file(&manifest);

    let picked = call(activity, |env, activity| {
        env.call_method(activity, jni_str!("safPickMedia"), jni_sig!("()V"), &[])?;
        let deadline = Instant::now() + timeout;
        loop {
            let done = env.call_method(activity, jni_str!("safIsDone"), jni_sig!("()Z"), &[])?.z().unwrap_or(false);
            if done {
                return Ok(true);
            }
            if Instant::now() > deadline {
                log::warn!("SAF: picker timed out after {timeout:?}");
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });

    match picked {
        Some(true) => read_manifest(&manifest),
        _ => Vec::new(),
    }
}

/// Ask the user where a file should land (`ACTION_CREATE_DOCUMENT`); true when a destination was
/// chosen. The URI stays on the Java side until [`publish`] spends it.
#[allow(unsafe_code)]
pub fn pick_save(activity: usize, title: &str, timeout: Duration) -> bool {
    let result = call(activity, |env, activity| {
        let title = env.new_string(title)?;
        env.call_method(activity, jni_str!("safPickSave"), jni_sig!("(Ljava/lang/String;)V"), &[JValue::Object(&title)])?;
        let deadline = Instant::now() + timeout;
        loop {
            let done = env.call_method(activity, jni_str!("safSaveDone"), jni_sig!("()Z"), &[])?.z().unwrap_or(false);
            if done {
                return env.call_method(activity, jni_str!("safHasSaveUri"), jni_sig!("()Z"), &[])?.z();
            }
            if Instant::now() > deadline {
                log::warn!("SAF: save picker timed out after {timeout:?}");
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
    result.unwrap_or(false)
}

/// Copy a finished file to the destination chosen by [`pick_save`].
#[allow(unsafe_code)]
pub fn publish(activity: usize, path: &str) -> bool {
    let result = call(activity, |env, activity| {
        let path = env.new_string(path)?;
        env.call_method(activity, jni_str!("safPublish"), jni_sig!("(Ljava/lang/String;)Z"), &[JValue::Object(&path)])?.z()
    });
    result.unwrap_or(false)
}

/// Copy `path` to the destination once the app has finished writing it.
///
/// A save runs in the app's own time and there is no completion hook to hang this off, so the file
/// is watched: the copy starts when it stops growing. A failed copy leaves the file behind so
/// nothing is silently lost.
pub fn publish_when_written(activity: usize, path: String) {
    std::thread::Builder::new()
        .name("saf-publish".into())
        .spawn(move || {
            let deadline = Instant::now() + WRITE_TIMEOUT;
            let mut last = 0u64;
            loop {
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                if size > 0 && size == last {
                    break;
                }
                last = size;
                if Instant::now() > deadline {
                    log::warn!("SAF: {path} was never written; nothing to publish");
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            if publish(activity, &path) {
                log::info!("SAF: published {path}");
                let _ = std::fs::remove_file(&path);
            } else {
                log::warn!("SAF: could not publish {path}; it is still in the app directory");
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| log::error!("SAF: could not start the publish thread: {e}"));
}

/// Open `url` in whatever program the device has for it (a browser for a sign-in, or the browser
/// for a link in a message). Runs on the calling thread: the Java side posts to the UI thread.
pub fn open_url(activity: usize, url: &str) -> bool {
    call(activity, |env, activity| {
        let url = env.new_string(url)?;
        let opened = env.call_method(activity, jni_str!("openUrl"), jni_sig!("(Ljava/lang/String;)Z"), &[JValue::Object(&url)])?.z();
        Ok(opened.unwrap_or(false))
    })
    .unwrap_or(false)
}

/// Hand a file to another app: an attachment the owner asked to open. `mime` may be empty.
pub fn open_file(activity: usize, path: &str, mime: &str) -> bool {
    call(activity, |env, activity| {
        let path = env.new_string(path)?;
        let mime = env.new_string(mime)?;
        let opened = env
            .call_method(activity, jni_str!("openFile"), jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Z"), &[
                JValue::Object(&path),
                JValue::Object(&mime),
            ])?
            .z();
        Ok(opened.unwrap_or(false))
    })
    .unwrap_or(false)
}

/// The device's own language tag (`zh-CN`, `en-US`), for an interface language left on Automatic.
pub fn device_locale(activity: usize) -> Option<String> {
    call(activity, |env, activity| {
        let value = env.call_method(activity, jni_str!("deviceLocale"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
        if value.is_null() {
            return Ok(None);
        }
        let string = env.cast_local::<jni::objects::JString>(value)?;
        Ok(Some(string.try_to_string(env)?))
    })
    .flatten()
}

/// The text on the system clipboard.
pub fn clipboard_text(activity: usize) -> Option<String> {
    call(activity, |env, activity| {
        let value = env.call_method(activity, jni_str!("clipboardText"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
        if value.is_null() {
            return Ok(None);
        }
        let string = env.cast_local::<jni::objects::JString>(value)?;
        Ok(Some(string.try_to_string(env)?))
    })
    .flatten()
}

/// Put text on the system clipboard.
pub fn set_clipboard_text(activity: usize, text: &str) -> bool {
    call(activity, |env, activity| {
        let text = env.new_string(text)?;
        env.call_method(activity, jni_str!("setClipboardText"), jni_sig!("(Ljava/lang/String;)V"), &[JValue::Object(&text)])?;
        Ok(true)
    })
    .unwrap_or(false)
}

fn read_manifest(path: &Path) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let paths: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect();
            log::info!("SAF: {} file(s) picked", paths.len());
            paths
        }
        Err(e) => {
            log::info!("SAF: nothing picked ({e})");
            Vec::new()
        }
    }
}
