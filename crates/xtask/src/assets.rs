//! `xtask assets`: the files the fragments expect at the root of the page,
//! copied from their pinned checkouts (never by hand into this repository):
//! `/lotus.png` from Learn's `public/`, `/brand/*.svg` from Search's
//! `public/brand/`. Staged in a directory whose files Trunk copies into the
//! dist (`index.html`).

use std::path::{Path, PathBuf};

/// Why the assets could not be staged.
#[derive(Debug, PartialEq, Eq)]
pub enum AssetsError {
    /// A file or directory the fragments must ship is not in its checkout.
    Missing(PathBuf),
    /// A copy or a directory failed.
    Failed(String),
}

impl std::fmt::Display for AssetsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(path) => write!(f, "asset missing: {}", path.display()),
            Self::Failed(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for AssetsError {}

/// Copies `<learn>/public/lotus.png` to `<out>/lotus.png` and every
/// `<search>/public/brand/*.svg` to `<out>/brand/`; `out` is emptied first.
/// Returns how many files were copied.
///
/// # Errors
/// [`AssetsError::Missing`] when Learn's lotus or Search's brand directory
/// (with at least one svg) is absent, [`AssetsError::Failed`] on I/O.
pub fn stage(
    learn: Option<&Path>,
    search: Option<&Path>,
    out: &Path,
) -> Result<usize, AssetsError> {
    let failed = |e: std::io::Error| AssetsError::Failed(e.to_string());
    let mut copies: Vec<(PathBuf, PathBuf)> = Vec::new();
    if let Some(learn) = learn {
        let lotus = learn.join("public/lotus.png");
        if !lotus.is_file() {
            return Err(AssetsError::Missing(lotus));
        }
        copies.push((lotus, out.join("lotus.png")));
    }
    if let Some(search) = search {
        let brand = search.join("public/brand");
        let svgs: Vec<PathBuf> = std::fs::read_dir(&brand)
            .map_err(|_| AssetsError::Missing(brand.clone()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "svg"))
            .collect();
        if svgs.is_empty() {
            return Err(AssetsError::Missing(brand.join("*.svg")));
        }
        for svg in svgs {
            if let Some(name) = svg.file_name() {
                copies.push((svg.clone(), out.join("brand").join(name)));
            }
        }
    }
    if out.exists() {
        std::fs::remove_dir_all(out).map_err(failed)?;
    }
    std::fs::create_dir_all(out.join("brand")).map_err(failed)?;
    for (from, to) in &copies {
        std::fs::copy(from, to).map_err(failed)?;
    }
    Ok(copies.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("xtask-assets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("learn/public")).unwrap();
        std::fs::create_dir_all(root.join("search/public/brand")).unwrap();
        std::fs::write(root.join("learn/public/lotus.png"), "png").unwrap();
        std::fs::write(root.join("search/public/brand/kofi.svg"), "<svg/>").unwrap();
        std::fs::write(root.join("search/public/brand/notes.txt"), "no").unwrap();
        root
    }

    #[test]
    fn the_lotus_and_the_brand_svgs_are_staged_at_the_root() {
        let root = tree("ok");
        let out = root.join("out");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("stale.txt"), "x").unwrap();
        let n = stage(Some(&root.join("learn")), Some(&root.join("search")), &out).unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            std::fs::read_to_string(out.join("lotus.png")).unwrap(),
            "png"
        );
        assert_eq!(
            std::fs::read_to_string(out.join("brand/kofi.svg")).unwrap(),
            "<svg/>"
        );
        assert!(!out.join("brand/notes.txt").exists());
        assert!(!out.join("stale.txt").exists(), "out is emptied first");
    }

    #[test]
    fn a_missing_lotus_or_brand_fails() {
        let root = tree("missing");
        std::fs::remove_file(root.join("learn/public/lotus.png")).unwrap();
        let err = stage(Some(&root.join("learn")), None, &root.join("out")).unwrap_err();
        assert!(
            matches!(err, AssetsError::Missing(ref p) if p.ends_with("lotus.png")),
            "{err}"
        );
        std::fs::remove_file(root.join("search/public/brand/kofi.svg")).unwrap();
        let err = stage(None, Some(&root.join("search")), &root.join("out")).unwrap_err();
        assert!(matches!(err, AssetsError::Missing(_)), "{err}");
        std::fs::remove_dir_all(root.join("search/public/brand")).unwrap();
        let err = stage(None, Some(&root.join("search")), &root.join("out")).unwrap_err();
        assert!(matches!(err, AssetsError::Missing(_)), "{err}");
    }
}
