//! The commands the webview may call, and nothing else: their names are
//! the bridge's [`COMMANDS`](palicanon_app_bridge::COMMANDS), their
//! answers its types. Each is a thin call into the native side; the path of
//! every file is the shell's, never the front's.
//!
//! They run off the main thread (`async`): a save waits at most for the
//! write in flight, and that wait must not hold the window.

// Tauri hands a command its managed state by value (`State` is a borrow).
#![allow(clippy::needless_pass_by_value)]

use std::sync::Arc;

use palicanon_app_bridge::{
    ImportAnswer, LoadAnswer, PassageAnswer, PullAnswer, PushAnswer, SYNC_SIGN_IN_EVENT,
    SearchAnswer, SearchRequest, SearchStatus, SensesAnswer, SignInStart, SyncAccountAnswer,
    SyncFailure, WordsAnswer,
};
use tauri::{AppHandle, Emitter, Runtime, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::dictionary::Dictionary;
use crate::engine::Engine;
use crate::files;
use crate::sync::SyncHost;
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

/// `search(request)`: one search of the local engine (started at the first
/// request, then kept), off the async runtime's threads.
///
/// # Errors
/// The blocking task could not run.
#[tauri::command]
pub async fn search(
    engine: State<'_, Engine>,
    request: SearchRequest,
) -> Result<SearchAnswer, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.search(&request))
        .await
        .map_err(|e| e.to_string())
}

/// `search_status()`: starts the engine if needed and answers once its
/// model has loaded or failed: whether it serves, and serves meaning.
///
/// # Errors
/// The engine could not start or stopped twice.
#[tauri::command]
pub async fn search_status(engine: State<'_, Engine>) -> Result<SearchStatus, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.status())
        .await
        .map_err(|e| e.to_string())?
}

/// `sync_sign_in()`: a sign-in begins (the one waiting before is
/// cancelled); Google's address is opened once in the system browser and
/// answered, for the app to show. Its outcome comes as a `sync-sign-in`
/// event.
///
/// # Errors
/// Never: a failure is the answer's `Failed`.
#[tauri::command]
pub async fn sync_sign_in<R: Runtime>(
    app: AppHandle<R>,
    host: State<'_, SyncHost>,
) -> Result<SignInStart, String> {
    let Some(service) = host.service.clone() else {
        return Ok(SignInStart::Failed(no_sync()));
    };
    let pending = match service.start_sign_in().await {
        Ok(pending) => pending,
        Err(failure) => return Ok(SignInStart::Failed(failure)),
    };
    if let Err(why) = (host.open)(&pending.url) {
        log::warn!("sync: the browser did not open the sign-in: {why}");
    }
    let finish = pending.finish;
    tauri::async_runtime::spawn(async move {
        let event = finish.await;
        if let Err(error) = app.emit(SYNC_SIGN_IN_EVENT, &event) {
            log::warn!("sync: the sign-in event was not delivered: {error}");
        }
    });
    Ok(SignInStart::Started {
        attempt: pending.attempt,
        url: pending.url,
    })
}

/// What every sync command answers in a build without sync.
fn no_sync() -> SyncFailure {
    SyncFailure::Drive("no sync in this build".into())
}

/// `sync_cancel()`: the sign-in waiting is stopped.
#[tauri::command]
pub fn sync_cancel(host: State<'_, SyncHost>) {
    if let Some(service) = &host.service {
        service.cancel();
    }
}

/// `sync_pull()`: the progress file in the learner's Drive.
///
/// # Errors
/// Never: a failure is the answer's `Failed`.
#[tauri::command]
pub async fn sync_pull(host: State<'_, SyncHost>) -> Result<PullAnswer, String> {
    Ok(match host.service.clone() {
        Some(service) => service.pull().await,
        None => PullAnswer::Failed(no_sync()),
    })
}

/// `sync_push(text)`: `text` written as the progress file.
///
/// # Errors
/// Never: a failure is the answer's `Failed` (a text over the cap too).
#[tauri::command]
pub async fn sync_push(host: State<'_, SyncHost>, text: String) -> Result<PushAnswer, String> {
    if let Err(why) = files::check_cap(&text) {
        return Ok(PushAnswer::Failed(SyncFailure::Drive(why)));
    }
    Ok(match host.service.clone() {
        Some(service) => service.push(&text).await,
        None => PushAnswer::Failed(no_sync()),
    })
}

/// `sync_account()`: whether this build syncs, and who is signed in. No
/// network.
#[tauri::command(async)]
pub fn sync_account(host: State<'_, SyncHost>) -> SyncAccountAnswer {
    match &host.service {
        Some(service) => service.account(),
        None => SyncAccountAnswer::Unavailable,
    }
}

/// `sync_sign_out()`: the token revoked at Google (best effort), then
/// forgotten. The local progress is kept.
///
/// # Errors
/// Never.
#[tauri::command]
pub async fn sync_sign_out(host: State<'_, SyncHost>) -> Result<(), String> {
    if let Some(service) = host.service.clone() {
        service.sign_out().await;
    }
    Ok(())
}

/// `dictionary_search(q)`: the dictionary entries whose headword starts
/// with `q`, off the async runtime's threads (task 008).
///
/// # Errors
/// The blocking task could not run.
#[tauri::command]
pub async fn dictionary_search(
    dictionary: State<'_, Arc<Dictionary>>,
    q: String,
) -> Result<WordsAnswer, String> {
    let dictionary = dictionary.inner().clone();
    tauri::async_runtime::spawn_blocking(move || dictionary.search(&q))
        .await
        .map_err(|e| e.to_string())
}

/// `dictionary_passage(lemma)`: the lemma's precomputed line.
///
/// # Errors
/// The blocking task could not run.
#[tauri::command]
pub async fn dictionary_passage(
    dictionary: State<'_, Arc<Dictionary>>,
    lemma: String,
) -> Result<PassageAnswer, String> {
    let dictionary = dictionary.inner().clone();
    tauri::async_runtime::spawn_blocking(move || dictionary.passage(&lemma))
        .await
        .map_err(|e| e.to_string())
}

/// `dictionary_lookup(form)`: the senses of a word as written.
///
/// # Errors
/// The blocking task could not run.
#[tauri::command]
pub async fn dictionary_lookup(
    dictionary: State<'_, Arc<Dictionary>>,
    form: String,
) -> Result<SensesAnswer, String> {
    let dictionary = dictionary.inner().clone();
    tauri::async_runtime::spawn_blocking(move || dictionary.lookup(&form))
        .await
        .map_err(|e| e.to_string())
}

/// `open_outside(url)`: a page's external link opened in the system browser
/// (task 009), under the navigation rule: `https:` and `mailto:` only, the
/// refusal logged and returned.
///
/// # Errors
/// The address is refused by the rule, or the system would not open it.
#[tauri::command]
pub async fn open_outside<R: Runtime>(app: AppHandle<R>, url: String) -> Result<(), String> {
    let url = crate::navigation::open_request(&url).inspect_err(|why| log::warn!("{why}"))?;
    let address = url.to_string();
    tauri::async_runtime::spawn_blocking(move || crate::open_outside_checked(&app, &address))
        .await
        .map_err(|e| e.to_string())?
        .inspect(|()| log::info!("opened in the system browser: {url}"))
        .inspect_err(|why| log::error!("the system refused to open {url}: {why}"))
}
