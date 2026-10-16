//! The pins: ONE reading of `cargo metadata --locked --filter-platform
//! wasm32-unknown-unknown`, which gives
//!
//! 1. the lockstep check: exactly one `palicanon-ui-lib` is built, from a git
//!    source at one revision, with `csr` on and `ssr` and `hydrate` off (two
//!    revisions would mean the app and a crate it mounts disagree on the
//!    shell; a path source would mean a sibling checkout);
//!
//!    `learn_core` and `learn_pali` (Learn and the progress format the
//!    native writer reads) resolve to one revision;
//!
//! 2. the resolved checkout directories of `palicanon-ui-lib`, of the map
//!    crate and of the Learn and Search fragments, which the CSS, data and
//!    assets tasks read files from. Never a sibling path: whatever the
//!    lockfile resolves.
//!
//! The check is over the **wasm32 graph** only (`--filter-platform`): the
//! native graph is not read, and native dev-dependencies (the front's route
//! tests turn Leptos' `ssr` on, natively) may legitimately unify `ssr` there.
//! What the lockstep guarantees is what Trunk builds for the browser.

use std::path::PathBuf;

use serde::Deserialize;

/// The platform whose resolution is read: the one the front is built for.
pub const PLATFORM: &str = "wasm32-unknown-unknown";

/// The UI library's crate name.
pub const UI_LIB: &str = "palicanon-ui-lib";

/// The front's crate name: a member of this workspace, whose directory the CSS
/// reads its sources from (never a literal path).
pub const FRONT: &str = "palicanon-app-front";

/// The map crate's name: vizualisation's fragment, a dependency of the front by
/// tag (`v0.2.0`). Its directory is the repository's `app/`.
pub const MAP_CRATE: Option<&str> = Some("vizualisation");

/// Learn's crate (learn-pali's fragment, by tag); its directory is the
/// repository's `app/`.
pub const LEARN_CRATE: &str = "learn_pali";

/// The progress format's crate, from the same learn-pali tag as Learn.
pub const LEARN_CORE: &str = "learn_core";

/// Search's crate (search-front's fragment, by tag), once the front mounts it.
pub const SEARCH_CRATE: Option<&str> = None;

/// The fragments the front mounts, by crate name.
#[derive(Debug, Clone, Copy, Default)]
pub struct Fragments<'a> {
    /// The map crate.
    pub map: Option<&'a str>,
    /// Learn's crate; with it, `learn_core` must resolve to its revision.
    pub learn: Option<&'a str>,
    /// Search's crate.
    pub search: Option<&'a str>,
}

/// The fragments this app is built with.
pub const FRAGMENTS: Fragments<'static> = Fragments {
    map: MAP_CRATE,
    learn: Some(LEARN_CRATE),
    search: SEARCH_CRATE,
};

/// What the build reads from the resolution.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pins {
    /// The checkout directory of `palicanon-ui-lib` (`style/theme.css`, `src/`).
    pub ui_lib: PathBuf,
    /// The checkout directory of the map crate, when the front depends on it.
    pub map: Option<PathBuf>,
    /// The checkout directory of Learn's crate (`src/`, `public/lotus.png`).
    pub learn: Option<PathBuf>,
    /// The checkout directory of Search's crate (`src/`, `public/brand/`).
    pub search: Option<PathBuf>,
    /// The front's own directory (the workspace member's manifest directory).
    pub front: PathBuf,
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
    /// The package comes from a path or a registry, not git.
    NotGit(String, Option<String>),
    /// The package comes from git, but not at an exact tag's revision (a
    /// branch, a bare rev, or a source without the 40-digit revision the
    /// lockfile records).
    NotTagged(String, String),
    /// Two crates that must come from one revision do not.
    Apart(String, String),
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
            Self::NotTagged(name, source) => {
                write!(f, "{name} is not at a tag's revision (source: {source})")
            }
            Self::Apart(a, b) => write!(f, "{a} and {b} come from different revisions"),
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
    read_with(
        metadata,
        Fragments {
            map: map_crate,
            ..Fragments::default()
        },
    )
}

