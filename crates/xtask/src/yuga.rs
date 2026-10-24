//! `xtask yuga`: the local search's files, fetched, checked against pinned
//! SHA-256 digests, and staged for the bundle (never committed):
//!
//! - the `yuga-sidecar` binary (anatta-rs/yuga-releases, a public release,
//!   the Yuga Binary License) as `bin/yuga-sidecar-<target triple>`, the
//!   name Tauri's `externalBin` looks for, and its licence beside it;
//! - the English store (`gs://palicanon-app-assets`, private: `gsutil` with
//!   the builder's credentials), unpacked into `store/`;
//! - the `BAAI/bge-m3-int8` query model (`Xenova/bge-m3` at a pinned
//!   revision, the five files and digests yuga pins), into `model/`.
//!
//! Every file is checked before use; a wrong digest fails the task and the
//! file is removed. A local directory (`XTASK_YUGA_CACHE`) may hold the
//! downloads already: a file there is used when its digest is the pinned
//! one, fetched otherwise.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// The target the bundle is built for (macOS on Apple silicon first).
pub const TRIPLE: &str = "aarch64-apple-darwin";

/// Where a pinned file comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source<'a> {
    /// A public HTTPS address (`curl`).
    Https(&'a str),
    /// A Google Cloud Storage object (`gsutil`, the builder's credentials).
    Gs(&'a str),
}

/// One pinned download: its file name, its source and its SHA-256.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned {
    /// The file's name once downloaded.
    pub file: &'static str,
    /// Where it comes from.
    pub source: Source<'static>,
    /// Its lower-case hex SHA-256.
    pub sha256: &'static str,
}

/// yuga-sidecar 0.5.2: words at once, the model in the background; its
/// answers add a `complete` field.
pub const SIDECAR: Pinned = Pinned {
    file: "yuga-sidecar-0.5.2-aarch64-apple-darwin.tar.gz",
    source: Source::Https(
        "https://github.com/anatta-rs/yuga-releases/releases/download/yuga-sidecar-v0.5.2/yuga-sidecar-0.5.2-aarch64-apple-darwin.tar.gz",
    ),
    sha256: "a0878f002970f55dd7065c0e70ef519f53989559bddc6965dcba696a42c62963",
};

/// The English store built with `BAAI/bge-m3` (2026-10-02, b).
pub const STORE: Pinned = Pinned {
    file: "en-bge-m3-2026-10-02-b.tar.zst",
    source: Source::Gs("gs://palicanon-app-assets/store/en-bge-m3-2026-10-02-b.tar.zst"),
    sha256: "aa4870db813ec278cfbd778518e316b37d01060ce7422f88dff9b06665502d61",
};

/// The `Xenova/bge-m3` revision the model files are read at.
pub const MODEL_REVISION: &str = "4de13258303883538bd53b696b452bf8099f0858";

/// The five model files: the name in `model/`, the path in the Hugging Face
/// repository, and the digest yuga pins (`embedder/local.rs`).
pub const MODEL: [(&str, &str, &str); 5] = [
    (
        "config.json",
        "config.json",
        "734a79bf12d388c1467a4e3ab625f45de7f6906cffcfb93a1eca1787504bed95",
    ),
    (
        "special_tokens_map.json",
        "special_tokens_map.json",
        "8c785abebea9ae3257b61681b4e6fd8365ceafde980c21970d001e834cf10835",
    ),
    (
        "tokenizer_config.json",
        "tokenizer_config.json",
        "7e4c1cc848840aeccdd763458c18dd525eb0f795c992e00ebe9c28554e7db2d4",
    ),
    (
        "tokenizer.json",
        "tokenizer.json",
        "6710678b12670bc442b99edc952c4d996ae309a7020c1fa0096dd245c2faf790",
    ),
    (
        "model.onnx",
        "onnx/model_int8.onnx",
        "a206e10e995aa2a833924bcd725ba5dd6c3425cd34bac3cf2b5677cd2a1c51d6",
    ),
];

/// The binary's name inside the release archive.
pub const BINARY: &str = "yuga-sidecar";

