//! The webview's side of the bridge: the commands through Tauri's IPC
//! (`window.__TAURI_INTERNALS__.invoke`, which the shell injects in every
//! page; no `@tauri-apps/api`, no global `window.__TAURI__`), the
//! `progress-status` event, and the ports the stores use. Browser only.
//!
//! Arguments and answers cross as JSON text, through the bridge's types.

use std::rc::Rc;

use palicanon_app_bridge::{
    DICTIONARY_LOOKUP, DICTIONARY_PASSAGE, DICTIONARY_SEARCH, EXPORT_FILE, ExportArgs, IMPORT_FILE,
    ImportAnswer, LoadAnswer, PROGRESS_LOAD, PROGRESS_SAVE, PROGRESS_STATUS_EVENT, PassageAnswer,
    PullAnswer, PushAnswer, PushArgs, SEARCH, SEARCH_STATUS, SYNC_ACCOUNT, SYNC_CANCEL, SYNC_PULL,
    SYNC_PUSH, SYNC_SIGN_IN, SYNC_SIGN_IN_EVENT, SYNC_SIGN_OUT, SaveArgs, SearchAnswer,
    SearchRequest, SearchStatus, SensesAnswer, SignInEvent, SignInStart, StatusEvent,
    SyncAccountAnswer, SyncFailure, WordsAnswer,
};
use send_wrapper::SendWrapper;
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::dictionary::DictionaryPort;
use crate::files::FilesPort;
use crate::progress::{AppStore, ProgressPort};
use crate::search::{PortFuture, SearchPort};
use crate::sync::{AppSync, SyncPort};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI_INTERNALS__"], js_name = invoke, catch)]
    fn invoke_raw(cmd: &str, args: &JsValue) -> Result<js_sys::Promise, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI_INTERNALS__"], js_name = transformCallback)]
    fn transform_callback(callback: &js_sys::Function, once: bool) -> f64;
}

/// A JS value as text, for an error message.
fn shown(value: &JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            js_sys::JSON::stringify(value)
                .ok()
                .and_then(|s| s.as_string())
        })
        .unwrap_or_else(|| "an unknown error".into())
}

/// Calls the command `cmd` with `args` and reads its answer as `T`.
///
/// # Errors
/// The IPC refused the call, the command answered an error, or the answer
/// is not a `T`.
pub async fn invoke<T: DeserializeOwned>(cmd: &str, args: &impl Serialize) -> Result<T, String> {
    let text = serde_json::to_string(args).map_err(|e| e.to_string())?;
    let args = js_sys::JSON::parse(&text).map_err(|e| shown(&e))?;
    let promise = invoke_raw(cmd, &args).map_err(|e| shown(&e))?;
    let answer = JsFuture::from(promise).await.map_err(|e| shown(&e))?;
    let text = js_sys::JSON::stringify(&answer)
        .ok()
        .and_then(|s| s.as_string())
        .unwrap_or_else(|| "null".into());
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// No arguments.
#[derive(Serialize)]
struct NoArgs {}

/// `progress_load()`: what the store is built from, before the mount.
///
/// # Errors
/// The call failed (the store then answers `Load::Unreadable`).
pub async fn progress_load() -> Result<LoadAnswer, String> {
    invoke(PROGRESS_LOAD, &NoArgs {}).await
}

/// Listens to the shell's event `name` for the whole session, each payload
/// read as a `T` and handed to `apply`. The listener lives as long as the
/// page.
fn listen<T: DeserializeOwned + 'static>(name: &'static str, apply: impl Fn(T) + 'static) {
    let handler = Closure::<dyn Fn(JsValue)>::new(move |event: JsValue| {
        let payload = js_sys::Reflect::get(&event, &"payload".into()).unwrap_or(JsValue::NULL);
        let text = js_sys::JSON::stringify(&payload)
            .ok()
            .and_then(|s| s.as_string())
            .unwrap_or_default();
        match serde_json::from_str::<T>(&text) {
            Ok(value) => apply(value),
            Err(e) => web_sys::console::warn_1(&format!("{name}: {e}").into()),
        }
    });
    let id = transform_callback(handler.as_ref().unchecked_ref(), false);
    handler.forget();
    let args = serde_json::json!({
        "event": name,
        "target": { "kind": "Any" },
        "handler": id,
    });
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = invoke::<serde_json::Value>("plugin:event|listen", &args).await {
            web_sys::console::error_1(&format!("cannot listen to {name}: {e}").into());
        }
    });
}

/// Listens to the `progress-status` event for the whole session: each one
/// applied to `store`.
pub fn listen_progress(store: AppStore) {
    listen::<StatusEvent>(PROGRESS_STATUS_EVENT, move |status| store.apply(status));
}

/// Listens to the `sync-sign-in` event for the whole session: each one
/// applied to `sync`.
pub fn listen_sync(sync: AppSync) {
    listen::<SignInEvent>(SYNC_SIGN_IN_EVENT, move |event| sync.apply(event));
}

/// `sync_account()`: whether this build syncs, and who is signed in; read
/// before the mount. A failed call is no sync.
pub async fn sync_account() -> SyncAccountAnswer {
    invoke(SYNC_ACCOUNT, &NoArgs {})
        .await
        .unwrap_or(SyncAccountAnswer::Unavailable)
}

