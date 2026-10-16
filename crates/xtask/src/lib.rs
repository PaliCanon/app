//! The app's build tasks, native and never shipped: `pins` reads the one
//! `cargo metadata` of the browser build for the lockstep check and the
//! checkout paths; `css` builds the Tailwind CSS, `data` stages the map's
//! data files and `assets` the fragments' root files, all from those paths.

use std::error::Error;
use std::path::{Path, PathBuf};

pub mod assets;
pub mod css;
pub mod data;
pub mod pins;

/// Runs the task `task` over `pins` and returns the line it reports.
/// `tailwind` finds the Tailwind CLI, only asked by `css`. `root` is the
/// workspace root: the CSS goes under its `target/`; `data_out` is where
/// `data` stages the map's files.
///
/// # Errors
/// An unknown task, or whatever the task refuses: a missing source or data
/// file, no Tailwind, a failing Tailwind, an unwritable output.
pub fn run(
    task: &str,
    root: &Path,
    pins: &pins::Pins,
    tailwind: impl FnOnce() -> Result<PathBuf, css::CssError>,
    data_out: &Path,
) -> Result<String, Box<dyn Error>> {
    match task {
        "pins" => {
            let map = pins.map.as_ref().map(|m| format!("\nmap={}", m.display()));
            Ok(format!(
                "ui_lib={}\nfront={}{}",
                pins.ui_lib.display(),
                pins.front.display(),
                map.unwrap_or_default()
            ))
        }
        "css" => Ok(css::build(root, pins, &tailwind()?)?.display().to_string()),
        "data" => Ok(match data::stage(pins.map.as_deref(), data_out)? {
            data::Staged::Skipped => {
                "xtask data: skipped, the front does not depend on the map crate yet".to_string()
            }
            data::Staged::Copied(n) => format!("xtask data: {n} files in {}", data_out.display()),
        }),
        "assets" => {
            let out = root.join("target/assets");
            let n = assets::stage(pins.learn.as_deref(), pins.search.as_deref(), &out)?;
            Ok(format!("xtask assets: {n} files in {}", out.display()))
        }
        other => Err(format!("unknown task {other:?} (pins, css, data, assets)").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(name: &str) -> (PathBuf, pins::Pins) {
        let root = std::env::temp_dir().join(format!("xtask-run-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in [
            "ui-lib/style",
            "ui-lib/src",
            "front/src",
            "map/src",
            "map/public",
        ] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        std::fs::write(root.join("ui-lib/style/theme.css"), "").unwrap();
        let pins = pins::Pins {
            ui_lib: root.join("ui-lib"),
            map: None,
            front: root.join("front"),
            ..pins::Pins::default()
        };
        (root, pins)
    }

    #[test]
    fn an_unknown_task_is_an_error() {
        let (root, pins) = workspace("unknown");
        let err = run(
            "nope",
            &root,
            &pins,
            || Err(css::CssError::NoTailwind),
            &root.join("out"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown task"), "{err}");
    }

    #[test]
    fn css_fails_without_a_tailwind_and_with_a_failing_one() {
        let (root, pins) = workspace("css");
        let err = run(
            "css",
            &root,
            &pins,
            || Err(css::CssError::NoTailwind),
            &root.join("out"),
        )
        .unwrap_err();
        assert_eq!(err.to_string(), css::CssError::NoTailwind.to_string());
        let err = run(
            "css",
            &root,
            &pins,
            || Ok("/usr/bin/false".into()),
            &root.join("out"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("tailwind exited"), "{err}");
    }

    #[test]
    fn css_fails_on_a_missing_source() {
        let (root, pins) = workspace("css-source");
        std::fs::remove_dir_all(root.join("front/src")).unwrap();
        let err = run(
            "css",
            &root,
            &pins,
            || Ok("/usr/bin/true".into()),
            &root.join("out"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
    }

    #[test]
    fn data_fails_on_a_missing_file_and_is_skipped_without_a_map() {
        let (root, mut pins) = workspace("data");
        let skipped = run(
            "data",
            &root,
            &pins,
            || Err(css::CssError::NoTailwind),
            &root.join("out"),
        )
        .unwrap();
        assert!(skipped.contains("skipped"), "{skipped}");
        pins.map = Some(root.join("map"));
        let err = run(
            "data",
            &root,
            &pins,
            || Err(css::CssError::NoTailwind),
            &root.join("out"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("map data missing"), "{err}");
    }

    #[test]
    fn pins_reports_the_directories() {
        let (root, pins) = workspace("pins");
        let line = run(
            "pins",
            &root,
            &pins,
            || Err(css::CssError::NoTailwind),
            &root.join("out"),
        )
        .unwrap();
        assert!(
            line.contains("ui_lib=") && line.contains("front="),
            "{line}"
        );
    }
}
