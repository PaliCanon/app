//! The pins: ONE reading of `cargo metadata --locked --filter-platform
//! wasm32-unknown-unknown`, which gives
//!
//! 1. the lockstep check: exactly one `palicanon-ui-lib` is built, from a git
//!    source at one revision, with `csr` on and `ssr` and `hydrate` off (two
//!    revisions would mean the app and a crate it mounts disagree on the
//!    shell; a path source would mean a sibling checkout);
//! 2. the resolved checkout directories of `palicanon-ui-lib` and of the map
//!    crate, which the CSS and data tasks read files from. Never a sibling
//!    path: whatever the lockfile resolves.

use std::path::PathBuf;

use serde::Deserialize;

/// The platform whose resolution is read: the one the front is built for.
pub const PLATFORM: &str = "wasm32-unknown-unknown";

/// The UI library's crate name.
pub const UI_LIB: &str = "palicanon-ui-lib";

/// The map crate's name, once vizualisation 012 is tagged and the front
/// depends on it. `None` until then: the pins carry no map directory.
pub const MAP_CRATE: Option<&str> = None;

/// What the build reads from the resolution.
#[derive(Debug, PartialEq, Eq)]
pub struct Pins {
    /// The checkout directory of `palicanon-ui-lib` (`style/theme.css`, `src/`).
    pub ui_lib: PathBuf,
    /// The checkout directory of the map crate, when the front depends on it.
    pub map: Option<PathBuf>,
}

/// Why the resolution is not acceptable, or could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum PinsError {
    /// `cargo metadata` failed or its output was not understood.
    Metadata(String),
    /// The package is not built at all.
    Missing(String),
    /// More than one version or source of the package is built.
    Several(String, Vec<String>),
    /// The package comes from a path or a registry, not a git tag.
    NotGit(String, Option<String>),
    /// A feature that must be on is off.
    FeatureOff(String),
    /// A feature that must be off is on.
    FeatureOn(String),
}

impl std::fmt::Display for PinsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Metadata(why) => write!(f, "cargo metadata: {why}"),
            Self::Missing(name) => write!(f, "{name} is not in the resolution"),
            Self::Several(name, sources) => {
                write!(
                    f,
                    "{name} is built {} times: {}",
                    sources.len(),
                    sources.join(", ")
                )
            }
            Self::NotGit(name, source) => {
                write!(f, "{name} is not a git dependency (source: {source:?})")
            }
            Self::FeatureOff(feature) => write!(f, "{UI_LIB} must have `{feature}` on"),
            Self::FeatureOn(feature) => write!(f, "{UI_LIB} must not have `{feature}` on"),
        }
    }
}

impl std::error::Error for PinsError {}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    resolve: Option<Resolve>,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    source: Option<String>,
    manifest_path: String,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    features: Vec<String>,
}

/// Reads `metadata` (the JSON `cargo metadata` prints) and returns the
/// pins, or the first rule the resolution breaks.
///
/// # Errors
/// A [`PinsError`] when the JSON is not metadata, when `palicanon-ui-lib` is
/// absent, built more than once, not from git, or has the wrong features.
pub fn read(metadata: &str, map_crate: Option<&str>) -> Result<Pins, PinsError> {
    let metadata: Metadata =
        serde_json::from_str(metadata).map_err(|e| PinsError::Metadata(e.to_string()))?;
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or_else(|| PinsError::Metadata("no resolve section".into()))?;
    let ui_lib = built_once(&metadata, resolve, UI_LIB)?;
    check_source(ui_lib.0)?;
    check_features(&ui_lib.1.features)?;
    let map = match map_crate {
        Some(name) => Some(directory(built_once(&metadata, resolve, name)?.0)),
        None => None,
    };
    Ok(Pins {
        ui_lib: directory(ui_lib.0),
        map,
    })
}

