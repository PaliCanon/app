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

/// A class only the map crate's sources use, once the front depends on the
/// map crate. `None` until then.
pub const MAP_MARKER: Option<&str> = None;

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
pub fn sources(pins: &Pins, front_src: &Path) -> Vec<PathBuf> {
    let mut sources = vec![pins.ui_lib.join("src")];
    if let Some(map) = &pins.map {
        sources.push(map.join("src"));
    }
    sources.push(front_src.to_path_buf());
    sources
}

/// The Tailwind input stylesheet for `pins`, written with absolute paths.
///
/// # Errors
/// [`CssError::MissingSource`] when ui-lib's theme or a source directory is
/// not there.
pub fn input(pins: &Pins, front_src: &Path) -> Result<String, CssError> {
    let theme = pins.ui_lib.join("style/theme.css");
    let sources = sources(pins, front_src);
    for path in std::iter::once(&theme).chain(&sources) {
        if !path.exists() {
            return Err(CssError::MissingSource(path.clone()));
        }
    }
    let mut lines = vec![
        "@import \"tailwindcss\";".to_string(),
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
    let mut markers = vec![UI_LIB_MARKER];
    if pins.map.is_some() {
        markers.extend(MAP_MARKER);
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

/// The Tailwind CLI: `$TAILWIND`, else the newest one `cargo-leptos` cached.
///
/// # Errors
/// [`CssError::NoTailwind`] when there is none.
pub fn tailwind_binary() -> Result<PathBuf, CssError> {
    if let Some(path) = std::env::var_os("TAILWIND") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(CssError::NoTailwind)?;
    let mut found: Vec<PathBuf> = Vec::new();
    for cache in [
        home.join("Library/Caches/cargo-leptos"),
        home.join(".cache/cargo-leptos"),
    ] {
        collect(&cache, 3, &mut found);
    }
    found.sort();
    found.pop().ok_or(CssError::NoTailwind)
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

/// Builds the app's CSS into `<root>/target/css/app.css` and checks it.
///
/// # Errors
/// A [`CssError`]: a missing source, no Tailwind, a failed run, or a built
/// CSS without a marker class.
pub fn build(root: &Path, pins: &Pins) -> Result<PathBuf, CssError> {
    let failed = |e: std::io::Error| CssError::Failed(e.to_string());
    let input_css = input(pins, &root.join("crates/front/src"))?;
    let dir = root.join("target/css");
    std::fs::create_dir_all(&dir).map_err(failed)?;
    let (input_path, output) = (dir.join("input.css"), dir.join("app.css"));
    std::fs::write(&input_path, input_css).map_err(failed)?;
    let status = std::process::Command::new(tailwind_binary()?)
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
        for dir in ["ui-lib/style", "ui-lib/src", "map/src", "front/src"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        std::fs::write(root.join("ui-lib/style/theme.css"), "").unwrap();
        let pins = Pins {
            ui_lib: root.join("ui-lib"),
            map: None,
        };
        (root, pins)
    }

    #[test]
    fn the_sources_are_ui_lib_then_the_front() {
        let (root, pins) = tree();
        assert_eq!(
            sources(&pins, &root.join("front/src")),
            [root.join("ui-lib/src"), root.join("front/src")]
        );
    }

    #[test]
    fn the_map_crates_sources_come_between() {
        let (root, mut pins) = tree();
        pins.map = Some(root.join("map"));
        assert_eq!(
            sources(&pins, &root.join("front/src")),
            [
                root.join("ui-lib/src"),
                root.join("map/src"),
                root.join("front/src")
            ]
        );
    }

    #[test]
    fn the_input_imports_the_theme_and_names_every_source() {
        let (root, mut pins) = tree();
        pins.map = Some(root.join("map"));
        let css = input(&pins, &root.join("front/src")).unwrap();
        assert!(css.starts_with("@import \"tailwindcss\";\n"), "{css}");
        let theme = root.join("ui-lib/style/theme.css");
        assert!(
            css.contains(&format!("@import \"{}\";", theme.display())),
            "{css}"
        );
        for dir in ["ui-lib/src", "map/src", "front/src"] {
            let line = format!("@source \"{}\";", root.join(dir).display());
            assert!(css.contains(&line), "{line} in {css}");
        }
    }

    #[test]
    fn a_missing_source_fails() {
        let (root, mut pins) = tree();
        std::fs::remove_dir_all(root.join("ui-lib/src")).unwrap();
        assert_eq!(
            input(&pins, &root.join("front/src")),
            Err(CssError::MissingSource(root.join("ui-lib/src")))
        );
        let (root, _) = tree();
        pins.map = Some(root.join("map-not-here"));
        assert_eq!(
            input(&pins, &root.join("front/src")),
            Err(CssError::MissingSource(root.join("map-not-here/src")))
        );
    }

    #[test]
    fn a_missing_theme_fails() {
        let (root, pins) = tree();
        std::fs::remove_file(root.join("ui-lib/style/theme.css")).unwrap();
        assert_eq!(
            input(&pins, &root.join("front/src")),
            Err(CssError::MissingSource(root.join("ui-lib/style/theme.css")))
        );
    }

    #[test]
    fn the_markers_follow_the_map_crate() {
        let (root, mut pins) = tree();
        assert_eq!(markers(&pins), [UI_LIB_MARKER]);
        pins.map = Some(root.join("map"));
        assert_eq!(markers(&pins).len(), 1 + usize::from(MAP_MARKER.is_some()));
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
