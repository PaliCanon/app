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

/// The CSP as `directive -> sources`.
fn csp() -> std::collections::BTreeMap<String, Vec<String>> {
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

/// An allowlist: exactly these five directives with exactly these sources.
/// A sixth directive, or one more source in any of them, fails.
#[test]
fn the_csp_is_exactly_the_designs_five_directives() {
    let expected: std::collections::BTreeMap<String, Vec<String>> = [
        ("default-src", vec!["'self'"]),
        ("script-src", vec!["'self'", "'wasm-unsafe-eval'"]),
        ("style-src", vec!["'self'", "'unsafe-inline'"]),
        ("connect-src", vec!["'self'"]),
        ("img-src", vec!["'self'", "data:"]),
    ]
    .into_iter()
    .map(|(name, sources)| {
        (
            name.to_string(),
            sources.into_iter().map(str::to_string).collect(),
        )
    })
    .collect();
    assert_eq!(csp(), expected);
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

/// The names in `generate_handler![...]` in `lib.rs`, without their module.
fn handler_names() -> Vec<String> {
    let source = std::fs::read_to_string(manifest_dir().join("src/lib.rs")).unwrap();
    let start = source
        .find("generate_handler![")
        .expect("lib.rs registers its commands")
        + "generate_handler![".len();
    let end = start + source[start..].find(']').expect("the list ends");
    source[start..end]
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| name.rsplit("::").next().unwrap().to_string())
        .collect()
}

/// The webview's commands are the bridge's, each once, registered in one
/// place: no other file registers a handler.
#[test]
fn the_commands_are_the_bridges_and_registered_once() {
    let names = handler_names();
    for name in &names {
        assert!(
            palicanon_app_bridge::COMMANDS.contains(&name.as_str()),
            "{name} is not a bridge command"
        );
        assert_eq!(names.iter().filter(|n| *n == name).count(), 1, "{name}");
    }
    for file in [
        "main.rs",
        "navigation.rs",
        "writer.rs",
        "disk.rs",
        "commands.rs",
    ] {
        let source = std::fs::read_to_string(manifest_dir().join("src").join(file)).unwrap();
        for forbidden in ["invoke_handler", "generate_handler"] {
            assert!(!source.contains(forbidden), "{file} has {forbidden}");
        }
    }
}
