//! The PaliCanon desktop app: a Tauri 2 shell around the front. The window
//! loads the embedded front from `tauri://localhost`; what it may navigate
//! to is decided in one place, [`navigation`]. The webview is given no
//! command and no permission: the only thing Rust does for it is open an
//! `https:` or `mailto:` address in the system browser.

pub mod navigation;
pub mod second_launch;

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Builder, Manager, Runtime, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

/// The label of the one window (`tauri.conf.json`).
const MAIN_WINDOW: &str = "main";

/// Opens `address` in the system browser, from Rust only; the system's
/// refusal comes back as the reason (the navigation rule logs it).
fn open_outside<R: Runtime>(app: &AppHandle<R>, address: &str) -> Result<(), String> {
    app.opener()
        .open_url(address, None::<&str>)
        .map_err(|error| error.to_string())
}

/// The webview's answer to a request for a new window: the navigation rule's
/// answer ([`navigation::allow_new_window`], which is never yes), as Tauri's
/// response. Kept apart from the closure so it is tested.
pub fn new_window_response<R: Runtime>(
    url: &url::Url,
    open: impl FnOnce(&str) -> Result<(), String>,
) -> NewWindowResponse<R> {
    if navigation::allow_new_window(url, open) {
        NewWindowResponse::Allow
    } else {
        NewWindowResponse::Deny
    }
}

/// Builds the one window from the configuration, with the navigation rule
/// on every navigation and every request for a new window.
///
/// # Errors
/// When the runtime cannot create the window or its webview.
///
/// # Panics
/// When the configuration has no `main` window.
pub fn create_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN_WINDOW)
        .expect("the main window is in tauri.conf.json");
    let (for_navigation, for_new_window) = (app.clone(), app.clone());
    WebviewWindowBuilder::from_config(app, config)?
        .on_navigation(move |url| {
            navigation::allow_navigation(url, |address| open_outside(&for_navigation, address))
        })
        .on_new_window(move |url, _features| {
            new_window_response(&url, |address| open_outside(&for_new_window, address))
        })
        .build()?;
    Ok(())
}

/// A second launch brings the running window forward instead of opening
/// another instance; with no window it is logged and ignored.
fn focus_main<R: Runtime>(app: &AppHandle<R>) {
    second_launch::handle_second_launch(app.get_webview_window(MAIN_WINDOW), |window| {
        for step in [window.unminimize(), window.show(), window.set_focus()] {
            if let Err(error) = step {
                log::warn!("second launch: cannot bring the window forward: {error}");
            }
        }
    });
}

/// The app's plugins and its window, on any runtime: what `run` adds to a
/// builder, and what the tests add to a mock one. The webview is given no
/// capability, so the plugins' commands (the opener's, the log's) are not
/// callable from it; `tests/acl.rs` holds that to account.
pub fn configure<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            create_window(app.handle())?;
            Ok(())
        })
}

/// Runs the app.
///
/// # Panics
/// When Tauri cannot start (a missing window configuration, no display).
pub fn run() {
    configure(Builder::default())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main(app);
        }))
        .run(tauri::generate_context!())
        .expect("run the PaliCanon window");
}

#[cfg(test)]
mod tests {
    use tauri::test::MockRuntime;

    use super::*;

    fn response(address: &str, opened: &mut Vec<String>) -> NewWindowResponse<MockRuntime> {
        let url = url::Url::parse(address).expect("an address");
        new_window_response(&url, |a| {
            opened.push(a.to_string());
            Ok(())
        })
    }

    #[test]
    fn every_request_for_a_new_window_is_denied() {
        for (address, opens) in [
            ("https://suttacentral.net/dn1", 1),
            ("mailto:a@b.example", 1),
            ("tauri://localhost/map", 0),
            ("javascript:alert(1)", 0),
            ("file:///etc/hosts", 0),
        ] {
            let mut opened = Vec::new();
            let answer = response(address, &mut opened);
            assert!(matches!(answer, NewWindowResponse::Deny), "{address}");
            assert_eq!(opened.len(), opens, "{address}");
        }
    }
}