/// [`read`] with every fragment named: each is held to the git-tag rule and
/// its directory returned; Learn's `learn_core` must come from Learn's
/// revision.
///
/// # Errors
/// As [`read`], and [`PinsError::Apart`] when `learn_core` and Learn's crate
/// resolve to different revisions.
pub fn read_with(metadata: &str, fragments: Fragments<'_>) -> Result<Pins, PinsError> {
    let metadata: Metadata =
        serde_json::from_str(metadata).map_err(|e| PinsError::Metadata(e.to_string()))?;
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or_else(|| PinsError::Metadata("no resolve section".into()))?;
    let ui_lib = built_once(&metadata, resolve, UI_LIB)?;
    check_source(ui_lib.0)?;
    check_features(&ui_lib.1.features)?;
    let pinned = |name: Option<&str>| -> Result<Option<&Package>, PinsError> {
        name.map(|name| {
            let (package, _) = built_once(&metadata, resolve, name)?;
            check_source(package)?;
            Ok(package)
        })
        .transpose()
    };
    let map = pinned(fragments.map)?;
    let learn = pinned(fragments.learn)?;
    if let Some(learn) = learn {
        let (core, _) = built_once(&metadata, resolve, LEARN_CORE)?;
        check_source(core)?;
        if revision(core) != revision(learn) {
            return Err(PinsError::Apart(LEARN_CORE.into(), learn.name.clone()));
        }
    }
    let search = pinned(fragments.search)?;
    let front = directory(built_once(&metadata, resolve, FRONT)?.0);
    Ok(Pins {
        ui_lib: directory(ui_lib.0),
        map: map.map(directory),
        learn: learn.map(directory),
        search: search.map(directory),
        front,
    })
}

