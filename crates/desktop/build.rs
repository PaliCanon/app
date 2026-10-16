//! Tauri's build step: generates the context `tauri.conf.json` describes,
//! with the app manifest declaring the app's commands, exactly the bridge's
//! twelve: each gets its `allow-<name>` permission, and a command not
//! declared here could not be granted (`capabilities/default.json` grants
//! the twelve).
//!
//! Then the sync's secret (task 006): the Desktop OAuth client's secret is
//! compiled in from `PALICANON_GOOGLE_DESKTOP_SECRET` (`option_env!` in
//! `src/sync/mod.rs`). When that variable is not set, it is read here from
//! the build machine's Keychain (`security find-generic-password -a
//! palicanon -s google-oauth-desktop-secret -w`) and handed to the compiler
//! only, never written to the repository, never printed by this script
//! (it is the last line the script emits, after every step that could
//! fail). `PALICANON_NO_SYNC=1` builds without it. A machine without the
//! item (CI) builds an app with no sync. A secret rotated in the Keychain
//! is taken by the next `cargo clean -p palicanon-app-desktop` build.

use std::process::{Command, Stdio};

/// The variable `option_env!` reads.
const SECRET_VARIABLE: &str = "PALICANON_GOOGLE_DESKTOP_SECRET";

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&palicanon_app_bridge::COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("tauri-build generates the context");
    println!("cargo:rerun-if-env-changed={SECRET_VARIABLE}");
    println!("cargo:rerun-if-env-changed=PALICANON_NO_SYNC");
    if std::env::var_os(SECRET_VARIABLE).is_some()
        || std::env::var_os("PALICANON_NO_SYNC").is_some()
    {
        return;
    }
    if let Some(secret) = keychain_secret() {
        println!("cargo:rustc-env={SECRET_VARIABLE}={secret}");
    }
}

/// The secret in the build machine's Keychain; none without the item (or
/// without `security`, off macOS).
fn keychain_secret() -> Option<String> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-a",
            "palicanon",
            "-s",
            "google-oauth-desktop-secret",
            "-w",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let secret = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!secret.is_empty() && !secret.contains(['\n', '\r'])).then_some(secret)
}
