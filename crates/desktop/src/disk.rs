//! The file operations the progress writer makes, behind a trait so the
//! tests can fail any one of them ([`Disk`]); [`RealDisk`] is the app's.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// What the writer asks of the file system, one operation per step of a
/// write, so a test can fail each step alone.
pub trait Disk: Send + Sync + 'static {
    /// The bytes of the file at `path`; `None` when it does not exist.
    ///
    /// # Errors
    /// Any other I/O error.
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>>;

    /// Creates (or truncates) `path`, writes `bytes` and flushes them to the
    /// disk (`fsync`).
    ///
    /// # Errors
    /// Any I/O error.
    fn write_synced(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;

    /// Renames `from` to `to`, replacing `to`.
    ///
    /// # Errors
    /// Any I/O error.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Flushes the directory `dir` itself (`fsync`), so a rename in it is
    /// on the disk.
    ///
    /// # Errors
    /// Any I/O error.
    fn sync_dir(&self, dir: &Path) -> io::Result<()>;

    /// Opens (creating it) the lock file at `path` and takes an exclusive,
    /// non-blocking lock on it, held as long as the returned file is open.
    /// `Ok(None)` when another open file holds it (another process).
    ///
    /// # Errors
    /// Any other I/O error.
    fn try_lock(&self, path: &Path) -> io::Result<Option<File>>;
}

/// The real file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealDisk;

impl Disk for RealDisk {
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write_synced(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        File::open(dir)?.sync_all()
    }

    fn try_lock(&self, path: &Path) -> io::Result<Option<File>> {
        use rustix::fs::{FlockOperation, flock};
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(Some(file)),
            Err(rustix::io::Errno::WOULDBLOCK) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pc-disk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_reads_as_none_and_a_written_one_as_its_bytes() {
        let dir = temp("read");
        let path = dir.join("a.json");
        assert_eq!(RealDisk.read(&path).unwrap(), None);
        RealDisk.write_synced(&path, b"{}").unwrap();
        assert_eq!(RealDisk.read(&path).unwrap(), Some(b"{}".to_vec()));
        RealDisk.rename(&path, &dir.join("b.json")).unwrap();
        RealDisk.sync_dir(&dir).unwrap();
        assert_eq!(RealDisk.read(&path).unwrap(), None);
    }

    #[test]
    fn a_directory_read_is_an_error_not_none() {
        let dir = temp("read-dir");
        assert!(RealDisk.read(&dir).is_err());
    }

    /// `flock` locks belong to the open file: a second open of the same
    /// file, in this process too, cannot take it while the first holds it.
    #[test]
    fn the_lock_is_exclusive_until_its_file_is_closed() {
        let dir = temp("lock");
        let path = dir.join("progress.lock");
        let first = RealDisk.try_lock(&path).unwrap();
        assert!(first.is_some());
        assert!(RealDisk.try_lock(&path).unwrap().is_none());
        drop(first);
        assert!(RealDisk.try_lock(&path).unwrap().is_some());
    }
}
