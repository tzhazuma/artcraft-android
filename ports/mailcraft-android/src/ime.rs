#![cfg(target_os = "android")]

//! What the soft keyboard typed.
//!
//! winit 0.30's Android backend forwards only key events
//! (`InputEvent::KeyEvent` → `KeyboardInput`); an input method's committed text arrives as
//! `InputEvent::TextEvent`, which that backend ignores — so on a phone, where every character comes
//! through an IME, a text field would open the keyboard and receive nothing. That is winit's gap,
//! not the app's, but an email client cannot live with it.
//!
//! So [`MainActivity`] wraps the activity's `InputConnection` and writes what the input method
//! commits into a file in the app's own directory; the shell drains it at the start of each frame
//! and hands egui the same events a desktop keyboard would have produced. The file, not JNI, is the
//! channel for the same reason the SAF bridge uses one: a write and a rename are hard to get wrong,
//! and a missing method cannot fail silently.
//!
//! Encoding, one operation per line:
//! - `T<text>` — text committed by the input method
//! - `B` — one backspace
//! - `E` — Enter

use std::path::Path;

/// The file the Java side appends to; the shell renames it away before reading it.
pub const FILE: &str = "ime-input.txt";

/// The file the shell reads after taking the one above, so a write racing the read is not lost.
const TAKEN: &str = "ime-input.taken.txt";

/// One thing the keyboard did.
#[derive(Debug, PartialEq, Eq)]
pub enum Op {
    Text(String),
    Backspace,
    Enter,
}

/// What was typed since the last frame (empty when nothing was).
pub fn drain(data_dir: &Path) -> Vec<Op> {
    let path = data_dir.join(FILE);
    if !path.exists() {
        return Vec::new();
    }
    let taken = data_dir.join(TAKEN);
    // Rename first: the input method may append while this frame runs, and it must land in the
    // next frame's file rather than be swallowed by this one's read-then-delete.
    if std::fs::rename(&path, &taken).is_err() {
        return Vec::new();
    }
    let text = match std::fs::read_to_string(&taken) {
        Ok(text) => text,
        Err(e) => {
            log::warn!("mailcraft-android: could not read what the keyboard typed: {e}");
            String::new()
        }
    };
    let _ = std::fs::remove_file(&taken);
    let mut ops = Vec::new();
    for line in text.lines() {
        match line {
            "" => {}
            "B" => ops.push(Op::Backspace),
            "E" => ops.push(Op::Enter),
            other => match other.strip_prefix('T') {
                Some(rest) => ops.push(Op::Text(rest.to_string())),
                None => log::debug!("mailcraft-android: unknown keyboard operation {other:?}"),
            },
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_operations_in_order() {
        let dir = std::env::temp_dir().join(format!("mailcraft-ime-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join(FILE), "Thello\nB\nE\n").expect("write");
        assert_eq!(drain(&dir), vec![Op::Text("hello".into()), Op::Backspace, Op::Enter]);
        // Nothing left: the file was taken and removed.
        assert!(drain(&dir).is_empty());
    }
}
