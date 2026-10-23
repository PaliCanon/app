//! The dictionary assets the app ships (task 008): the dictionary (every
//! DPD entry, 23 MB), the forms (a word as written to its senses, 11 MB)
//! and the passages (every entry's line from the suttas, precomputed by
//! learn-pali's `build-deck passages`). Each is pinned by its SHA-256 and
//! fetched into `target/dictionary/` under a fixed name, which the bundle
//! copies to `Resources/dictionary/`.

use std::path::{Path, PathBuf};

use crate::yuga::{Fetch, Pinned, Source, YugaError, obtain};

/// The dictionary asset: `api 008`, built from the DPD on 2026-10-22.
pub const DICTIONARY: Pinned = Pinned {
    file: "dictionary.db",
    source: Source::Gs("gs://palicanon-app-assets/dictionary/dictionary-2026-10-22.db"),
    sha256: "18db6704861624fceadc2140e2d1368a9fff3abc3388daccfd335379281bf90b",
};

/// The forms asset: the DPD's inflections to their senses (2026-10-01).
pub const FORMS: Pinned = Pinned {
    file: "dpd.db",
    source: Source::Gs("gs://palicanon-app-assets/dpd/dpd-2026-10-01.db"),
    sha256: "768bdab1f718312fe67b54f935c3763b30657b7888d6fa6c4bbaf97de8c008f4",
};

/// The passages asset: learn-pali 068, built on 2026-10-23.
pub const PASSAGES: Pinned = Pinned {
    file: "passages.db",
    source: Source::Gs("gs://palicanon-app-assets/passages/passages-2026-10-23.db"),
    sha256: "8ea02066e449cd2ee3f1e8840cf6b1fef98af97d98ed7b591c41ed438bc6790e",
};

/// Fetches the three assets into `out` (checked by digest, a cached copy
/// used when it matches) and returns the directory.
///
/// # Errors
/// A fetch failed, or a file's digest is not the pinned one.
pub fn stage(
    out: &Path,
    cache: Option<&Path>,
    assets: &[Pinned],
    fetch: &dyn Fetch,
) -> Result<PathBuf, YugaError> {
    for asset in assets {
        let cached = cache.map(|c| c.join(asset.file));
        obtain(
            &asset.source,
            asset.sha256,
            &out.join(asset.file),
            cached.as_deref(),
            fetch,
        )?;
    }
    Ok(out.to_path_buf())
}

/// The three assets, in the order they are fetched.
#[must_use]
pub fn assets() -> [Pinned; 3] {
    [DICTIONARY, FORMS, PASSAGES]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yuga::sha256_of;

    /// A fetch that writes a known text for every source.
    struct Writes(&'static str);

    impl Fetch for Writes {
        fn fetch(&self, _source: &Source<'_>, to: &Path) -> Result<(), String> {
            std::fs::write(to, self.0).map_err(|e| e.to_string())
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pc-dictionary-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_assets_land_under_their_fixed_names_when_the_digest_matches() {
        let out = temp("ok");
        let text = "the dictionary";
        let sha = {
            let probe = out.join("probe");
            std::fs::create_dir_all(&out).unwrap();
            std::fs::write(&probe, text).unwrap();
            sha256_of(&probe).unwrap()
        };
        let pinned = Pinned {
            file: "dictionary.db",
            source: Source::Gs("gs://bucket/dictionary.db"),
            sha256: Box::leak(sha.into_boxed_str()),
        };
        let dir = stage(&out, None, &[pinned], &Writes(text)).unwrap();
        assert_eq!(dir, out);
        assert_eq!(
            std::fs::read_to_string(out.join("dictionary.db")).unwrap(),
            text
        );
    }

    #[test]
    fn a_wrong_digest_fails_and_leaves_no_file() {
        let out = temp("bad");
        let pinned = Pinned {
            file: "dpd.db",
            source: Source::Gs("gs://bucket/dpd.db"),
            sha256: "00",
        };
        let err = stage(&out, None, &[pinned], &Writes("x")).unwrap_err();
        assert!(matches!(err, YugaError::Digest { .. }), "{err}");
        assert!(!out.join("dpd.db").exists());
    }

    #[test]
    fn the_three_assets_have_distinct_names_and_full_digests() {
        let all = assets();
        let names: std::collections::BTreeSet<&str> = all.iter().map(|a| a.file).collect();
        assert_eq!(names.len(), 3);
        for a in &all {
            assert_eq!(a.sha256.len(), 64, "{}: a pinned digest", a.file);
        }
    }
}
