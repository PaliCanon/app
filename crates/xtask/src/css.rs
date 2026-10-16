//! `xtask css`: the app's Tailwind CSS. The input stylesheet is written from
//! the pins (ui-lib's theme, and the sources whose classes are generated:
//! ui-lib's, the map crate's and the front's), so no path is a sibling
//! checkout; a source that does not exist fails the task (Tailwind would
//! silently generate nothing for it), and the built CSS must hold a class
//! only a given source uses.

use std::path::{Path, PathBuf};

use crate::pins::Pins;

/// A class only ui-lib's sources use (its shell's page column): a CSS
/// without it did not read ui-lib's `src`.
pub const UI_LIB_MARKER: &str = ".min-h-screen";

/// A class only the map crate's sources use (the translators' table collapses its
/// borders; neither ui-lib nor the front uses it): a CSS without it did not
/// read the map's `src`.
pub const MAP_MARKER: Option<&str> = Some(".border-collapse");

/// Why the CSS could not be built.
#[derive(Debug, PartialEq, Eq)]
pub enum CssError {
    /// A path the input names does not exist.
    MissingSource(PathBuf),
    /// The built CSS lacks a class that a source must have contributed.
    MissingClass(String),
    /// No Tailwind CLI was found.
    NoTailwind,
    /// The CLI failed, or a file could not be read or written.
    Failed(String),
}

impl std::fmt::Display for CssError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSource(path) => write!(f, "{} does not exist", path.display()),
            Self::MissingClass(class) => {
                write!(f, "the built CSS has no `{class}`: a source was not read")
            }
            Self::NoTailwind => write!(f, "no Tailwind CLI: set TAILWIND to its path"),
            Self::Failed(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for CssError {}

/// The directories Tailwind scans for classes, in order.
pub fn sources(pins: &Pins) -> Vec<PathBuf> {
    let mut sources = vec![pins.ui_lib.join("src")];
    if let Some(map) = &pins.map {
        sources.push(map.join("src"));
    }
    sources.push(pins.front.join("src"));
    sources
}

/// The Tailwind input stylesheet for `pins`, written with absolute paths.
///
/// # Errors
/// [`CssError::MissingSource`] when ui-lib's theme or a source directory is
/// not there.
pub fn input(pins: &Pins) -> Result<String, CssError> {
    let theme = pins.ui_lib.join("style/theme.css");
    let sources = sources(pins);
    for path in std::iter::once(&theme).chain(&sources) {
        if !path.exists() {
            return Err(CssError::MissingSource(path.clone()));
        }
    }
    let mut lines = vec![
        "@import \"tailwindcss\" source(none);".to_string(),
        format!("@import \"{}\";", theme.display()),
    ];
    lines.extend(
        sources
            .iter()
            .map(|s| format!("@source \"{}\";", s.display())),
    );
    Ok(lines.join("\n") + "\n")
}

/// The classes the built CSS must hold for `pins`.
pub fn markers(pins: &Pins) -> Vec<&'static str> {
    markers_with(pins, MAP_MARKER)
}

