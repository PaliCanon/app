//! The native progress writer: the learner's progress kept in Rust and
//! written to `progress.json` in the app's data directory.
//!
//! **One mutex** over the writer's whole state (the slot, `closed`, the lock
//! file held) and over every write. The front's save fills **one slot** (a
//! newer `seq` replaces it, an older one is dropped) under that mutex and
//! wakes the loop, so a save made during a write waits for that one write and
//! is written after it: the file's order is the saves' order.
//!
//! The loop, per write: take the mutex, re-check `closed`, take the slot;
//! read the file on disk: a version newer than `learn_core`'s
//! `State::VERSION` → write nothing, emit `NewerVersion(seq, raw)`, close; a
//! file that does not parse → move it aside to
//! `progress.json.corrupt-<millis>` first; write a temp file, `fsync`,
//! rename, `fsync` the directory; emit `Saved(seq)`. Any I/O error on that
//! path (the move aside included) → emit `Transient(seq)` and put the text
//! back in the slot unless a newer one is there; the retry waits
//! ([`RETRY`], or the next save) with the mutex released.
//!
//! [`Writer::load`] runs on the same state: it takes the lock on
//! `progress.lock` for the session (another process holding it → the
//! answer is `Unreadable("PaliCanon is already open")` and this one never
//! writes), and an unreadable file shuts the writer before the front can
//! save. [`Writer::exit`] (on `RunEvent::Exit`) takes the mutex, waiting at
//! most for the write in flight, writes the slot if it is filled and the
//! writer not closed, and closes: whatever the loop held is then refused.
//! A crash or a kill loses at most the write in flight and the slot.

use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use learn_core::progress::{State, StateError};
use palicanon_app_bridge::{LoadAnswer, MAX_TEXT_BYTES, StatusEvent};

use crate::disk::Disk;

/// The progress file's name in the data directory.
pub const PROGRESS_FILE: &str = "progress.json";
/// The lock file's name in the data directory: held for the session.
pub const LOCK_FILE: &str = "progress.lock";
/// The temp file a write goes through before its rename.
const TEMP_FILE: &str = "progress.json.tmp";
/// What [`Writer::load`] answers when another PaliCanon holds the lock.
pub const ALREADY_OPEN: &str = "PaliCanon is already open";
/// How long a failed write waits before its retry (unless a save comes).
pub const RETRY: Duration = Duration::from_secs(5);

/// Where the writer's verdicts go (the `progress-status` event in the app).
/// Called with the mutex held, so the verdicts go out in the writes' order.
pub type Emit = Box<dyn Fn(StatusEvent) + Send + Sync>;

/// The writer: cheap to clone, one shared state.
#[derive(Clone)]
pub struct Writer {
    shared: Arc<Shared>,
}

struct Shared {
    dir: PathBuf,
    disk: Box<dyn Disk>,
    emit: Emit,
    retry: Duration,
    state: Mutex<Inner>,
    wake: Condvar,
}

/// What the one mutex guards.
#[derive(Default)]
struct Inner {
    /// The newest save not yet written.
    slot: Option<(u64, String)>,
    /// Nothing more is written this session (`Unreadable`, `NewerVersion`, Exit,
    /// another process holding the lock).
    closed: bool,
    /// The lock on `progress.lock`, held for the session once taken.
    lock: Option<File>,
    /// The last write failed: the loop waits [`Shared::retry`] or a save.
    retry_pending: bool,
}

/// What one write did.
enum Outcome {
    Saved,
    Newer(String),
    Transient(std::io::Error),
}

/// Puts the save `seq` in `slot`: the newest save wins, an older one is
/// dropped (the put-back after a failed write follows the same rule).
pub fn offer(slot: &mut Option<(u64, String)>, seq: u64, text: String) {
    if slot.as_ref().is_none_or(|(kept, _)| *kept < seq) {
        *slot = Some((seq, text));
    }
}

impl Writer {
    /// A writer over the data directory `dir` (created when missing, at the
    /// first load), its file operations on `disk`, its verdicts to `emit`,
    /// a failed write retried after `retry`.
    pub fn new(dir: PathBuf, disk: impl Disk, emit: Emit, retry: Duration) -> Self {
        Writer {
            shared: Arc::new(Shared {
                dir,
                disk: Box::new(disk),
                emit,
                retry,
                state: Mutex::new(Inner::default()),
                wake: Condvar::new(),
            }),
        }
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Reads the progress file, I/O only (the front classifies the text),
    /// taking the session's lock first. `Unreadable` closes the writer.
    pub fn load(&self) -> LoadAnswer {
        let mut inner = self.inner();
        if let Err(why) = self.shared.hold_lock(&mut inner) {
            inner.closed = true;
            return LoadAnswer::Unreadable(why);
        }
        match self.shared.disk.read(&self.shared.path(PROGRESS_FILE)) {
            Ok(None) => LoadAnswer::Missing,
            Ok(Some(bytes)) => LoadAnswer::Text(String::from_utf8_lossy(&bytes).into_owned()),
            Err(error) => {
                inner.closed = true;
                LoadAnswer::Unreadable(error.to_string())
            }
        }
    }

    /// Puts the save `seq` in the slot (under the mutex: waits at most for
    /// the write in flight) and wakes the loop; returns at once after.
    ///
    /// # Errors
    /// A text over [`MAX_TEXT_BYTES`]: refused, nothing kept.
    pub fn save(&self, seq: u64, text: String) -> Result<(), String> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(format!(
                "the progress is {} bytes, over the {MAX_TEXT_BYTES} cap",
                text.len()
            ));
        }
        let mut inner = self.inner();
        offer(&mut inner.slot, seq, text);
        inner.retry_pending = false;
        drop(inner);
        self.shared.wake.notify_all();
        Ok(())
    }

    /// One step of the loop, without waiting: takes the mutex, re-checks
    /// `closed`, takes the slot and writes it. False when there was nothing
    /// to write (an empty slot, a closed writer).
    pub fn write_once(&self) -> bool {
        let mut inner = self.inner();
        self.shared.write_slot(&mut inner)
    }

    /// Starts the loop on its own thread; it ends once the writer is closed.
    pub fn spawn(&self) -> std::thread::JoinHandle<()> {
        let shared = Arc::clone(&self.shared);
        std::thread::spawn(move || shared.run())
    }

    /// The exit write (`RunEvent::Exit`): waits at most for the write in
    /// flight, writes the slot if it is filled and the writer is not
    /// closed, then closes the writer for good.
    pub fn exit(&self) {
        let mut inner = self.inner();
        inner.retry_pending = false;
        self.shared.write_slot(&mut inner);
        inner.closed = true;
        inner.slot = None;
        drop(inner);
        self.shared.wake.notify_all();
    }
}

