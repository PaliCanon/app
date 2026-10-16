//! The webview's side of the bridge: the commands through Tauri's IPC
//! (`window.__TAURI_INTERNALS__.invoke`, which the shell injects in every
//! page; no `@tauri-apps/api`, no global `window.__TAURI__`), the
//! `progress-status` event, and the ports the stores use. Browser only.
//!
//! Arguments and answers cross as JSON text, through the bridge's types.

use std::rc::Rc;

use palicanon_app_bridge::{
    EXPORT_FILE, ExportArgs, IMPORT_FILE, ImportAnswer, LoadAnswer, PROGRESS_LOAD, PROGRESS_SAVE,
    PROGRESS_STATUS_EVENT, SaveArgs, StatusEvent,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::files::FilesPort;
use crate::progress::{AppStore, ProgressPort};

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

/// Listens to the `progress-status` event for the whole session: each one
/// applied to `store`. The listener lives as long as the page.
pub fn listen_progress(store: AppStore) {
    let handler = Closure::<dyn Fn(JsValue)>::new(move |event: JsValue| {
        let payload = js_sys::Reflect::get(&event, &"payload".into()).unwrap_or(JsValue::NULL);
        let text = js_sys::JSON::stringify(&payload)
            .ok()
            .and_then(|s| s.as_string())
            .unwrap_or_default();
        match serde_json::from_str::<StatusEvent>(&text) {
            Ok(status) => store.apply(status),
            Err(e) => web_sys::console::warn_1(&format!("progress-status: {e}").into()),
        }
    });
    let id = transform_callback(handler.as_ref().unchecked_ref(), false);
    handler.forget();
    let args = serde_json::json!({
        "event": PROGRESS_STATUS_EVENT,
        "target": { "kind": "Any" },
        "handler": id,
    });
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = invoke::<serde_json::Value>("plugin:event|listen", &args).await {
            web_sys::console::error_1(&format!("cannot listen to progress-status: {e}").into());
        }
    });
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
