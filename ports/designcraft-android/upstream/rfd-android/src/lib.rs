//! `rfd` for the Android build of DesignCraft.
//!
//! rfd 0.17.2 has no Android backend: its `FileDialog`/`AsyncFileDialog` impls exist only for
//! macOS, Windows, Linux and the BSDs, so `cargo ndk` fails inside rfd itself with
//! `the trait bound FileDialog: AsyncFilePickerDialogImpl is not satisfied`.
//!
//! DesignCraft 0.14.0 reaches rfd through `craft-dialogs` (a craft-libs crate that wraps rfd's
//! async dialogs in a non-blocking queue). On Android the real file picking goes through the
//! port's `Services` hooks and the Storage Access Framework (`src/saf.rs`); `craft-dialogs` is
//! only the desktop path. This crate keeps that dependency graph compiling by providing the same
//! public surface as rfd 0.17.2, with every dialog answering "cancelled" immediately.
//!
//! It is selected by a `[patch.crates-io]` entry that `upstream/setup-rfd-stub.sh` adds to the
//! upstream workspace root — see that script for why the patch is not committed upstream.

use std::future::{ready, Future};
use std::path::{Path, PathBuf};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

/// A file chosen in a dialog. Never constructed here: no Android dialog can choose one.
#[derive(Debug, Clone, Default)]
pub struct FileHandle {
    path: PathBuf,
}

impl FileHandle {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// rfd's async file dialog builder. Every setter is a no-op and every pick answers `None`, which
/// `craft-dialogs` turns into `Answer::Cancelled`.
#[derive(Debug, Clone, Default)]
pub struct AsyncFileDialog {
    _private: (),
}

impl AsyncFileDialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_filter(self, _name: impl Into<String>, _extensions: &[impl ToString]) -> Self {
        self
    }

    pub fn set_directory<P: AsRef<Path>>(self, _path: P) -> Self {
        self
    }

    pub fn set_file_name(self, _file_name: impl Into<String>) -> Self {
        self
    }

    pub fn set_title(self, _title: impl Into<String>) -> Self {
        self
    }

    pub fn set_parent<W: HasWindowHandle + HasDisplayHandle + ?Sized>(self, _parent: &W) -> Self {
        self
    }

    pub fn set_can_create_directories(self, _can: bool) -> Self {
        self
    }

    pub fn pick_file(self) -> impl Future<Output = Option<FileHandle>> {
        ready(None)
    }

    pub fn pick_files(self) -> impl Future<Output = Option<Vec<FileHandle>>> {
        ready(None)
    }

    pub fn pick_folder(self) -> impl Future<Output = Option<FileHandle>> {
        ready(None)
    }

    pub fn pick_folders(self) -> impl Future<Output = Option<Vec<FileHandle>>> {
        ready(None)
    }

    pub fn pick_file_or_folder(self) -> impl Future<Output = Option<FileHandle>> {
        ready(None)
    }

    pub fn pick_files_or_folders(self) -> impl Future<Output = Option<Vec<FileHandle>>> {
        ready(None)
    }

    pub fn save_file(self) -> impl Future<Output = Option<FileHandle>> {
        ready(None)
    }
}

/// rfd's message-dialog levels.
#[derive(Debug, Clone, Copy, Default)]
pub enum MessageLevel {
    #[default]
    Info,
    Warning,
    Error,
}

/// rfd's message-dialog button sets.
#[derive(Debug, Clone, Default)]
pub enum MessageButtons {
    #[default]
    Ok,
    OkCancel,
    YesNo,
    YesNoCancel,
    OkCustom(String),
    OkCancelCustom(String, String),
    YesNoCancelCustom(String, String, String),
}

/// rfd's message-dialog answers.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum MessageDialogResult {
    Yes,
    No,
    Ok,
    #[default]
    Cancel,
    Custom(String),
}

/// rfd's async message dialog. `show` answers `Cancel` immediately.
#[derive(Debug, Clone, Default)]
pub struct AsyncMessageDialog {
    _private: (),
}

impl AsyncMessageDialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_level(self, _level: MessageLevel) -> Self {
        self
    }

    pub fn set_title(self, _text: impl Into<String>) -> Self {
        self
    }

    pub fn set_description(self, _text: impl Into<String>) -> Self {
        self
    }

    pub fn set_buttons(self, _btn: MessageButtons) -> Self {
        self
    }

    pub fn set_parent<W: HasWindowHandle + HasDisplayHandle + ?Sized>(self, _parent: &W) -> Self {
        self
    }

    pub fn show(self) -> impl Future<Output = MessageDialogResult> {
        ready(MessageDialogResult::Cancel)
    }
}