/// The 40-digit revision after the `#` of a git source.
fn revision(package: &Package) -> Option<&str> {
    package
        .source
        .as_deref()?
        .rsplit_once('#')
        .map(|(_, rev)| rev)
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

/// A pinned package comes from git at an exact tag: `git+<url>?tag=<tag>#<rev>`
/// with the 40-digit revision the lockfile records. A path, a registry, a
/// branch or a bare revision is refused.
fn check_source(package: &Package) -> Result<(), PinsError> {
    let Some(source) = &package.source else {
        return Err(PinsError::NotGit(package.name.clone(), None));
    };
    let Some(git) = source.strip_prefix("git+") else {
        return Err(PinsError::NotGit(
            package.name.clone(),
            Some(source.clone()),
        ));
    };
    let tagged = git.split_once('#').is_some_and(|(address, rev)| {
        let has_tag = address.split_once('?').is_some_and(|(_, query)| {
            query
                .split('&')
                .any(|part| part.strip_prefix("tag=").is_some_and(|tag| !tag.is_empty()))
        });
        has_tag && rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit())
    });
    if tagged {
        Ok(())
    } else {
        Err(PinsError::NotTagged(package.name.clone(), source.clone()))
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
    run_metadata_with(&cargo, workspace)
}

/// [`run_metadata`] with the `cargo` to run given.
///
/// # Errors
/// As [`run_metadata`].
pub fn run_metadata_with(
    cargo: &std::ffi::OsStr,
    workspace: &std::path::Path,
) -> Result<String, PinsError> {
    run_metadata_in(cargo, workspace, &std::env::temp_dir())
}

/// [`run_metadata_with`] with the directory for its scratch files given. The
/// scratch directory is removed whatever happens: cargo missing, failing, or
/// its output unreadable.
///
/// # Errors
/// As [`run_metadata`].
pub fn run_metadata_in(
    cargo: &std::ffi::OsStr,
    workspace: &std::path::Path,
    base: &std::path::Path,
) -> Result<String, PinsError> {
    static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = base.join(format!("xtask-metadata-{}-{call}", std::process::id()));
    let result = std::fs::create_dir_all(&dir)
        .map_err(|e| PinsError::Metadata(e.to_string()))
        .and_then(|()| metadata_into(cargo, workspace, &dir));
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Runs `cargo metadata` with its output in files of `dir`.
///
/// Output goes to files, not pipes: a daemon that cargo starts and leaves
/// behind (sccache's server, under `RUSTC_WRAPPER`) inherits a pipe and keeps
/// it open, and reading a pipe to its end would wait for the daemon. The
/// wrapper is also switched off: `cargo metadata` compiles nothing.
fn metadata_into(
    cargo: &std::ffi::OsStr,
    workspace: &std::path::Path,
    dir: &std::path::Path,
) -> Result<String, PinsError> {
    let failed = |e: std::io::Error| PinsError::Metadata(e.to_string());
    let (out_path, err_path) = (dir.join("stdout"), dir.join("stderr"));
    let status = std::process::Command::new(cargo)
        .current_dir(workspace)
        .args(["metadata", "--locked", "--format-version", "1"])
        .args(["--filter-platform", PLATFORM])
        .env("RUSTC_WRAPPER", "")
        .env("RUSTC_WORKSPACE_WRAPPER", "")
        .stdin(std::process::Stdio::null())
        .stdout(std::fs::File::create(&out_path).map_err(failed)?)
        .stderr(std::fs::File::create(&err_path).map_err(failed)?)
        .status()
        .map_err(failed)?;
    let stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
    if !status.success() {
        return Err(PinsError::Metadata(stderr.trim().to_string()));
    }
    std::fs::read_to_string(&out_path).map_err(failed)
}

/// The pins of the workspace at `workspace`: [`run_metadata`], then [`read`]
/// with the map crate this app is built with.
///
/// # Errors
/// Whatever [`run_metadata`] or [`read`] refuses.
pub fn of_workspace(workspace: &std::path::Path) -> Result<Pins, PinsError> {
    read_with(&run_metadata(workspace)?, FRAGMENTS)
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
        let mut packages = vec![
            package(UI_LIB, Some(&src), "/co/ui-lib/9d07205"),
            package(FRONT, None, "/ws/crates/front"),
        ];
        let mut nodes = vec![node(UI_LIB, Some(&src), features), node(FRONT, None, &[])];
        if map {
            let msrc = "git+ssh://git@github.com/PaliCanon/vizualisation.git?tag=v0.1.0#2222222222222222222222222222222222222222";
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
    fn the_front_is_the_workspace_members_directory() {
        let v = metadata_with(&["csr"], REV_A, false);
        let pins = read_json(&v, None).expect("pins");
        assert_eq!(pins.front, PathBuf::from("/ws/crates/front"));
    }

    #[test]
    fn a_resolution_without_the_front_is_refused() {
        let mut v = metadata_with(&["csr"], REV_A, false);
        v["packages"].as_array_mut().unwrap().remove(1);
        v["resolve"]["nodes"].as_array_mut().unwrap().remove(1);
        assert_eq!(read_json(&v, None), Err(PinsError::Missing(FRONT.into())));
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

    /// ui-lib's source replaced by `source`, as `cargo metadata` would print it.
    fn with_ui_lib_source(source: &str) -> Value {
        let mut v = metadata_with(&["csr"], REV_A, false);
        v["packages"][0]["source"] = json!(source);
        v["packages"][0]["id"] = json!(format!("{UI_LIB} {source}"));
        v["resolve"]["nodes"][0]["id"] = json!(format!("{UI_LIB} {source}"));
        v
    }

    #[test]
    fn a_branch_a_bare_rev_or_a_short_rev_is_not_a_tag() {
        let rev = REV_A;
        for source in [
            format!("git+ssh://git@github.com/PaliCanon/ui-lib.git?branch=main#{rev}"),
            format!("git+ssh://git@github.com/PaliCanon/ui-lib.git?rev={rev}#{rev}"),
            format!("git+ssh://git@github.com/PaliCanon/ui-lib.git#{rev}"),
            "git+ssh://git@github.com/PaliCanon/ui-lib.git?tag=v0.3.0".to_string(),
            "git+ssh://git@github.com/PaliCanon/ui-lib.git?tag=v0.3.0#9d07205".to_string(),
            format!("git+ssh://git@github.com/PaliCanon/ui-lib.git?tag=#{rev}"),
            format!(
                "git+ssh://git@github.com/PaliCanon/ui-lib.git?tag=v0.3.0#{}",
                "z".repeat(40)
            ),
        ] {
            let v = with_ui_lib_source(&source);
            assert!(
                matches!(read_json(&v, None), Err(PinsError::NotTagged(..))),
                "{source}"
            );
        }
    }

    #[test]
    fn a_tag_with_other_query_parts_is_accepted() {
        let v = with_ui_lib_source(&git(REV_A).replace("?tag=v0.3.0", "?foo=1&tag=v0.3.0"));
        assert!(read_json(&v, None).is_ok());
    }

    #[test]
    fn the_map_crate_is_held_to_the_same_source_rule() {
        for (source, path_source) in [
            (
                Some(
                    "git+ssh://git@github.com/PaliCanon/vizualisation.git?branch=main#2222222222222222222222222222222222222222",
                ),
                false,
            ),
            (
                Some("registry+https://github.com/rust-lang/crates.io-index"),
                true,
            ),
            (None, true),
        ] {
            let mut v = metadata_with(&["csr"], REV_A, true);
            v["packages"][2]["source"] = json!(source);
            let err = read_json(&v, Some("palicanon-map")).unwrap_err();
            if path_source {
                assert!(matches!(err, PinsError::NotGit(..)), "{source:?}: {err}");
            } else {
                assert!(matches!(err, PinsError::NotTagged(..)), "{source:?}: {err}");
            }
        }
    }

    /// `metadata_with` plus Learn's crate and `learn_core`, at the revisions
    /// given.
    fn with_learn(learn_rev: &str, core_rev: &str) -> Value {
        let mut v = metadata_with(&["csr"], REV_A, false);
        for (name, rev, dir) in [
            (LEARN_CRATE, learn_rev, "/co/learn-pali/abc/app"),
            (LEARN_CORE, core_rev, "/co/learn-pali/abc/core"),
        ] {
            let src = format!(
                "git+ssh://git@github.com/PaliCanon/learn-pali.git?tag=fragment-v0.1.0#{rev}"
            );
            v["packages"]
                .as_array_mut()
                .unwrap()
                .push(package(name, Some(&src), dir));
            v["resolve"]["nodes"]
                .as_array_mut()
                .unwrap()
                .push(node(name, Some(&src), &[]));
        }
        v
    }

    fn learn_only() -> Fragments<'static> {
        Fragments {
            learn: Some(LEARN_CRATE),
            ..Fragments::default()
        }
    }

    #[test]
    fn learn_and_its_core_from_one_revision_give_learns_directory() {
        let v = with_learn(REV_A, REV_A);
        let pins = read_with(&v.to_string(), learn_only()).expect("pins");
        assert_eq!(pins.learn, Some(PathBuf::from("/co/learn-pali/abc/app")));
    }

    #[test]
    fn learn_and_its_core_from_two_revisions_are_refused() {
        let v = with_learn(REV_A, REV_B);
        assert_eq!(
            read_with(&v.to_string(), learn_only()),
            Err(PinsError::Apart(LEARN_CORE.into(), LEARN_CRATE.into()))
        );
    }

    #[test]
    fn learn_without_its_core_is_refused() {
        let mut v = with_learn(REV_A, REV_A);
        v["packages"].as_array_mut().unwrap().pop();
        v["resolve"]["nodes"].as_array_mut().unwrap().pop();
        assert_eq!(
            read_with(&v.to_string(), learn_only()),
            Err(PinsError::Missing(LEARN_CORE.into()))
        );
    }

    #[test]
    fn no_ui_lib_is_refused() {
        let v = json!({"packages": [], "resolve": {"nodes": []}});
        assert_eq!(read_json(&v, None), Err(PinsError::Missing(UI_LIB.into())));
    }

    fn pins_with_map(
        ui_lib: &std::path::Path,
        map: &std::path::Path,
        front: &std::path::Path,
    ) -> Pins {
        Pins {
            ui_lib: ui_lib.to_path_buf(),
            map: Some(map.to_path_buf()),
            front: front.to_path_buf(),
            ..Pins::default()
        }
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
        // The map crate is pinned too: its sources, and the data files in the
        // repository's public/ beside its directory, all six.
        let map = pins.map.expect("the front depends on the map crate");
        assert!(map.join("src/fragment.rs").is_file(), "{map:?}");
        // Tailwind scans the pinned checkout's `app/src` (the crate is the
        // repository's `app/`).
        let sources = crate::css::sources(&pins_with_map(&pins.ui_lib, &map, &pins.front));
        assert!(
            sources.iter().any(|s| s.ends_with("app/src")),
            "{sources:?}"
        );
        for file in crate::data::FILES {
            assert!(crate::data::public_dir(&map).join(file).is_file(), "{file}");
        }
        // Learn's checkout: its sources and the brand's lotus.
        let learn = pins.learn.expect("the front depends on Learn");
        assert!(learn.join("src/fragment.rs").is_file(), "{learn:?}");
        assert!(learn.join("public/lotus.png").is_file(), "{learn:?}");
    }

    /// A `cargo` that leaves a daemon behind holding its output, as
    /// sccache's server does when `RUSTC_WRAPPER` starts it (seen: the first
    /// `cargo metadata` after the server's idle exit hung the Trunk build).
    fn cargo_with_a_daemon(name: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("xtask-fake-cargo-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("cargo");
        std::fs::write(&script, "#!/bin/sh\nsleep 8 &\necho '{\"packages\": []}'\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[test]
    fn a_daemon_holding_cargos_output_does_not_hang_the_read() {
        let cargo = cargo_with_a_daemon("daemon");
        let started = std::time::Instant::now();
        let out = run_metadata_with(cargo.as_os_str(), std::path::Path::new("."))
            .expect("the fake cargo succeeds");
        assert!(out.contains("packages"), "{out}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(4),
            "waited for the daemon: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn the_scratch_directory_is_removed_on_every_outcome() {
        let base = std::env::temp_dir().join(format!("xtask-scratch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let leftovers = || std::fs::read_dir(&base).unwrap().count();
        let here = std::path::Path::new(".");
        let ok = cargo_with_a_daemon("scratch-ok");
        assert!(run_metadata_in(ok.as_os_str(), here, &base).is_ok());
        assert_eq!(leftovers(), 0, "after success");
        let missing = run_metadata_in("/nonexistent/cargo".as_ref(), here, &base);
        assert!(missing.is_err());
        assert_eq!(leftovers(), 0, "after a cargo that cannot start");
        let failing = fake_cargo("scratch-fail", "echo oops >&2\nexit 3\n");
        assert!(run_metadata_in(failing.as_os_str(), here, &base).is_err());
        assert_eq!(leftovers(), 0, "after a cargo that fails");
        // A workspace that is not a directory: the spawn fails after the files exist.
        let no_workspace = std::path::Path::new("/nonexistent/workspace");
        assert!(run_metadata_in(ok.as_os_str(), no_workspace, &base).is_err());
        assert_eq!(leftovers(), 0, "after a spawn error");
    }

    /// A `cargo` script with `body` as its commands.
    fn fake_cargo(name: &str, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("xtask-fake-cargo-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("cargo");
        std::fs::write(&script, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[test]
    fn a_failing_cargo_gives_its_stderr() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("xtask-fake-cargo-fail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("cargo");
        std::fs::write(
            &script,
            "#!/bin/sh\necho 'lock needs an update' >&2\nexit 101\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let err = run_metadata_with(script.as_os_str(), std::path::Path::new(".")).unwrap_err();
        assert_eq!(err, PinsError::Metadata("lock needs an update".into()));
        let missing = run_metadata_with("/nonexistent/cargo".as_ref(), std::path::Path::new("."));
        assert!(matches!(missing, Err(PinsError::Metadata(_))));
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