/// The one package called `name` that the resolution builds, with its node.
fn built_once<'a>(
    metadata: &'a Metadata,
    resolve: &'a Resolve,
    name: &str,
) -> Result<(&'a Package, &'a Node), PinsError> {
    let built: Vec<(&Package, &Node)> = resolve
        .nodes
        .iter()
        .filter_map(|node| {
            let package = metadata.packages.iter().find(|p| p.id == node.id)?;
            (package.name == name).then_some((package, node))
        })
        .collect();
    match built.as_slice() {
        [] => Err(PinsError::Missing(name.into())),
        [one] => Ok(*one),
        many => Err(PinsError::Several(
            name.into(),
            many.iter().map(|(p, _)| p.id.clone()).collect(),
        )),
    }
}

/// ui-lib must come from a git source (a tag), never a path or a registry.
fn check_source(package: &Package) -> Result<(), PinsError> {
    match &package.source {
        Some(source) if source.starts_with("git+") => Ok(()),
        other => Err(PinsError::NotGit(package.name.clone(), other.clone())),
    }
}

/// `csr` on, `ssr` and `hydrate` off.
fn check_features(features: &[String]) -> Result<(), PinsError> {
    for off in ["ssr", "hydrate"] {
        if features.iter().any(|f| f == off) {
            return Err(PinsError::FeatureOn(off.into()));
        }
    }
    if features.iter().any(|f| f == "csr") {
        Ok(())
    } else {
        Err(PinsError::FeatureOff("csr".into()))
    }
}

/// The checkout directory: the manifest's parent.
fn directory(package: &Package) -> PathBuf {
    let manifest = PathBuf::from(&package.manifest_path);
    manifest.parent().map(PathBuf::from).unwrap_or(manifest)
}

