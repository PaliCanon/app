//! `xtask data`: stages the map's six data files from the map crate's
//! checkout (its `public/`) into a directory that Trunk copies into the dist
//! under `/map-data/`.

use std::path::{Path, PathBuf};

/// The six files, by name (the front's `data::FILES` is tested equal).
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

/// Copies the six files from `<map>/public/` into `out` (created, emptied
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
                .map(|f| (map.join("public").join(f), *f))
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

    fn map_with(files: &[&str], name: &str) -> PathBuf {
        let map = dir(name);
        std::fs::create_dir_all(map.join("public")).unwrap();
        for f in files {
            std::fs::write(map.join("public").join(f), f.as_bytes()).unwrap();
        }
        map
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
            map.join("public/blurbs-en.json"),
            map.join("public/blurbs-fr.json"),
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
