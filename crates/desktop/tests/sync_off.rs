//! A build without sync (no OAuth secret): through the real IPC and ACL,
//! `sync_account` answers `Unavailable` (the front then shows no sync UI)
//! and every other sync command refuses, opening nothing.

use palicanon_app_bridge::{PullAnswer, PushAnswer, SignInStart, SyncAccountAnswer};
use palicanon_app_desktop::{DataDir, SyncChoice};
use tauri::Manager;
use tauri::http::HeaderMap;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;

fn request(cmd: &str, body: serde_json::Value) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.to_string(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().expect("an address"),
        body: InvokeBody::Json(body),
        headers: HeaderMap::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

fn call<T: serde::de::DeserializeOwned>(
    app: &tauri::App<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> T {
    let webview = app.get_webview_window("main").expect("the main window");
    let answer = get_ipc_response(&webview, request(cmd, body))
        .unwrap_or_else(|e| panic!("{cmd} answers: {e}"));
    answer.deserialize::<T>().expect("the bridge's type")
}

#[test]
fn a_build_without_sync_answers_unavailable_and_refuses_the_rest() {
    let dir = std::env::temp_dir().join(format!("pc-sync-off-{}", std::process::id()));
    let mut app = palicanon_app_desktop::configure(
        mock_builder(),
        Vec::new(),
        DataDir::At(dir),
        SyncChoice::Off,
    )
    .build(tauri::generate_context!())
    .expect("the app builds on the mock runtime");
    #[allow(deprecated, reason = "one iteration runs setup")]
    app.run_iteration(|_, _| {});
    let account: SyncAccountAnswer = call(&app, "sync_account", serde_json::json!({}));
    assert_eq!(account, SyncAccountAnswer::Unavailable);
    let start: SignInStart = call(&app, "sync_sign_in", serde_json::json!({}));
    assert!(matches!(start, SignInStart::Failed(_)), "{start:?}");
    let pull: PullAnswer = call(&app, "sync_pull", serde_json::json!({}));
    assert!(matches!(pull, PullAnswer::Failed(_)), "{pull:?}");
    let push: PushAnswer = call(&app, "sync_push", serde_json::json!({"text": "{}"}));
    assert!(matches!(push, PushAnswer::Failed(_)), "{push:?}");
    call::<()>(&app, "sync_cancel", serde_json::json!({}));
    call::<()>(&app, "sync_sign_out", serde_json::json!({}));
}