impl Shared {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Takes the session's lock unless it is held already.
    fn hold_lock(&self, inner: &mut Inner) -> Result<(), String> {
        if inner.lock.is_some() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        match self.disk.try_lock(&self.path(LOCK_FILE)) {
            Ok(Some(file)) => {
                inner.lock = Some(file);
                Ok(())
            }
            Ok(None) => Err(ALREADY_OPEN.to_string()),
            Err(error) => Err(error.to_string()),
        }
    }

    /// The loop: waits for a save (or the retry's delay), writes it, until
    /// the writer is closed.
    fn run(&self) {
        let mut inner = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if inner.closed {
                return;
            }
            if inner.retry_pending {
                let (guard, _) = self
                    .wake
                    .wait_timeout(inner, self.retry)
                    .unwrap_or_else(PoisonError::into_inner);
                inner = guard;
                inner.retry_pending = false;
                continue;
            }
            if inner.slot.is_none() {
                inner = self
                    .wake
                    .wait(inner)
                    .unwrap_or_else(PoisonError::into_inner);
                continue;
            }
            self.write_slot(&mut inner);
        }
    }

    /// Writes the slot under the mutex the caller holds; false when there
    /// was nothing to write. Never writes without the session's lock.
    fn write_slot(&self, inner: &mut Inner) -> bool {
        if inner.closed || inner.slot.is_none() {
            return false;
        }
        if self.hold_lock(inner).is_err() {
            inner.closed = true;
            return false;
        }
        let Some((seq, text)) = inner.slot.take() else {
            return false;
        };
        match self.write(&text) {
            Outcome::Saved => (self.emit)(StatusEvent::Saved(seq)),
            Outcome::Newer(raw) => {
                inner.closed = true;
                (self.emit)(StatusEvent::NewerVersion { seq, raw });
            }
            Outcome::Transient(error) => {
                log::warn!("progress: the write of save {seq} failed, retried: {error}");
                offer(&mut inner.slot, seq, text);
                inner.retry_pending = true;
                (self.emit)(StatusEvent::Transient(seq));
            }
        }
        true
    }

    /// One write of `text`: the file on disk checked, a corrupt one moved
    /// aside, then temp, fsync, rename, fsync of the directory.
    fn write(&self, text: &str) -> Outcome {
        let file = self.path(PROGRESS_FILE);
        match self.disk.read(&file) {
            Err(error) => return Outcome::Transient(error),
            Ok(Some(bytes)) => match on_disk(&bytes) {
                OnDisk::Readable => {}
                OnDisk::Newer(raw) => return Outcome::Newer(raw),
                OnDisk::Corrupt => {
                    let aside = self.path(&corrupt_name());
                    if let Err(error) = self.disk.rename(&file, &aside) {
                        return Outcome::Transient(error);
                    }
                }
            },
            Ok(None) => {}
        }
        let temp = self.path(TEMP_FILE);
        let steps = self
            .disk
            .write_synced(&temp, text.as_bytes())
            .and_then(|()| self.disk.rename(&temp, &file))
            .and_then(|()| self.disk.sync_dir(&self.dir));
        match steps {
            Ok(()) => Outcome::Saved,
            Err(error) => Outcome::Transient(error),
        }
    }
}

/// What the file on disk is, for a write.
enum OnDisk {
    /// A state this build reads: replaced.
    Readable,
    /// A newer version's: never overwritten.
    Newer(String),
    /// Anything else (not UTF-8, not JSON, not a state): moved aside.
    Corrupt,
}

fn on_disk(bytes: &[u8]) -> OnDisk {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return OnDisk::Corrupt;
    };
    match State::from_json(text) {
        Ok(_) => OnDisk::Readable,
        Err(StateError::UnknownVersion(version)) if version > State::VERSION => {
            OnDisk::Newer(text.to_owned())
        }
        Err(_) => OnDisk::Corrupt,
    }
}

/// `progress.json.corrupt-<milliseconds since the epoch>`.
fn corrupt_name() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    format!("{PROGRESS_FILE}.corrupt-{millis}")
}
