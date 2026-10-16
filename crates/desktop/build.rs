//! Tauri's build step: generates the context `tauri.conf.json` describes,
//! with the app manifest declaring the app's commands, exactly the bridge's
//! six: each gets its `allow-<name>` permission, and a command not declared
//! here could not be granted (`capabilities/default.json` grants the six).

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&palicanon_app_bridge::COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("tauri-build generates the context");
}
