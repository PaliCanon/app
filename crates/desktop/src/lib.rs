//! The PaliCanon desktop app: a Tauri 2 shell around the front. The window
//! loads the embedded front from `tauri://localhost`; what it may navigate
//! to is decided in one place, [`navigation`]. The webview is given no
//! command and no permission: the only thing Rust does for it is open an
//! `https:` or `mailto:` address in the system browser.

pub mod navigation;
pub mod second_launch;

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

/// The label of the one window (`tauri.conf.json`).
const MAIN_WINDOW: &str = "main";

/// Opens `address` in the system browser, from Rust only; the system's
/// refusal comes back as the reason (the navigation rule logs it).
fn open_outside(app: &AppHandle, address: &str) -> Result<(), String> {
    app.opener()
        .open_url(address, None::<&str>)
        .map_err(|error| error.to_string())
}

/// Builds the one window from the configuration, with the navigation rule
/// on every navigation and every request for a new window.
fn create_window(app: &AppHandle) -> tauri::Result<()> {
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
            navigation::allow_new_window(&url, |address| open_outside(&for_new_window, address));
            NewWindowResponse::Deny
        })
        .build()?;
    Ok(())
}

/// A second launch brings the running window forward instead of opening
/// another instance; with no window it is logged and ignored.
fn focus_main(app: &AppHandle) {
    second_launch::handle_second_launch(app.get_webview_window(MAIN_WINDOW), |window| {
        for step in [window.unminimize(), window.show(), window.set_focus()] {
            if let Err(error) = step {
                log::warn!("second launch: cannot bring the window forward: {error}");
            }
        }
    });
}

/// Runs the app.
///
/// # Panics
/// When Tauri cannot start (a missing window configuration, no display).
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main(app);
        }))
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
        .run(tauri::generate_context!())
        .expect("run the PaliCanon window");
}