/// Calls `cmd` and hands its answer to `done`; a failed call is `failed`.
fn answer<T: DeserializeOwned + 'static>(
    cmd: &'static str,
    args: impl Serialize + 'static,
    failed: impl FnOnce(SyncFailure) -> T + 'static,
    done: Box<dyn FnOnce(T)>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let answer = invoke::<T>(cmd, &args).await.unwrap_or_else(|e| {
            web_sys::console::warn_1(&format!("{cmd}: {e}").into());
            failed(SyncFailure::Network)
        });
        done(answer);
    });
}

/// Calls `cmd`, its answer only logged when it fails.
fn fire(cmd: &'static str) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = invoke::<()>(cmd, &NoArgs {}).await {
            web_sys::console::warn_1(&format!("{cmd}: {e}").into());
        }
    });
}

/// The sync, to the shell's six sync commands.
pub struct TauriSync;

impl SyncPort for TauriSync {
    fn sign_in(&self, done: Box<dyn FnOnce(SignInStart)>) {
        answer(SYNC_SIGN_IN, NoArgs {}, SignInStart::Failed, done);
    }

    fn cancel(&self) {
        fire(SYNC_CANCEL);
    }

    fn pull(&self, done: Box<dyn FnOnce(PullAnswer)>) {
        answer(SYNC_PULL, NoArgs {}, PullAnswer::Failed, done);
    }

    fn push(&self, text: String, done: Box<dyn FnOnce(PushAnswer)>) {
        answer(SYNC_PUSH, PushArgs { text }, PushAnswer::Failed, done);
    }

    fn account(&self, done: Box<dyn FnOnce(SyncAccountAnswer)>) {
        answer(
            SYNC_ACCOUNT,
            NoArgs {},
            |_| SyncAccountAnswer::Unavailable,
            done,
        );
    }

    fn sign_out(&self) {
        fire(SYNC_SIGN_OUT);
    }
}

/// The saves, to `progress_save`.
pub struct TauriProgress;

impl ProgressPort for TauriProgress {
    fn save(&self, seq: u64, text: String) {
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = invoke::<()>(PROGRESS_SAVE, &SaveArgs { seq, text }).await {
                web_sys::console::error_1(&format!("progress_save {seq}: {e}").into());
            }
        });
    }
}

/// The files, to `export_file` and `import_file`.
pub struct TauriFiles;

impl FilesPort for TauriFiles {
    fn export(&self, name: String, text: String) {
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = invoke::<()>(EXPORT_FILE, &ExportArgs { name, text }).await {
                web_sys::console::warn_1(&format!("export_file: {e}").into());
            }
        });
    }

    fn import(&self, done: Box<dyn FnOnce(ImportAnswer)>) {
        wasm_bindgen_futures::spawn_local(async move {
            let answer = invoke::<ImportAnswer>(IMPORT_FILE, &NoArgs {})
                .await
                .unwrap_or_else(ImportAnswer::Failed);
            done(answer);
        });
    }
}

/// The app's two ports, shared.
pub fn ports() -> (Rc<dyn ProgressPort>, Rc<dyn FilesPort>) {
    (Rc::new(TauriProgress), Rc::new(TauriFiles))
}

/// `search`'s arguments: the request under its parameter's name.
#[derive(Serialize)]
struct SearchArgs {
    request: SearchRequest,
}

/// The local engine, through `search` and `search_status`. The webview's
/// futures are not `Send`: they are wrapped (the page's resource needs
/// `Send`, and runs them on this one thread).
pub struct TauriSearch;

impl SearchPort for TauriSearch {
    fn search(&self, request: SearchRequest) -> PortFuture<SearchAnswer> {
        Box::pin(SendWrapper::new(async move {
            invoke(SEARCH, &SearchArgs { request }).await
        }))
    }

    fn status(&self) -> PortFuture<SearchStatus> {
        Box::pin(SendWrapper::new(async move {
            invoke(SEARCH_STATUS, &NoArgs {}).await
        }))
    }
}

/// The desktop's dictionary commands (task 008), each answered once.
pub struct TauriDictionary;

#[derive(Serialize)]
struct QueryArgs {
    q: String,
}

#[derive(Serialize)]
struct LemmaArgs {
    lemma: String,
}

#[derive(Serialize)]
struct FormArgs {
    form: String,
}

/// Invokes `cmd` and hands its answer, or the failure's text, to `done`.
fn ask<T: DeserializeOwned + 'static>(
    cmd: &'static str,
    args: impl Serialize + 'static,
    done: Box<dyn FnOnce(Result<T, String>)>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let answer = invoke::<T>(cmd, &args).await;
        if let Err(e) = &answer {
            web_sys::console::warn_1(&format!("{cmd}: {e}").into());
        }
        done(answer);
    });
}

impl DictionaryPort for TauriDictionary {
    fn search(&self, q: String, done: Box<dyn FnOnce(Result<WordsAnswer, String>)>) {
        ask(DICTIONARY_SEARCH, QueryArgs { q }, done);
    }

    fn passage(&self, lemma: String, done: Box<dyn FnOnce(Result<PassageAnswer, String>)>) {
        ask(DICTIONARY_PASSAGE, LemmaArgs { lemma }, done);
    }

    fn lookup(&self, form: String, done: Box<dyn FnOnce(Result<SensesAnswer, String>)>) {
        ask(DICTIONARY_LOOKUP, FormArgs { form }, done);
    }
}
