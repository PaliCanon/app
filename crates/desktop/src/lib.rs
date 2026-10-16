//! The PaliCanon desktop app: a Tauri 2 shell around the front. The window
//! loads the embedded front from `tauri://localhost`; what it may navigate
//! to is decided in one place, [`navigation`]. The webview may call the
//! app's own commands ([`commands`], the bridge's names) and nothing else:
//! the progress is kept and written by the native [`writer`], the files and
//! the search are the shell's; an `https:` or `mailto:` address opens in the
//! system browser.

pub mod commands;
pub mod disk;
pub mod engine;
pub mod files;
pub mod navigation;
pub mod second_launch;
pub mod writer;

use std::path::PathBuf;

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Builder, Emitter, Manager, RunEvent, Runtime, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

use crate::writer::Writer;

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

/// Where the progress lives: the app data directory of the app's identifier,
/// or a given directory (a test's temp dir: a test never touches the
/// user's progress).
#[derive(Debug, Clone)]
pub enum DataDir {
    /// `<app data dir of app.palicanon.learn>`.
    App,
    /// This directory.
    At(PathBuf),
}

/// The progress writer over `dir`, its verdicts emitted to the webview as
/// the bridge's status event; an emit that fails (the webview gone) is
/// logged, nothing else.
fn progress_writer<R: Runtime>(app: &AppHandle<R>, dir: PathBuf) -> Writer {
    let emitter = app.clone();
    Writer::new(
        dir,
        disk::RealDisk,
        Box::new(move |event| {
            if let Err(error) = emitter.emit(palicanon_app_bridge::PROGRESS_STATUS_EVENT, event) {
                log::warn!("progress: the status event was not delivered: {error}");
            }
        }),
        writer::RETRY,
    )
}

/// The local search engine: `yuga-sidecar` beside the app's executable
/// (Tauri's `externalBin`, `Contents/MacOS/` in the bundle), the store and
/// the model in the bundle's resources (`yuga/store`, `yuga/model`), read
/// only. Nothing starts until the first search or status; a missing file
/// makes the start fail, and Search answers `Failed`.
fn search_engine<R: Runtime>(app: &AppHandle<R>) -> engine::Engine {
    let resources = app.path().resource_dir().unwrap_or_default();
    let binary = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(engine::BINARY)))
        .unwrap_or_default();
    engine::Engine::new(
        engine::SidecarSpawn {
            binary,
            store: resources.join("yuga/store"),
            model: resources.join("yuga/model"),
        },
        engine::POLL,
    )
}

/// What the app does on `RunEvent::Exit` (Cmd-Q, the last window closed):
/// the progress writer's exit write, then the search engine killed.
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: &RunEvent) {
    if let RunEvent::Exit = event {
        if let Some(writer) = app.try_state::<Writer>() {
            writer.exit();
        }
        if let Some(engine) = app.try_state::<engine::Engine>() {
            engine.shutdown();
        }
    }
}

/// The app's plugins, its commands, its progress writer and its window, on
/// any runtime: what `run` adds to a builder, and what the tests add to a
/// mock one. The webview may call the app's commands only, never a
/// plugin's; `tests/acl.rs` holds that to account.
///
/// `log_targets` is where the app's log goes: [`default_log_targets`] in the
/// app, none in a test (a test must not write into the user's log).
/// `data` is where the progress lives.
pub fn configure<R: Runtime>(
    builder: Builder<R>,
    log_targets: Vec<tauri_plugin_log::Target>,
    data: DataDir,
) -> Builder<R> {
    builder
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .clear_targets()
                .targets(log_targets)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::progress_load,
            commands::progress_save,
            commands::export_file,
            commands::import_file,
            commands::search,
            commands::search_status,
        ])
        .setup(move |app| {
            let dir = match data {
                DataDir::App => app.path().app_data_dir()?,
                DataDir::At(dir) => dir,
            };
            let writer = progress_writer(app.handle(), dir);
            writer.spawn();
            app.manage(writer);
            app.manage(search_engine(app.handle()));
            create_window(app.handle())?;
            Ok(())
        })
}

/// Where the app's log goes: the terminal it was started from, and the log
/// directory of the app's identifier (`~/Library/Logs/app.palicanon.learn/`).
pub fn default_log_targets() -> Vec<tauri_plugin_log::Target> {
    use tauri_plugin_log::{Target, TargetKind};
    vec![
        Target::new(TargetKind::Stdout),
        Target::new(TargetKind::LogDir { file_name: None }),
    ]
}

/// Runs the app.
///
/// # Panics
/// When Tauri cannot start (a missing window configuration, no display).
pub fn run() {
    // The single-instance plugin first, as its documentation asks.
    let builder =
        Builder::default().plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main(app);
        }));
    configure(builder, default_log_targets(), DataDir::App)
        .build(tauri::generate_context!())
        .expect("build the PaliCanon app")
        .run(|app, event| on_run_event(app, &event));
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
