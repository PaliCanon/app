//! The Tauri configuration, read as data: the fixed identity, the window, the
//! CSP, and that the webview is given no command and no permission.

use std::path::Path;

use palicanon_app_identity::{BUNDLE_IDENTIFIER, DISPLAY_NAME};
use serde_json::Value;

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn config() -> Value {
    let text = std::fs::read_to_string(manifest_dir().join("tauri.conf.json"))
        .expect("tauri.conf.json is in the crate");
    serde_json::from_str(&text).expect("tauri.conf.json is JSON")
}

fn csp() -> Vec<(String, Vec<String>)> {
    let config = config();
    let csp = config["app"]["security"]["csp"]
        .as_str()
        .expect("a CSP string");
    csp.split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| {
            let mut parts = d.split_whitespace().map(str::to_string);
            (parts.next().unwrap(), parts.collect())
        })
        .collect()
}

fn directive(name: &str) -> Vec<String> {
    csp()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no {name} directive"))
        .1
}

#[test]
fn the_identity_is_the_fixed_one() {
    let config = config();
    assert_eq!(config["identifier"], BUNDLE_IDENTIFIER);
    assert_eq!(config["productName"], DISPLAY_NAME);
}

#[test]
fn the_window_is_created_in_rust_from_this_entry() {
    // `create: false`: the shell builds the window itself (navigation rule).
    let window = &config()["app"]["windows"][0];
    assert_eq!(window["create"], false);
    assert_eq!(window["label"], "main");
    assert_eq!(window["title"], DISPLAY_NAME);
    assert_eq!(
        (window["width"].as_u64(), window["height"].as_u64()),
        (Some(1100), Some(800))
    );
    assert_eq!(
        (window["minWidth"].as_u64(), window["minHeight"].as_u64()),
        (Some(360), Some(500))
    );
}

#[test]
fn macos_13_is_the_minimum() {
    assert_eq!(config()["bundle"]["macOS"]["minimumSystemVersion"], "13.0");
}

#[test]
fn the_front_is_embedded_from_the_trunk_dist() {
    let config = config();
    assert_eq!(config["build"]["frontendDist"], "../../target/dist");
    let before = config["build"]["beforeBuildCommand"]["script"]
        .as_str()
        .unwrap_or_default();
    assert!(before.starts_with("trunk build --release"), "{before}");
}

#[test]
fn the_csp_allows_only_the_app_itself() {
    assert_eq!(directive("default-src"), ["'self'"]);
    assert_eq!(directive("script-src"), ["'self'", "'wasm-unsafe-eval'"]);
    assert_eq!(directive("style-src"), ["'self'", "'unsafe-inline'"]);
    assert_eq!(directive("connect-src"), ["'self'"]);
    assert_eq!(directive("img-src"), ["'self'", "data:"]);
}

#[test]
fn the_csp_names_no_remote_origin_and_no_eval() {
    for (name, sources) in csp() {
        for source in sources {
            assert!(
                !source.contains("://") && source != "*" && source != "'unsafe-eval'",
                "{name} {source}"
            );
        }
    }
}

#[test]
fn the_bundle_is_the_app_only() {
    assert_eq!(config()["bundle"]["targets"], serde_json::json!(["app"]));
}

#[test]
fn the_webview_has_no_permission() {
    assert!(!manifest_dir().join("capabilities").exists());
    let config = config();
    assert!(config["app"]["security"].get("capabilities").is_none());
    assert!(
        config["app"]
            .get("withGlobalTauri")
            .is_none_or(|v| v == false)
    );
}

#[test]
fn no_command_is_exposed() {
    for file in ["lib.rs", "main.rs", "navigation.rs"] {
        let source = std::fs::read_to_string(manifest_dir().join("src").join(file)).unwrap();
        for forbidden in ["invoke_handler", "#[tauri::command]", "generate_handler"] {
            assert!(!source.contains(forbidden), "{file} has {forbidden}");
        }
    }
}
