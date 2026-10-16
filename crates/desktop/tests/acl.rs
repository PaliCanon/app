//! No door: the webview has no command and no permission. Two proofs.
//!
//! 1. What `tauri-build` generates from this crate's capabilities is empty:
//!    no capability grants anything to any window.
//! 2. The real ACL, in the real context, refuses the plugins' commands: the
//!    app is built on Tauri's mock runtime with the app's own `configure`
//!    and `generate_context!()`, and IPC messages as the webview would send
//!    them (from `tauri://localhost`) are answered "not allowed".
//!
//! Both are headless. The live check in the real `WKWebView` (the probe build
//! of the `.app`) is in the task log; a wasm test cannot do it, because the
//! `invoke` bridge exists only inside a Tauri webview.

use std::path::Path;

use tauri::Manager;
use tauri::http::HeaderMap;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder};
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

#[test]
fn the_generated_capabilities_grant_nothing() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("gen/schemas/capabilities.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("tauri-build writes {}: {e}", path.display()));
    let capabilities: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(capabilities, serde_json::json!({}), "{text}");
}

#[test]
fn the_webview_cannot_invoke_a_plugin_command() {
    let app = app_under_test();
    // `setup` runs when the app runs; the window it builds is built here.
    palicanon_app_desktop::create_window(app.handle()).expect("the window");
    let webview = app.get_webview_window("main").expect("the main window");
    for (cmd, body) in [
        (
            "plugin:opener|open_url",
            serde_json::json!({ "url": "https://example.invalid/" }),
        ),
        (
            "plugin:opener|open_path",
            serde_json::json!({ "path": "/" }),
        ),
        (
            "plugin:opener|reveal_item_in_dir",
            serde_json::json!({ "paths": ["/"] }),
        ),
        (
            "plugin:log|log",
            serde_json::json!({ "level": 3, "message": "acl probe" }),
        ),
        ("plugin:window|close", serde_json::json!({})),
        ("plugin:event|emit", serde_json::json!({ "event": "x" })),
    ] {
        let answer = get_ipc_response(&webview, request(cmd, body));
        let refusal = answer
            .expect_err(&format!("{cmd} must be refused"))
            .to_string();
        assert!(refusal.contains("not allowed"), "{cmd}: {refusal}");
    }
}

fn app_under_test() -> tauri::App<tauri::test::MockRuntime> {
    palicanon_app_desktop::configure(mock_builder())
        .build(tauri::generate_context!())
        .expect("the app builds on the mock runtime")
}
