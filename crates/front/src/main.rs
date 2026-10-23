//! The browser entry: Trunk builds this binary to wasm and runs it. The
//! progress is read first (`progress_load`) and the store built from it, so
//! Learn's first screen already has it; then ui-lib's `csr::mount` mounts
//! the app in the empty `<body>`, with the theme and the language kept in
//! `localStorage`. The store follows the writer's `progress-status` events
//! for the whole session. The sync is read too (`sync_account`): a build
//! without it mounts no sync UI; a refresh token in the Keychain makes the
//! account signed in at once, so Learn syncs at load.

fn main() {
    console_error_panic_hook::set_once();
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(start());
}

#[cfg(target_arch = "wasm32")]
async fn start() {
    use leptos::prelude::*;
    use palicanon_app_front::app::App;
    use palicanon_app_front::dictionary::AppDictionary;
    use palicanon_app_front::files::AppFiles;
    use palicanon_app_front::progress::AppStore;
    use palicanon_app_front::search::AppSearch;
    use palicanon_app_front::sync::AppSync;
    use palicanon_app_front::tauri;

    let answer = tauri::progress_load().await;
    let (progress, files) = tauri::ports();
    let store = AppStore::new(answer, progress);
    tauri::listen_progress(store.clone());
    let files = AppFiles::new(files);
    let search = AppSearch::new(std::sync::Arc::new(tauri::TauriSearch));
    let dictionary = AppDictionary::new(std::rc::Rc::new(tauri::TauriDictionary));
    let sync = AppSync::new(
        tauri::sync_account().await,
        std::rc::Rc::new(tauri::TauriSync),
    );
    if let Some(sync) = &sync {
        tauri::listen_sync(sync.clone());
    }
    palicanon_ui_lib::csr::mount(move || {
        view! {
            <App store=store files=files search=search dictionary=dictionary sync=sync.clone() />
        }
    });
}
