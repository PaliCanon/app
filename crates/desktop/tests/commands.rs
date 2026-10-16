//! The progress commands through the real IPC and ACL, on Tauri's mock
//! runtime, the progress in a temp directory: a save from the webview
//! reaches the file, a load reads it back, a text over the cap is refused,
//! and the exit event writes the slot.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use palicanon_app_bridge::{LoadAnswer, MAX_TEXT_BYTES};
use palicanon_app_desktop::DataDir;
use palicanon_app_desktop::writer::{PROGRESS_FILE, Writer};
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

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pc-commands-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn app_over(dir: &Path) -> tauri::App<MockRuntime> {
    let mut app =
        palicanon_app_desktop::configure(mock_builder(), Vec::new(), DataDir::At(dir.into()))
            .build(tauri::generate_context!())
            .expect("the app builds on the mock runtime");
    #[allow(deprecated, reason = "one iteration runs setup")]
    app.run_iteration(|_, _| {});
    app
}

fn call(
    app: &tauri::App<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let webview = app.get_webview_window("main").expect("the main window");
    get_ipc_response(&webview, request(cmd, body))
        .map(|body| body.deserialize::<serde_json::Value>().expect("JSON"))
        .map_err(|e| e.to_string())
}

fn wait_for_file(path: &Path, text: &str) {
    let since = Instant::now();
    while std::fs::read_to_string(path).ok().as_deref() != Some(text) {
        assert!(
            since.elapsed() < Duration::from_secs(10),
            "{} never held {text}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// One app per test binary: the log plugin sets the process's logger once.
/// So the scenarios run in order on one app: a save from the webview
/// reaches the file and loads back; a text over the cap is refused by the
/// command, an export over it before any dialog; `RunEvent::Exit` runs the exit write and closes the writer.
#[test]
fn the_progress_commands_through_the_real_ipc() {
    let dir = temp("ipc");
    let app = app_over(&dir);
    let load = |app: &tauri::App<MockRuntime>| -> LoadAnswer {
        serde_json::from_value(call(app, "progress_load", serde_json::json!({})).unwrap()).unwrap()
    };
    assert_eq!(load(&app), LoadAnswer::Missing);
    let text = r#"{"version":1,"progress":[]}"#;
    call(
        &app,
        "progress_save",
        serde_json::json!({"seq": 1, "text": text}),
    )
    .expect("accepted");
    wait_for_file(&dir.join(PROGRESS_FILE), text);
    assert_eq!(load(&app), LoadAnswer::Text(text.into()));

    let big = "x".repeat(MAX_TEXT_BYTES + 1);
    let refusal = call(
        &app,
        "progress_save",
        serde_json::json!({"seq": 2, "text": big}),
    )
    .unwrap_err();
    assert!(refusal.contains("cap"), "{refusal}");

    // An export over the cap is refused before any dialog opens.
    let refusal = call(
        &app,
        "export_file",
        serde_json::json!({"name": "p.json", "text": "x".repeat(MAX_TEXT_BYTES + 1)}),
    )
    .unwrap_err();
    assert!(refusal.contains("over"), "{refusal}");

    palicanon_app_desktop::on_run_event(app.handle(), &tauri::RunEvent::Exit);
    let writer = app.state::<Writer>();
    writer.save(5, "late".into()).unwrap();
    assert!(!writer.write_once(), "nothing is written after Exit");
    assert_eq!(
        std::fs::read_to_string(dir.join(PROGRESS_FILE)).unwrap(),
        text
    );
}
