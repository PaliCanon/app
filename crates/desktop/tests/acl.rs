//! The webview's door is the fifteen commands (the six of the progress, the
//! files and the search, the six of the sync) and event listen, nothing else.
//! Two proofs.
//!
//! 1. What `tauri-build` generates from this crate's capabilities is one
//!    capability granting exactly those.
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

/// What tauri-build generates from `capabilities/`: one capability, the
/// fifteen commands and event listen.
#[test]
fn the_generated_capabilities_grant_the_fifteen_commands_and_listen() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("gen/schemas/capabilities.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("tauri-build writes {}: {e}", path.display()));
    let capabilities: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let all = capabilities.as_object().expect("an object");
    assert_eq!(all.len(), 1, "{text}");
    let mut granted: Vec<&str> = all["default"]["permissions"]
        .as_array()
        .expect("permissions")
        .iter()
        .map(|p| p.as_str().expect("an identifier"))
        .collect();
    granted.sort_unstable();
    assert_eq!(
        granted,
        [
            "allow-dictionary-lookup",
            "allow-dictionary-passage",
            "allow-dictionary-search",
            "allow-export-file",
            "allow-import-file",
            "allow-progress-load",
            "allow-progress-save",
            "allow-search",
            "allow-search-status",
            "allow-sync-account",
            "allow-sync-cancel",
            "allow-sync-pull",
            "allow-sync-push",
            "allow-sync-sign-in",
            "allow-sync-sign-out",
            "core:event:allow-listen",
        ]
    );
}

#[test]
fn the_webview_cannot_invoke_a_plugin_command() {
    let mut app = app_under_test();
    // `setup` runs with the app's first iteration and builds the window.
    #[allow(
        deprecated,
        reason = "one iteration is the point: run_return would not return"
    )]
    app.run_iteration(|_, _| {});
    let windows = app.webview_windows();
    assert_eq!(windows.len(), 1, "exactly one window: {:?}", windows.keys());
    assert!(windows.contains_key("main"));
    let webview = app.get_webview_window("main").expect("the main window");
    for (cmd, body) in refused() {
        let answer = get_ipc_response(&webview, request(cmd, body));
        let refusal = answer
            .expect_err(&format!("{cmd} must be refused"))
            .to_string();
        assert!(refusal.contains("not allowed"), "{cmd}: {refusal}");
    }
    the_open_door(&webview);
}

fn app_under_test() -> tauri::App<tauri::test::MockRuntime> {
    let data = std::env::temp_dir().join(format!("pc-acl-{}", std::process::id()));
    palicanon_app_desktop::configure(
        mock_builder(),
        Vec::new(),
        palicanon_app_desktop::DataDir::At(data),
        palicanon_app_desktop::SyncChoice::Off,
    )
    .build(tauri::generate_context!())
    .expect("the app builds on the mock runtime")
}

/// The plugin commands the webview must be refused, with plausible bodies.
fn refused() -> Vec<(&'static str, serde_json::Value)> {
    vec![
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
        ("plugin:dialog|open", serde_json::json!({ "options": {} })),
        ("plugin:dialog|save", serde_json::json!({ "options": {} })),
        (
            "plugin:dialog|message",
            serde_json::json!({ "message": "x" }),
        ),
        (
            "plugin:fs|read_text_file",
            serde_json::json!({ "path": "/etc/hosts" }),
        ),
        ("plugin:event|emit", serde_json::json!({ "event": "x" })),
        (
            "plugin:event|unlisten",
            serde_json::json!({ "event": "x", "eventId": 1 }),
        ),
        ("plugin:app|version", serde_json::json!({})),
    ]
}

/// The door that is open: listening to an event (the progress status),
/// and the app's own commands (tests/commands.rs calls them all).
fn the_open_door(webview: &tauri::WebviewWindow<tauri::test::MockRuntime>) {
    let listen = get_ipc_response(
        webview,
        request(
            "plugin:event|listen",
            serde_json::json!({ "event": "progress-status", "target": { "kind": "Any" }, "handler": 7 }),
        ),
    );
    assert!(listen.is_ok(), "{:?}", listen.err().map(|e| e.to_string()));
    let load = get_ipc_response(webview, request("progress_load", serde_json::json!({})));
    assert!(load.is_ok(), "{:?}", load.err().map(|e| e.to_string()));
    let account = get_ipc_response(webview, request("sync_account", serde_json::json!({})));
    assert!(
        account.is_ok(),
        "{:?}",
        account.err().map(|e| e.to_string())
    );
}
