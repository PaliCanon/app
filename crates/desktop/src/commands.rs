//! The commands the webview may call, and nothing else: their names are
//! the bridge's [`COMMANDS`](palicanon_app_bridge::COMMANDS), their
//! answers its types. Each is a thin call into the native side; the path of
//! every file is the shell's, never the front's.
//!
//! They run off the main thread (`async`): a save waits at most for the
//! write in flight, and that wait must not hold the window.

// Tauri hands a command its managed state by value (`State` is a borrow).
#![allow(clippy::needless_pass_by_value)]

use palicanon_app_bridge::LoadAnswer;
use tauri::State;

use crate::writer::Writer;

/// `progress_load()`: the progress file, read now, I/O only.
#[tauri::command(async)]
pub fn progress_load(writer: State<'_, Writer>) -> LoadAnswer {
    writer.load()
}

/// `progress_save(seq, text)`: the save put in the writer's slot; returns
/// at once, the verdict comes as a `progress-status` event.
///
/// # Errors
/// A text over the 4 MiB cap.
#[tauri::command(async)]
pub fn progress_save(writer: State<'_, Writer>, seq: u64, text: String) -> Result<(), String> {
    writer.save(seq, text)
}