/// [`markers`] with the map crate's marker given: it is required when, and
/// only when, `pins` has a map crate.
pub fn markers_with(pins: &Pins, map_marker: Option<&'static str>) -> Vec<&'static str> {
    let mut markers = vec![UI_LIB_MARKER];
    if pins.map.is_some() {
        markers.extend(map_marker);
    }
    markers
}

/// Checks that the built `css` holds every class in `markers`.
///
/// # Errors
/// [`CssError::MissingClass`] naming the first one it lacks.
pub fn check_built(css: &str, markers: &[&str]) -> Result<(), CssError> {
    match markers.iter().find(|marker| !css.contains(**marker)) {
        Some(missing) => Err(CssError::MissingClass((*missing).to_string())),
        None => Ok(()),
    }
}

/// The Tailwind CLI from the process environment: `$TAILWIND`, else the
/// newest one `cargo-leptos` cached under `$HOME`.
///
/// # Errors
/// [`CssError::NoTailwind`] when there is none.
pub fn tailwind_binary() -> Result<PathBuf, CssError> {
    find_tailwind(
        std::env::var_os("TAILWIND"),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

/// [`tailwind_binary`] with the environment given: `tailwind` is `$TAILWIND`
/// (an empty value counts as unset), `home` is `$HOME`.
///
/// # Errors
/// [`CssError::NoTailwind`] without a variable, a home, or a cached CLI.
pub fn find_tailwind(
    tailwind: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
) -> Result<PathBuf, CssError> {
    if let Some(path) = tailwind.filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = home.ok_or(CssError::NoTailwind)?;
    let mut found: Vec<PathBuf> = Vec::new();
    for cache in [
        home.join("Library/Caches/cargo-leptos"),
        home.join(".cache/cargo-leptos"),
    ] {
        collect(&cache, 3, &mut found);
    }
    // The newest by version, numerically: v4.10 comes after v4.9.
    found.sort_by_cached_key(|p| (version_of(p), p.clone()));
    found.pop().ok_or(CssError::NoTailwind)
}

/// The numbers of the version in a cached CLI's directory name
/// (`tailwindcss-v4.10.0` gives `[4, 10, 0]`; none gives nothing).
fn version_of(cli: &Path) -> Vec<u64> {
    let directory = cli
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    directory
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// Adds to `found` the files named `tailwindcss-*` at most `depth` levels
/// under `dir` (the cache holds `tailwindcss-v4/tailwindcss-v4/tailwindcss-<os>`).
fn collect(dir: &Path, depth: u8, found: &mut Vec<PathBuf>) {
    for path in read_dir(dir) {
        let is_tailwind = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("tailwindcss-"));
        if path.is_file() && is_tailwind {
            found.push(path);
        } else if path.is_dir() && depth > 1 {
            collect(&path, depth - 1, found);
        }
    }
}

/// The entries of `dir` (none when it cannot be read).
fn read_dir(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default()
}

/// Builds the app's CSS into `<root>/target/css/app.css` with the Tailwind
/// CLI at `tailwind`, and checks it.
///
/// # Errors
/// A [`CssError`]: a missing source, no Tailwind, a failed run, or a built
/// CSS without a marker class.
pub fn build(root: &Path, pins: &Pins, tailwind: &Path) -> Result<PathBuf, CssError> {
    let failed = |e: std::io::Error| CssError::Failed(e.to_string());
    let input_css = input(pins)?;
    let dir = root.join("target/css");
    std::fs::create_dir_all(&dir).map_err(failed)?;
    let (input_path, output) = (dir.join("input.css"), dir.join("app.css"));
    std::fs::write(&input_path, input_css).map_err(failed)?;
    let status = std::process::Command::new(tailwind)
        .arg("-i")
        .arg(&input_path)
        .arg("-o")
        .arg(&output)
        .arg("--minify")
        .status()
        .map_err(failed)?;
    if !status.success() {
        return Err(CssError::Failed(format!("tailwind exited with {status}")));
    }
    check_built(
        &std::fs::read_to_string(&output).map_err(failed)?,
        &markers(pins),
    )?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (PathBuf, Pins) {
        let root = std::env::temp_dir().join(format!(
            "xtask-css-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["ui-lib/style", "ui-lib/src", "map/src", "members/front/src"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        std::fs::write(root.join("ui-lib/style/theme.css"), "").unwrap();
        let pins = Pins {
            ui_lib: root.join("ui-lib"),
            map: None,
            // Not a path the code could guess: it comes from the pins.
            front: root.join("members/front"),
        };
        (root, pins)
    }

    #[test]
    fn the_sources_are_ui_lib_then_the_front() {
        let (root, pins) = tree();
        assert_eq!(
            sources(&pins),
            [root.join("ui-lib/src"), root.join("members/front/src")]
        );
    }

    #[test]
    fn the_map_crates_sources_come_between() {
        let (root, mut pins) = tree();
        pins.map = Some(root.join("map"));
        assert_eq!(
            sources(&pins),
            [
                root.join("ui-lib/src"),
                root.join("map/src"),
                root.join("members/front/src")
            ]
        );
    }

    #[test]
    fn the_input_imports_the_theme_and_names_every_source() {
        let (root, mut pins) = tree();
        pins.map = Some(root.join("map"));
        let css = input(&pins).unwrap();
        // No automatic source detection: it scans the working directory, which holds
        // this very file and its marker strings (a marker then lands by itself).
        assert!(
            css.starts_with("@import \"tailwindcss\" source(none);\n"),
            "{css}"
        );
        let theme = root.join("ui-lib/style/theme.css");
        assert!(
            css.contains(&format!("@import \"{}\";", theme.display())),
            "{css}"
        );
        for dir in ["ui-lib/src", "map/src", "members/front/src"] {
            let line = format!("@source \"{}\";", root.join(dir).display());
            assert!(css.contains(&line), "{line} in {css}");
        }
    }

    #[test]
    fn a_missing_source_fails() {
        let (root, mut pins) = tree();
        std::fs::remove_dir_all(root.join("ui-lib/src")).unwrap();
        assert_eq!(
            input(&pins),
            Err(CssError::MissingSource(root.join("ui-lib/src")))
        );
        let (root, _) = tree();
        pins.map = Some(root.join("map-not-here"));
        assert_eq!(
            input(&pins),
            Err(CssError::MissingSource(root.join("map-not-here/src")))
        );
    }

    #[test]
    fn a_missing_theme_fails() {
        let (root, pins) = tree();
        std::fs::remove_file(root.join("ui-lib/style/theme.css")).unwrap();
        assert_eq!(
            input(&pins),
            Err(CssError::MissingSource(root.join("ui-lib/style/theme.css")))
        );
    }

    #[test]
    fn the_map_marker_is_required_exactly_when_the_pins_have_a_map_crate() {
        let (root, mut pins) = tree();
        assert_eq!(markers_with(&pins, Some(".map-only")), [UI_LIB_MARKER]);
        pins.map = Some(root.join("map"));
        assert_eq!(
            markers_with(&pins, Some(".map-only")),
            [UI_LIB_MARKER, ".map-only"]
        );
    }

    /// A fake `$HOME` holding the CLI where cargo-leptos puts it.
    fn home_with(files: &[&str], name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("xtask-home-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for file in files {
            let path = home.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    #[test]
    fn the_variable_wins_and_an_empty_one_is_unset() {
        let home = home_with(
            &["Library/Caches/cargo-leptos/t-1/t-1/tailwindcss-x"],
            "var",
        );
        assert_eq!(
            find_tailwind(Some("/opt/tw".into()), Some(home.clone())),
            Ok(PathBuf::from("/opt/tw"))
        );
        assert_eq!(
            find_tailwind(Some("".into()), Some(home.clone())),
            Ok(home.join("Library/Caches/cargo-leptos/t-1/t-1/tailwindcss-x"))
        );
    }

    #[test]
    fn no_variable_and_no_home_is_no_tailwind() {
        assert_eq!(find_tailwind(None, None), Err(CssError::NoTailwind));
    }

    #[test]
    fn an_empty_or_missing_cache_is_no_tailwind() {
        let empty = home_with(&[], "empty");
        assert_eq!(find_tailwind(None, Some(empty)), Err(CssError::NoTailwind));
        let other = home_with(
            &["Library/Caches/cargo-leptos/wasm-opt-1/wasm-opt-1/wasm-opt"],
            "other",
        );
        assert_eq!(find_tailwind(None, Some(other)), Err(CssError::NoTailwind));
    }

    #[test]
    fn the_cache_walk_finds_the_cli_at_cargo_leptos_depth_and_takes_the_newest() {
        let base = "Library/Caches/cargo-leptos";
        let home = home_with(
            &[
                &format!("{base}/tailwindcss-v4.2.1/tailwindcss-v4.2.1/tailwindcss-macos-arm64"),
                &format!("{base}/tailwindcss-v4.3.0/tailwindcss-v4.3.0/tailwindcss-macos-arm64"),
                // Too deep, a directory name, and other tools: not the CLI.
                &format!("{base}/a/b/c/tailwindcss-deep"),
                &format!("{base}/tailwindcss-v9/tailwindcss-v9/not-tailwind"),
            ],
            "walk",
        );
        assert_eq!(
            find_tailwind(None, Some(home.clone())),
            Ok(home.join(format!(
                "{base}/tailwindcss-v4.3.0/tailwindcss-v4.3.0/tailwindcss-macos-arm64"
            )))
        );
        let linux = home_with(
            &[".cache/cargo-leptos/tailwindcss-v4/tailwindcss-v4/tailwindcss-linux-x64"],
            "linux",
        );
        assert!(find_tailwind(None, Some(linux)).is_ok());
    }

    #[test]
    fn versions_are_compared_by_number_not_by_text() {
        let base = "Library/Caches/cargo-leptos";
        let home = home_with(
            &[
                &format!("{base}/tailwindcss-v4.9.0/tailwindcss-v4.9.0/tailwindcss-macos-arm64"),
                &format!("{base}/tailwindcss-v4.10.0/tailwindcss-v4.10.0/tailwindcss-macos-arm64"),
                &format!("{base}/tailwindcss-v4.2.1/tailwindcss-v4.2.1/tailwindcss-macos-arm64"),
            ],
            "numeric",
        );
        assert_eq!(
            find_tailwind(None, Some(home.clone())),
            Ok(home.join(format!(
                "{base}/tailwindcss-v4.10.0/tailwindcss-v4.10.0/tailwindcss-macos-arm64"
            )))
        );
        assert_eq!(
            version_of(Path::new("/c/tailwindcss-v4.10.0/x")),
            [4, 10, 0]
        );
        assert_eq!(version_of(Path::new("/c/none/x")), Vec::<u64>::new());
    }

    #[test]
    fn a_failing_or_missing_cli_fails_the_build() {
        let (root, pins) = tree();
        for cli in ["/usr/bin/false", "/nonexistent/tailwindcss"] {
            let err = build(&root, &pins, Path::new(cli)).unwrap_err();
            assert!(matches!(err, CssError::Failed(_)), "{cli}: {err}");
        }
    }

    #[test]
    fn a_built_css_without_a_marker_fails() {
        let css = ".flex{display:flex}.min-h-screen{min-height:100vh}";
        assert_eq!(check_built(css, &[".min-h-screen"]), Ok(()));
        assert_eq!(
            check_built(css, &[".min-h-screen", ".only-the-map"]),
            Err(CssError::MissingClass(".only-the-map".into()))
        );
        assert_eq!(
            check_built(".flex{}", &[UI_LIB_MARKER]),
            Err(CssError::MissingClass(UI_LIB_MARKER.into()))
        );
    }
}