/// Runs `cargo metadata --locked` for the browser platform in `workspace`.
///
/// # Errors
/// [`PinsError::Metadata`] when cargo cannot be run or fails (an out-of-date
/// lockfile included: `--locked`).
pub fn run_metadata(workspace: &std::path::Path) -> Result<String, PinsError> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = std::process::Command::new(cargo)
        .current_dir(workspace)
        .args(["metadata", "--locked", "--format-version", "1"])
        .args(["--filter-platform", PLATFORM])
        .output()
        .map_err(|e| PinsError::Metadata(e.to_string()))?;
    if !output.status.success() {
        return Err(PinsError::Metadata(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| PinsError::Metadata(e.to_string()))
}

/// The pins of the workspace at `workspace`: [`run_metadata`], then [`read`]
/// with the map crate this app is built with.
///
/// # Errors
/// Whatever [`run_metadata`] or [`read`] refuses.
pub fn of_workspace(workspace: &std::path::Path) -> Result<Pins, PinsError> {
    read(&run_metadata(workspace)?, MAP_CRATE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const REV_A: &str = "9d07205f701fb7fd4841a6cf411ef1e1d5b5c8dd";
    const REV_B: &str = "1111111111111111111111111111111111111111";

    fn git(rev: &str) -> String {
        format!("git+ssh://git@github.com/PaliCanon/ui-lib.git?tag=v0.3.0#{rev}")
    }

    fn package(name: &str, source: Option<&str>, dir: &str) -> Value {
        let id = format!("{name} {}", source.unwrap_or_default());
        json!({"id": id, "name": name, "source": source,
               "manifest_path": format!("{dir}/Cargo.toml")})
    }

    fn node(name: &str, source: Option<&str>, features: &[&str]) -> Value {
        let id = format!("{name} {}", source.unwrap_or_default());
        json!({"id": id, "features": features})
    }

    /// A resolution with one ui-lib at `rev` and the given features.
    fn metadata_with(features: &[&str], rev: &str, map: bool) -> Value {
        let src = git(rev);
        let mut packages = vec![package(UI_LIB, Some(&src), "/co/ui-lib/9d07205")];
        let mut nodes = vec![node(UI_LIB, Some(&src), features)];
        if map {
            let msrc = "git+ssh://git@github.com/PaliCanon/vizualisation.git?tag=v0.1.0#abc";
            packages.push(package(
                "palicanon-map",
                Some(msrc),
                "/co/vizualisation/abc/app",
            ));
            nodes.push(node("palicanon-map", Some(msrc), &["csr"]));
        }
        json!({"packages": packages, "resolve": {"nodes": nodes}})
    }

    fn read_json(v: &Value, map: Option<&str>) -> Result<Pins, PinsError> {
        read(&v.to_string(), map)
    }

    #[test]
    fn one_ui_lib_with_csr_gives_its_directory() {
        let v = metadata_with(&["csr", "browser"], REV_A, false);
        let pins = read_json(&v, None).expect("pins");
        assert_eq!(pins.ui_lib, PathBuf::from("/co/ui-lib/9d07205"));
        assert_eq!(pins.map, None);
    }

    #[test]
    fn the_map_directory_is_the_map_crates() {
        let v = metadata_with(&["csr"], REV_A, true);
        let pins = read_json(&v, Some("palicanon-map")).expect("pins");
        assert_eq!(pins.map, Some(PathBuf::from("/co/vizualisation/abc/app")));
    }

    #[test]
    fn a_map_crate_that_is_not_built_is_refused() {
        let v = metadata_with(&["csr"], REV_A, false);
        assert_eq!(
            read_json(&v, Some("palicanon-map")),
            Err(PinsError::Missing("palicanon-map".into()))
        );
    }

    #[test]
    fn two_revisions_of_ui_lib_are_refused() {
        let mut v = metadata_with(&["csr"], REV_A, false);
        let other = git(REV_B);
        v["packages"].as_array_mut().unwrap().push(package(
            UI_LIB,
            Some(&other),
            "/co/ui-lib/1111111",
        ));
        v["resolve"]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(node(UI_LIB, Some(&other), &["csr"]));
        let err = read_json(&v, None).unwrap_err();
        assert!(
            matches!(err, PinsError::Several(ref n, ref s) if n == UI_LIB && s.len() == 2),
            "{err}"
        );
    }

    #[test]
    fn ssr_is_refused() {
        let v = metadata_with(&["csr", "ssr"], REV_A, false);
        assert_eq!(read_json(&v, None), Err(PinsError::FeatureOn("ssr".into())));
    }

    #[test]
    fn hydrate_is_refused() {
        let v = metadata_with(&["csr", "hydrate"], REV_A, false);
        assert_eq!(
            read_json(&v, None),
            Err(PinsError::FeatureOn("hydrate".into()))
        );
    }

    #[test]
    fn a_missing_csr_is_refused() {
        let v = metadata_with(&["browser"], REV_A, false);
        assert_eq!(
            read_json(&v, None),
            Err(PinsError::FeatureOff("csr".into()))
        );
    }

    #[test]
    fn a_path_ui_lib_is_refused() {
        let mut v = metadata_with(&["csr"], REV_A, false);
        v["packages"][0]["source"] = Value::Null;
        v["packages"][0]["id"] = json!("palicanon-ui-lib ");
        v["resolve"]["nodes"][0]["id"] = json!("palicanon-ui-lib ");
        assert_eq!(
            read_json(&v, None),
            Err(PinsError::NotGit(UI_LIB.into(), None))
        );
    }

    #[test]
    fn a_registry_ui_lib_is_refused() {
        let mut v = metadata_with(&["csr"], REV_A, false);
        v["packages"][0]["source"] = json!("registry+https://github.com/rust-lang/crates.io-index");
        assert!(matches!(read_json(&v, None), Err(PinsError::NotGit(..))));
    }

    #[test]
    fn no_ui_lib_is_refused() {
        let v = json!({"packages": [], "resolve": {"nodes": []}});
        assert_eq!(read_json(&v, None), Err(PinsError::Missing(UI_LIB.into())));
    }

    /// The lockstep test: the real workspace's resolution for the browser
    /// builds one ui-lib, from git, with `csr` and no `ssr` or `hydrate`.
    #[test]
    fn the_workspace_builds_one_csr_ui_lib() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pins = of_workspace(&root).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            pins.ui_lib.join("style/theme.css").is_file(),
            "{:?}",
            pins.ui_lib
        );
        assert!(pins.ui_lib.join("src").is_dir());
    }

    #[test]
    fn garbage_is_a_metadata_error() {
        assert!(matches!(
            read("not json", None),
            Err(PinsError::Metadata(_))
        ));
        assert!(matches!(
            read("{\"packages\": []}", None),
            Err(PinsError::Metadata(_))
        ));
    }
}
