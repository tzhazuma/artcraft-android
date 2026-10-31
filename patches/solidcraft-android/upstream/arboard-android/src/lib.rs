//! `arboard` for Android: the real crate has no Android backend.
//!
//! arboard 3.6's `src/platform/mod.rs` compiles a backend for Windows, macOS and the X11/Wayland
//! desktops only; on `target_os = "android"` no `platform` module is selected at all, so the crate
//! itself fails to build with
//!
//! ```text
//! error[E0425]: cannot find function `get` in module `platform`
//! error[E0425]: cannot find function `set_text` in module `platform`
//! ```
//!
//! SolidCraft reaches it through `solidcraft-ui-egui`, which declares arboard under
//! `cfg(not(target_arch = "wasm32"))` — true on Android — and uses it for four things: the theme
//! import's Paste button, the text field's right-click menu ("is there text to paste?"), and the
//! agent chat's "attach the clipboard picture" / "attach the files on the clipboard".
//!
//! This shim keeps that public surface and answers like a clipboard with nothing on it, which is
//! exactly what an app with no clipboard integration can honestly say. Nothing here panics or
//! blocks, and every call is cheap.
//!
//! Wiring the real Android clipboard is a separate step (JNI `ClipboardManager` for text; a
//! `content://` URI grant for pictures and files). Until then cross-app copy/paste does not work
//! in the port — in-app Cut/Copy/Paste, which egui handles from its own buffers, does.

use std::borrow::Cow;
use std::path::PathBuf;

/// The errors arboard can report. Only the shape matters to the callers (`and_then`, `.ok()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// There is nothing of that kind on the clipboard.
    ContentNotAvailable,
    /// The clipboard could not be reached at all (no Android backend).
    ClipboardNotSupported,
    /// Anything else.
    Unknown { description: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContentNotAvailable => f.write_str("the clipboard holds nothing of that kind"),
            Self::ClipboardNotSupported => f.write_str("this platform has no clipboard backend"),
            Self::Unknown { description } => f.write_str(description),
        }
    }
}

impl std::error::Error for Error {}

/// A picture on the clipboard: RGBA rows, top to bottom. The lifetime is arboard's.
#[derive(Debug, Clone)]
pub struct ImageData<'a> {
    pub width: usize,
    pub height: usize,
    pub bytes: Cow<'a, [u8]>,
}

impl ImageData<'_> {
    /// The bytes, owned (arboard's own method; call sites use it to detach the picture).
    pub fn into_owned_bytes(self) -> Cow<'static, [u8]> {
        self.bytes.into_owned().into()
    }
}

/// A clipboard. Every instance is empty; it exists so the call sites compile unchanged.
#[derive(Debug, Clone, Copy, Default)]
pub struct Clipboard {
    _private: (),
}

impl Clipboard {
    /// No Android clipboard backend: this always succeeds and always hands back an empty clipboard,
    /// so a caller's `.and_then(|c| c.get_text())` reaches `get_text` and gets the honest
    /// "nothing there" answer.
    pub fn new() -> Result<Self, Error> {
        Ok(Self { _private: () })
    }

    /// The clipboard's text: never any on Android.
    pub fn get_text(&mut self) -> Result<String, Error> {
        Err(Error::ContentNotAvailable)
    }

    /// Put text on the clipboard: not wired (see the module comment).
    pub fn set_text(&mut self, _text: impl Into<Cow<'static, str>>) -> Result<(), Error> {
        Err(Error::ClipboardNotSupported)
    }

    /// The clipboard's picture: never any on Android.
    pub fn get_image(&mut self) -> Result<ImageData<'static>, Error> {
        Err(Error::ContentNotAvailable)
    }

    /// Put a picture on the clipboard: not wired.
    pub fn set_image(&mut self, _image: ImageData<'_>) -> Result<(), Error> {
        Err(Error::ClipboardNotSupported)
    }

    /// Everything on the clipboard at once ([`Get`]). arboard returns this without a `Result`;
    /// the answer to "what is on the clipboard?" is what comes back from the getters below.
    pub fn get(&mut self) -> Get<'_> {
        Get { _clipboard: self }
    }

    /// Clear the clipboard: a no-op.
    pub fn clear(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

/// One read of the clipboard, as [`Clipboard::get`] hands it out.
pub struct Get<'clipboard> {
    _clipboard: &'clipboard mut Clipboard,
}

impl Get<'_> {
    /// The files on the clipboard (a file manager's copy): never any on Android.
    pub fn file_list(self) -> Result<Vec<PathBuf>, Error> {
        Err(Error::ContentNotAvailable)
    }

    /// The picture on the clipboard: never any on Android.
    pub fn image(self) -> Result<ImageData<'static>, Error> {
        Err(Error::ContentNotAvailable)
    }

    /// The text on the clipboard: never any on Android.
    pub fn text(self) -> Result<String, Error> {
        Err(Error::ContentNotAvailable)
    }

    /// The clipboard's HTML: never any on Android.
    pub fn html(self) -> Result<String, Error> {
        Err(Error::ContentNotAvailable)
    }
}
