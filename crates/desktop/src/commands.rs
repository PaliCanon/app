//! The commands the webview may call, and nothing else: their names are
//! the bridge's [`COMMANDS`](palicanon_app_bridge::COMMANDS), their
//! answers its types. Each is a thin call into the native side; the path of
//! every file is the shell's, never the front's.
//!
//! They run off the main thread (`async`): a save waits at most for the
//! write in flight, and that wait must not hold the window.

// Tauri hands a command its managed state by value (`State` is a borrow).
#![allow(clippy::needless_pass_by_value)]

use palicanon_app_bridge::{ImportAnswer, LoadAnswer};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::files;
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

/// `export_file(name, text)`: the native save dialog, suggesting `name`
/// reduced to a plain `.json` file name; the text written where the learner
/// chose. A failure is shown by a native message dialog; a cancelled dialog
/// writes nothing.
///
/// # Errors
/// A text over the cap (refused before any dialog), or the write's failure.
#[tauri::command]
pub async fn export_file<R: Runtime>(
    app: AppHandle<R>,
    name: String,
    text: String,
) -> Result<(), String> {
    files::check_cap(&text)?;
    let suggested = files::reduce_name(&name);
    let dialogs = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        dialogs
            .dialog()
            .file()
            .set_file_name(&suggested)
            .add_filter("JSON", &["json"])
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(chosen) = chosen else {
        return Ok(());
    };
    let written = chosen
        .into_path()
        .map_err(|e| e.to_string())
        .and_then(|path| files::write_export(&path, &text));
    if let Err(why) = &written {
        show_failure(&app, why);
    }
    written
}

/// `import_file()`: the native open dialog (`*.json`), then the chosen
/// file read, at most 4 MiB.
#[tauri::command]
pub async fn import_file<R: Runtime>(app: AppHandle<R>) -> ImportAnswer {
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("JSON", &["json"])
            .blocking_pick_file()
    })
    .await;
    match chosen {
        Err(e) => ImportAnswer::Failed(e.to_string()),
        Ok(None) => ImportAnswer::Cancelled,
        Ok(Some(chosen)) => match chosen.into_path() {
            Ok(path) => files::read_import(&path),
            Err(e) => ImportAnswer::Failed(e.to_string()),
        },
    }
}

/// A failed export, told by a native message dialog.
fn show_failure<R: Runtime>(app: &AppHandle<R>, why: &str) {
    log::warn!("export failed: {why}");
    app.dialog()
        .message(why)
        .title("PaliCanon")
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}