/// The name Tauri's `externalBin` looks for: `<name>-<target triple>`.
pub fn external_bin_name() -> String {
    format!("{BINARY}-{TRIPLE}")
}

/// The Hugging Face address of a model file at the pinned revision.
pub fn model_url(path: &str) -> String {
    format!("https://huggingface.co/Xenova/bge-m3/resolve/{MODEL_REVISION}/{path}")
}

/// Why the files could not be staged.
#[derive(Debug, PartialEq, Eq)]
pub enum YugaError {
    /// A file's digest is not the pinned one (the file was removed).
    Digest {
        /// The file.
        file: String,
        /// What it hashed to.
        got: String,
    },
    /// A download, an unpacking or a copy failed.
    Failed(String),
}

impl std::fmt::Display for YugaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Digest { file, got } => {
                write!(f, "{file}: SHA-256 {got} is not the pinned one")
            }
            Self::Failed(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for YugaError {}

/// How a file is fetched: `curl` and `gsutil` in the task, a fake in tests.
pub trait Fetch {
    /// Writes the file at `source` to `to`.
    ///
    /// # Errors
    /// The download failed.
    fn fetch(&self, source: &Source<'_>, to: &Path) -> Result<(), String>;
}

/// `curl` for HTTPS, `gsutil` for Cloud Storage.
pub struct Tools;

impl Fetch for Tools {
    fn fetch(&self, source: &Source<'_>, to: &Path) -> Result<(), String> {
        let status = match source {
            Source::Https(url) => Command::new("curl")
                .args(["-sfL", "--retry", "3", "-o"])
                .arg(to)
                .arg(url)
                .status(),
            Source::Gs(object) => Command::new("gsutil")
                .args(["-q", "cp", object])
                .arg(to)
                .status(),
        }
        .map_err(|e| format!("cannot run the download of {source:?}: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("the download of {source:?} failed: {status}"))
        }
    }
}

/// The lower-case hex SHA-256 of the file at `path`.
///
/// # Errors
/// It cannot be read.
pub fn sha256_of(path: &Path) -> Result<String, YugaError> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| YugaError::Failed(format!("cannot read {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|e| YugaError::Failed(format!("cannot read {}: {e}", path.display())))?;
    Ok(hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        }))
}

/// Whether `path` exists with the digest `sha256`.
fn holds(path: &Path, sha256: &str) -> bool {
    path.is_file() && sha256_of(path).is_ok_and(|got| got == sha256)
}

/// Puts the pinned file at `to`: kept when it is there with the digest,
/// copied from `cache` when it is there with the digest, fetched otherwise,
/// then checked. A wrong digest removes the file and fails.
///
/// # Errors
/// The fetch failed, or the file's digest is not `sha256`.
pub fn obtain(
    source: &Source<'_>,
    sha256: &str,
    to: &Path,
    cached: Option<&Path>,
    fetch: &dyn Fetch,
) -> Result<(), YugaError> {
    let failed = |e: std::io::Error| YugaError::Failed(format!("{}: {e}", to.display()));
    if holds(to, sha256) {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }
    match cached {
        Some(cached) if holds(cached, sha256) => {
            std::fs::copy(cached, to).map_err(failed)?;
        }
        _ => fetch.fetch(source, to).map_err(YugaError::Failed)?,
    }
    let got = sha256_of(to)?;
    if got == sha256 {
        Ok(())
    } else {
        let _ = std::fs::remove_file(to);
        Err(YugaError::Digest {
            file: to.display().to_string(),
            got,
        })
    }
}

/// Unpacks the archive at `archive` (gzip or zstd, `tar` detects it) into a
/// fresh `into`.
///
/// # Errors
/// `tar` failed.
pub fn unpack(archive: &Path, into: &Path) -> Result<(), YugaError> {
    let _ = std::fs::remove_dir_all(into);
    std::fs::create_dir_all(into).map_err(|e| YugaError::Failed(e.to_string()))?;
    let status = Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .status()
        .map_err(|e| YugaError::Failed(format!("cannot run tar: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(YugaError::Failed(format!(
            "tar could not unpack {}: {status}",
            archive.display()
        )))
    }
}

/// The one directory an archive unpacked into (its top level).
fn top_dir(dir: &Path) -> Result<PathBuf, YugaError> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| YugaError::Failed(e.to_string()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    match (dirs.pop(), dirs.is_empty()) {
        (Some(one), true) => Ok(one),
        _ => Err(YugaError::Failed(format!(
            "{} does not hold exactly one directory",
            dir.display()
        ))),
    }
}

/// What [`stage`] staged, under `out`.
#[derive(Debug, PartialEq, Eq)]
pub struct Staged {
    /// `bin/yuga-sidecar-<triple>`.
    pub binary: PathBuf,
    /// `store/`.
    pub store: PathBuf,
    /// `model/`.
    pub model: PathBuf,
}

/// The model files as [`stage`] takes them: name, Hugging Face address at
/// the pinned revision, digest.
pub fn model_files() -> Vec<(&'static str, String, &'static str)> {
    MODEL
        .iter()
        .map(|(name, path, sha)| (*name, model_url(path), *sha))
        .collect()
}

/// Stages the sidecar, the store and the model under `out` (`bin/`,
/// `licence/`, `store/`, `model/`; the archives in `downloads/`), each file
/// checked; `cache` is a directory that may already hold the downloads, by
/// file name (the model files by their name in `model/`).
///
/// # Errors
/// A fetch, a digest, an unpacking or a copy failed.
pub fn stage(
    out: &Path,
    cache: Option<&Path>,
    sidecar: Pinned,
    store: Pinned,
    model: &[(&str, Source<'_>, &str)],
    fetch: &dyn Fetch,
) -> Result<Staged, YugaError> {
    let cached = |name: &str| cache.map(|c| c.join(name));
    let binary = stage_sidecar(out, sidecar, cached(sidecar.file).as_deref(), fetch)?;
    let store_dir = stage_store(out, store, cached(store.file).as_deref(), fetch)?;
    // The model: each file checked in place.
    let model_dir = out.join("model");
    for (name, source, sha256) in model {
        obtain(
            source,
            sha256,
            &model_dir.join(name),
            cached(name).as_deref(),
            fetch,
        )?;
    }
    Ok(Staged {
        binary,
        store: store_dir,
        model: model_dir,
    })
}

/// The sidecar: its archive checked, its binary under the `externalBin`
/// name in `bin/`, its licence in `licence/`.
fn stage_sidecar(
    out: &Path,
    sidecar: Pinned,
    cached: Option<&Path>,
    fetch: &dyn Fetch,
) -> Result<PathBuf, YugaError> {
    let failed = |e: std::io::Error| YugaError::Failed(e.to_string());
    let archive = out.join("downloads").join(sidecar.file);
    obtain(&sidecar.source, sidecar.sha256, &archive, cached, fetch)?;
    let unpacked = out.join("unpacked-sidecar");
    unpack(&archive, &unpacked)?;
    let top = top_dir(&unpacked)?;
    let binary = out.join("bin").join(external_bin_name());
    std::fs::create_dir_all(out.join("bin")).map_err(failed)?;
    std::fs::copy(top.join(BINARY), &binary).map_err(failed)?;
    std::fs::create_dir_all(out.join("licence")).map_err(failed)?;
    if top.join("LICENSE.md").is_file() {
        std::fs::copy(
            top.join("LICENSE.md"),
            out.join("licence/yuga-sidecar-LICENSE.md"),
        )
        .map_err(failed)?;
    }
    let _ = std::fs::remove_dir_all(&unpacked);
    Ok(binary)
}

/// The store: its archive checked, its one directory as `store/`.
fn stage_store(
    out: &Path,
    store: Pinned,
    cached: Option<&Path>,
    fetch: &dyn Fetch,
) -> Result<PathBuf, YugaError> {
    let failed = |e: std::io::Error| YugaError::Failed(e.to_string());
    let archive = out.join("downloads").join(store.file);
    obtain(&store.source, store.sha256, &archive, cached, fetch)?;
    let unpacked = out.join("unpacked-store");
    unpack(&archive, &unpacked)?;
    let store_dir = out.join("store");
    let _ = std::fs::remove_dir_all(&store_dir);
    std::fs::rename(top_dir(&unpacked)?, &store_dir).map_err(failed)?;
    let _ = std::fs::remove_dir_all(&unpacked);
    if store_dir.join("MANIFEST.json").is_file() {
        Ok(store_dir)
    } else {
        Err(YugaError::Failed("the store has no MANIFEST.json".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-yuga-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A fetch that copies files from a directory of "remote" files, by the
    /// last segment of the address, and records what it was asked.
    struct Remote {
        dir: PathBuf,
        asked: RefCell<Vec<String>>,
    }

    impl Fetch for Remote {
        fn fetch(&self, source: &Source<'_>, to: &Path) -> Result<(), String> {
            let (Source::Https(at) | Source::Gs(at)) = source;
            self.asked.borrow_mut().push((*at).to_string());
            let name = at.rsplit('/').next().unwrap();
            std::fs::copy(self.dir.join(name), to)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
    }

    fn tar(dir: &Path, archive: &Path, top: &str) {
        let status = Command::new("tar")
            .arg("-czf")
            .arg(archive)
            .arg("-C")
            .arg(dir)
            .arg(top)
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// The model files of a test: name, address, digest.
    type Model = Vec<(&'static str, String, String)>;

    /// A remote holding a sidecar archive, a store archive and two model
    /// files; the pins of them.
    fn remote(name: &str) -> (PathBuf, Remote, Pinned, Pinned, Model) {
        let root = temp(name);
        let src = root.join("src");
        let remote = root.join("remote");
        std::fs::create_dir_all(src.join("yuga-sidecar-x")).unwrap();
        std::fs::create_dir_all(src.join("en-store/dense")).unwrap();
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::write(src.join("yuga-sidecar-x/yuga-sidecar"), "#!/bin/sh\n").unwrap();
        std::fs::write(src.join("yuga-sidecar-x/LICENSE.md"), "licence").unwrap();
        std::fs::write(src.join("en-store/MANIFEST.json"), "{}").unwrap();
        tar(&src, &remote.join("sidecar.tar.gz"), "yuga-sidecar-x");
        // gzip here: `tar -x` detects the compression (the real store is zstd).
        tar(&src, &remote.join("store.tar.zst"), "en-store");
        std::fs::write(remote.join("a.json"), "a").unwrap();
        std::fs::write(remote.join("model_int8.onnx"), "onnx").unwrap();
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        let sidecar = Pinned {
            file: "sidecar.tar.gz",
            source: Source::Https("https://example.invalid/sidecar.tar.gz"),
            sha256: leak(sha256_of(&remote.join("sidecar.tar.gz")).unwrap()),
        };
        let store = Pinned {
            file: "store.tar.zst",
            source: Source::Gs("gs://bucket/store.tar.zst"),
            sha256: leak(sha256_of(&remote.join("store.tar.zst")).unwrap()),
        };
        let model = vec![
            (
                "a.json",
                "https://example.invalid/a.json".to_string(),
                sha256_of(&remote.join("a.json")).unwrap(),
            ),
            (
                "model.onnx",
                "https://example.invalid/onnx/model_int8.onnx".to_string(),
                sha256_of(&remote.join("model_int8.onnx")).unwrap(),
            ),
        ];
        let fetch = Remote {
            dir: remote,
            asked: RefCell::default(),
        };
        (root, fetch, sidecar, store, model)
    }

    fn sources<'a>(
        model: &'a [(&'static str, String, String)],
    ) -> Vec<(&'static str, Source<'a>, &'a str)> {
        model
            .iter()
            .map(|(n, url, sha)| (*n, Source::Https(url.as_str()), sha.as_str()))
            .collect()
    }

    #[test]
    fn the_external_bin_is_named_with_the_target_triple() {
        assert_eq!(external_bin_name(), "yuga-sidecar-aarch64-apple-darwin");
    }

    /// The pins are the brief's and yuga's: the release and its digest, the
    /// five model files with the digests `embedder/local.rs` pins, at one
    /// revision of `Xenova/bge-m3`.
    #[test]
    fn the_pins_are_the_released_files() {
        assert_eq!(
            SIDECAR.sha256,
            "a0878f002970f55dd7065c0e70ef519f53989559bddc6965dcba696a42c62963"
        );
        assert!(
            matches!(SIDECAR.source, Source::Https(u) if u.ends_with(SIDECAR.file) && u.contains("/yuga-sidecar-v0.5.2/"))
        );
        assert!(matches!(STORE.source, Source::Gs(u) if u.ends_with(STORE.file)));
        let names: Vec<&str> = MODEL.iter().map(|(n, _, _)| *n).collect();
        assert_eq!(
            names,
            [
                "config.json",
                "special_tokens_map.json",
                "tokenizer_config.json",
                "tokenizer.json",
                "model.onnx"
            ]
        );
        for (_, _, sha) in MODEL {
            assert_eq!(sha.len(), 64);
            assert!(
                sha.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            );
        }
        assert_eq!(
            model_url("onnx/model_int8.onnx"),
            format!(
                "https://huggingface.co/Xenova/bge-m3/resolve/{MODEL_REVISION}/onnx/model_int8.onnx"
            )
        );
        assert_eq!(model_files().len(), 5);
    }

    #[test]
    fn everything_is_staged_checked_and_the_binary_named_for_tauri() {
        let (root, fetch, sidecar, store, model) = remote("stage");
        let out = root.join("out");
        let staged = stage(&out, None, sidecar, store, &sources(&model), &fetch).unwrap();
        assert_eq!(
            staged.binary,
            out.join("bin/yuga-sidecar-aarch64-apple-darwin")
        );
        assert!(staged.binary.is_file());
        assert!(out.join("licence/yuga-sidecar-LICENSE.md").is_file());
        assert!(staged.store.join("MANIFEST.json").is_file());
        assert!(staged.store.join("dense").is_dir());
        assert_eq!(
            std::fs::read_to_string(staged.model.join("model.onnx")).unwrap(),
            "onnx"
        );
        assert_eq!(fetch.asked.borrow().len(), 4);
        // A second run keeps what is there with its digest: nothing fetched.
        stage(&out, None, sidecar, store, &sources(&model), &fetch).unwrap();
        assert_eq!(fetch.asked.borrow().len(), 4);
    }

    #[test]
    fn a_cached_file_with_the_digest_is_used_and_a_wrong_one_is_fetched() {
        let (root, fetch, sidecar, store, model) = remote("cache");
        let cache = root.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::copy(
            root.join("remote/model_int8.onnx"),
            cache.join("model.onnx"),
        )
        .unwrap();
        std::fs::write(cache.join("a.json"), "tampered").unwrap();
        stage(
            &root.join("out"),
            Some(&cache),
            sidecar,
            store,
            &sources(&model),
            &fetch,
        )
        .unwrap();
        let asked = fetch.asked.borrow();
        assert!(
            !asked.iter().any(|a| a.ends_with("model_int8.onnx")),
            "{asked:?}"
        );
        assert!(asked.iter().any(|a| a.ends_with("a.json")), "{asked:?}");
    }

    #[test]
    fn a_wrong_digest_fails_and_removes_the_file() {
        let (root, fetch, mut sidecar, store, model) = remote("digest");
        sidecar.sha256 = "0000000000000000000000000000000000000000000000000000000000000000";
        let out = root.join("out");
        let err = stage(&out, None, sidecar, store, &sources(&model), &fetch).unwrap_err();
        assert!(matches!(err, YugaError::Digest { .. }), "{err}");
        assert!(!out.join("downloads/sidecar.tar.gz").exists());
        assert!(!out.join("bin").exists());
    }

    #[test]
    fn a_failed_fetch_fails() {
        let (root, fetch, sidecar, store, mut model) = remote("fetch");
        model[0].1 = "https://example.invalid/absent.json".into();
        let err = stage(
            &root.join("out"),
            None,
            sidecar,
            store,
            &sources(&model),
            &fetch,
        )
        .unwrap_err();
        assert!(matches!(err, YugaError::Failed(_)), "{err}");
    }
}
