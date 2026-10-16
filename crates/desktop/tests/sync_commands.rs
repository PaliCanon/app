//! The sync commands through the real IPC and ACL, on Tauri's mock runtime,
//! with a sync over a fake Google (`support/fake_google.rs`) and a fake
//! Keychain: the sign-in opens its address once and answers it, its
//! outcome comes as the `sync-sign-in` event (here: cancelled), the
//! account, the pull, the push and the sign-out answer through the IPC.
//! Another binary (`tests/sync_off.rs`) holds a build without sync.

#[path = "support/fake_google.rs"]
mod fake_google;

use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use palicanon_app_bridge::{
    MAX_TEXT_BYTES, PullAnswer, PushAnswer, SYNC_SIGN_IN_EVENT, SignInEvent, SignInStart,
    SyncAccountAnswer, SyncFailure,
};
use palicanon_app_desktop::sync::{Endpoints, MemorySecrets, OAuthClient, SyncService};
use palicanon_app_desktop::{DataDir, SyncChoice};
use tauri::http::HeaderMap;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{Listener, Manager};

use fake_google::FakeGoogle;

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

fn app_over(google: &FakeGoogle, opened: Arc<Mutex<Vec<String>>>) -> tauri::App<MockRuntime> {
    let client = OAuthClient {
        id: "id".into(),
        secret: "secret".into(),
    };
    let service = SyncService::new(
        client,
        Endpoints::at(&google.base),
        Arc::new(MemorySecrets::default()),
    );
    let open = Arc::new(move |url: &str| {
        opened.lock().unwrap().push(url.to_string());
        Ok(())
    });
    let dir = std::env::temp_dir().join(format!("pc-sync-ipc-{}", std::process::id()));
    let mut app = palicanon_app_desktop::configure(
        mock_builder(),
        Vec::new(),
        DataDir::At(dir),
        SyncChoice::Given(service, open),
    )
    .build(tauri::generate_context!())
    .expect("the app builds on the mock runtime");
    #[allow(deprecated, reason = "one iteration runs setup")]
    app.run_iteration(|_, _| {});
    app
}

/// One app per test binary: the log plugin sets the process's logger once.
#[test]
fn the_sync_commands_through_the_real_ipc() {
    let google = tauri::async_runtime::block_on(FakeGoogle::start());
    let opened = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(&google, opened.clone());
    let (events, received) = mpsc::channel();
    app.listen_any(SYNC_SIGN_IN_EVENT, move |event| {
        let _ = events.send(event.payload().to_string());
    });

    let account: SyncAccountAnswer = call(&app, "sync_account", serde_json::json!({}));
    assert_eq!(account, SyncAccountAnswer::SignedOut);

    let start: SignInStart = call(&app, "sync_sign_in", serde_json::json!({}));
    let SignInStart::Started { attempt, url } = start else {
        panic!("a sign-in starts: {start:?}");
    };
    assert!(url.starts_with(&format!("{}/auth?", google.base)), "{url}");
    assert_eq!(*opened.lock().unwrap(), vec![url], "opened once");

    call::<()>(&app, "sync_cancel", serde_json::json!({}));
    let payload = received
        .recv_timeout(Duration::from_secs(5))
        .expect("the sign-in's event");
    let event: SignInEvent = serde_json::from_str(&payload).expect("a sign-in event");
    assert_eq!(
        event,
        SignInEvent {
            attempt,
            outcome: Err(SyncFailure::Cancelled),
        }
    );

    let pull: PullAnswer = call(&app, "sync_pull", serde_json::json!({}));
    assert_eq!(pull, PullAnswer::Failed(SyncFailure::SignedOut));
    let push: PushAnswer = call(&app, "sync_push", serde_json::json!({"text": "{}"}));
    assert_eq!(push, PushAnswer::Failed(SyncFailure::SignedOut));
    let big = "x".repeat(MAX_TEXT_BYTES + 1);
    let push: PushAnswer = call(&app, "sync_push", serde_json::json!({"text": big}));
    assert!(
        matches!(push, PushAnswer::Failed(SyncFailure::Drive(_))),
        "{push:?}"
    );
    call::<()>(&app, "sync_sign_out", serde_json::json!({}));
    assert_eq!(google.seen().len(), 0, "no token: nothing asked of Google");
}
