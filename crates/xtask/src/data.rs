//! `xtask data`: stages the map's six data files from the map crate's
//! checkout (the repository's `public/`) into a directory that Trunk copies into the dist
//! under `/map-data/`.

use std::path::{Path, PathBuf};

/// The six files, by name (the front's `data::bundled_files`, the map's `MAP_FILES` less the optional `blurbs-vi.json`, is tested equal).
pub const FILES: [&str; 6] = [
    "graph.json",
    "graph-sim.json",
    "graph-sim.bin",
    "translations.json",
    "blurbs-en.json",
    "blurbs-fr.json",
];

/// What staging did.
#[derive(Debug, PartialEq, Eq)]
pub enum Staged {
    /// The front does not depend on the map crate yet: nothing to copy.
    Skipped,
    /// This many files were copied.
    Copied(usize),
}

/// Why the files could not be staged.
#[derive(Debug, PartialEq, Eq)]
pub enum DataError {
    /// These files are not in the map crate's `public/`.
    Missing(Vec<PathBuf>),
    /// A copy or a directory failed.
    Failed(String),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(files) => {
                let list: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
                write!(f, "map data missing: {}", list.join(", "))
            }
            Self::Failed(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for DataError {}

/// The map's data directory: the repository's `public/`, beside the crate's
/// directory (the crate is `app/` in the map's repository).
pub fn public_dir(map: &Path) -> PathBuf {
    map.parent().unwrap_or(map).join("public")
}

/// Copies the six files from `<map>/../public/` into `out` (created, emptied
/// first). With no map crate, only creates the empty `out` and says
/// [`Staged::Skipped`].
///
/// # Errors
/// [`DataError::Missing`] naming every absent file (nothing is copied then),
/// or [`DataError::Failed`] on an I/O error.
pub fn stage(map: Option<&Path>, out: &Path) -> Result<Staged, DataError> {
    let failed = |e: std::io::Error| DataError::Failed(e.to_string());
    let sources: Vec<(PathBuf, &str)> = map
        .map(|map| {
            FILES
                .iter()
                .map(|f| (public_dir(map).join(f), *f))
                .collect()
        })
        .unwrap_or_default();
    let missing: Vec<PathBuf> = sources
        .iter()
        .filter(|(p, _)| !p.is_file())
        .map(|(p, _)| p.clone())
        .collect();
    if !missing.is_empty() {
        return Err(DataError::Missing(missing));
    }
    if out.exists() {
        std::fs::remove_dir_all(out).map_err(failed)?;
    }
    std::fs::create_dir_all(out).map_err(failed)?;
    if map.is_none() {
        return Ok(Staged::Skipped);
    }
    for (from, name) in &sources {
        std::fs::copy(from, out.join(name)).map_err(failed)?;
    }
    Ok(Staged::Copied(sources.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("xtask-data-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A checkout laid out as the map's: the crate in `app/`, the data in
    /// the repository's `public/`. Returns the crate's directory.
    fn map_with(files: &[&str], name: &str) -> PathBuf {
        let root = dir(name);
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(root.join("public")).unwrap();
        for f in files {
            std::fs::write(root.join("public").join(f), f.as_bytes()).unwrap();
        }
        root.join("app")
    }

    #[test]
    fn the_files_are_the_designs_six_names() {
        assert_eq!(
            FILES,
            [
                "graph.json",
                "graph-sim.json",
                "graph-sim.bin",
                "translations.json",
                "blurbs-en.json",
                "blurbs-fr.json"
            ]
        );
    }

    #[test]
    fn an_output_that_cannot_be_created_is_a_failure() {
        let map = map_with(&FILES, "unwritable");
        let blocker = dir("unwritable-out").join("a-file");
        std::fs::write(&blocker, "x").unwrap();
        // The output's parent is a regular file.
        let result = stage(Some(&map), &blocker.join("map-data"));
        assert!(matches!(result, Err(DataError::Failed(_))), "{result:?}");
        let result = stage(None, &blocker.join("map-data"));
        assert!(matches!(result, Err(DataError::Failed(_))), "{result:?}");
    }

    #[test]
    fn the_six_files_are_copied() {
        let map = map_with(&FILES, "all");
        let out = dir("all-out").join("map-data");
        assert_eq!(stage(Some(&map), &out), Ok(Staged::Copied(6)));
        for f in FILES {
            assert_eq!(std::fs::read(out.join(f)).unwrap(), f.as_bytes());
        }
    }

    #[test]
    fn a_missing_file_fails_and_copies_nothing() {
        let map = map_with(&FILES[..4], "four");
        let out = dir("four-out").join("map-data");
        let want = vec![
            map.parent().unwrap().join("public/blurbs-en.json"),
            map.parent().unwrap().join("public/blurbs-fr.json"),
        ];
        assert_eq!(stage(Some(&map), &out), Err(DataError::Missing(want)));
        assert!(!out.join("graph.json").exists());
    }

    #[test]
    fn no_map_crate_is_skipped_with_an_empty_directory() {
        let out = dir("none-out").join("map-data");
        assert_eq!(stage(None, &out), Ok(Staged::Skipped));
        assert!(out.is_dir());
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
    }

    #[test]
    fn a_stale_file_is_removed() {
        let map = map_with(&FILES, "stale");
        let out = dir("stale-out").join("map-data");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("old.json"), "x").unwrap();
        stage(Some(&map), &out).unwrap();
        assert!(!out.join("old.json").exists());
    }
}
