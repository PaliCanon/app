//! The native progress writer, natively, on a temp directory: the order of
//! writes, the exit write, the retry and its put-back, the refusals (a newer
//! file, an unreadable one, a second process), the corrupt file moved aside,
//! the cap. A [`Flaky`] disk fails the step a test names and can hold a
//! write in flight.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use palicanon_app_bridge::{LoadAnswer, MAX_TEXT_BYTES, StatusEvent};
use palicanon_app_desktop::disk::{Disk, RealDisk};
use palicanon_app_desktop::writer::{ALREADY_OPEN, LOCK_FILE, PROGRESS_FILE, Writer, offer};

/// A progress text, told apart by `n`.
fn state(n: u64) -> String {
    format!(r#"{{"version":1,"progress":[],"n":{n}}}"#)
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pc-writer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The real disk, with failures to order and a gate that holds writes.
#[derive(Default)]
struct Flaky {
    fail_read: AtomicUsize,
    fail_write: AtomicUsize,
    fail_rename: AtomicUsize,
    fail_sync_dir: AtomicUsize,
    /// Every text that reached `progress.json` (a successful rename onto it).
    landed: Mutex<Vec<String>>,
    /// While true, `write_synced` waits.
    held: Mutex<bool>,
    gate: Condvar,
    /// How many writes started (to know one is in flight).
    started: AtomicUsize,
}

/// Which failure counter of the flaky disk a step arms.
type Counter = fn(&Flaky) -> &AtomicUsize;

/// Takes one from `counter` unless it is 0 (whether it took one).
fn take(counter: &AtomicUsize) -> bool {
    let mut n = counter.load(Ordering::SeqCst);
    while n > 0 {
        match counter.compare_exchange(n, n - 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return true,
            Err(now) => n = now,
        }
    }
    false
}

fn failure() -> std::io::Error {
    std::io::Error::other("injected")
}

impl Flaky {
    fn hold(&self) {
        *self.held.lock().unwrap() = true;
    }

    fn release(&self) {
        *self.held.lock().unwrap() = false;
        self.gate.notify_all();
    }

    fn landed(&self) -> Vec<String> {
        self.landed.lock().unwrap().clone()
    }
}

/// The flaky disk as the writer holds it (a test keeps another handle).
struct Shared(Arc<Flaky>);

impl std::ops::Deref for Shared {
    type Target = Flaky;
    fn deref(&self) -> &Flaky {
        &self.0
    }
}

impl Disk for Shared {
    fn read(&self, path: &Path) -> std::io::Result<Option<Vec<u8>>> {
        if take(&self.fail_read) {
            return Err(failure());
        }
        RealDisk.read(path)
    }

    fn write_synced(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.started.fetch_add(1, Ordering::SeqCst);
        let mut held = self.held.lock().unwrap();
        while *held {
            held = self.gate.wait(held).unwrap();
        }
        drop(held);
        if take(&self.fail_write) {
            return Err(failure());
        }
        RealDisk.write_synced(path, bytes)
    }

    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        if take(&self.fail_rename) {
            return Err(failure());
        }
        RealDisk.rename(from, to)?;
        if to.file_name().is_some_and(|n| n == PROGRESS_FILE) {
            let text = std::fs::read_to_string(to).unwrap();
            self.landed.lock().unwrap().push(text);
        }
        Ok(())
    }

    fn sync_dir(&self, dir: &Path) -> std::io::Result<()> {
        if take(&self.fail_sync_dir) {
            return Err(failure());
        }
        RealDisk.sync_dir(dir)
    }

    fn try_lock(&self, path: &Path) -> std::io::Result<Option<std::fs::File>> {
        RealDisk.try_lock(path)
    }
}

/// A writer over `dir` with a flaky disk, its events, and a channel of them.
struct Rig {
    dir: PathBuf,
    disk: Arc<Flaky>,
    writer: Writer,
    events: Arc<Mutex<Vec<StatusEvent>>>,
    rx: Mutex<Receiver<StatusEvent>>,
}

impl Rig {
    fn new(name: &str) -> Self {
        Self::over(temp(name), Duration::from_secs(3600))
    }

    fn over(dir: PathBuf, retry: Duration) -> Self {
        let disk = Arc::new(Flaky::default());
        let events = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = channel();
        let sink = Arc::clone(&events);
        let tx = Mutex::new(tx);
        let writer = Writer::new(
            dir.clone(),
            Shared(Arc::clone(&disk)),
            Box::new(move |event| {
                sink.lock().unwrap().push(event.clone());
                let _ = tx.lock().unwrap().send(event);
            }),
            retry,
        );
        Rig {
            dir,
            disk,
            writer,
            events,
            rx: Mutex::new(rx),
        }
    }

    fn events(&self) -> Vec<StatusEvent> {
        self.events.lock().unwrap().clone()
    }

    fn next_event(&self) -> StatusEvent {
        self.rx
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .expect("an event within 10 s")
    }

    fn file(&self) -> Option<String> {
        std::fs::read_to_string(self.dir.join(PROGRESS_FILE)).ok()
    }

    fn corrupt_copies(&self) -> Vec<String> {
        std::fs::read_dir(&self.dir)
            .unwrap()
            .filter_map(|e| {
                let name = e.unwrap().file_name().into_string().unwrap();
                name.starts_with("progress.json.corrupt-")
                    .then(|| std::fs::read_to_string(self.dir.join(name)).unwrap())
            })
            .collect()
    }
}

#[test]
fn a_missing_file_loads_as_missing_and_a_present_one_as_its_text() {
    let rig = Rig::new("load");
    assert_eq!(rig.writer.load(), LoadAnswer::Missing);
    std::fs::write(rig.dir.join(PROGRESS_FILE), state(1)).unwrap();
    assert_eq!(rig.writer.load(), LoadAnswer::Text(state(1)));
    assert!(
        rig.dir.join(LOCK_FILE).exists(),
        "the lock is taken at load"
    );
}

#[test]
fn a_save_is_written_and_says_saved() {
    let rig = Rig::new("save");
    rig.writer.load();
    rig.writer.save(1, state(1)).unwrap();
    assert!(rig.writer.write_once());
    assert_eq!(rig.file(), Some(state(1)));
    assert_eq!(rig.events(), [StatusEvent::Saved(1)]);
    assert!(!rig.writer.write_once(), "the slot is empty");
}

#[test]
fn the_slot_keeps_the_newest_save_only() {
    let mut slot = None;
    offer(&mut slot, 2, state(2));
    offer(&mut slot, 1, state(1));
    assert_eq!(slot, Some((2, state(2))), "an older save is dropped");
    offer(&mut slot, 3, state(3));
    assert_eq!(slot, Some((3, state(3))), "a newer save replaces it");
}

/// A save made while a write is in flight waits for it (one mutex) and is
/// written after it: the file's order is the saves' order.
#[test]
fn a_save_during_a_write_is_written_after_it_in_order() {
    let rig = Rig::over(temp("order"), Duration::from_secs(3600));
    rig.writer.load();
    let _loop = rig.writer.spawn();
    rig.disk.hold();
    rig.writer.save(1, state(1)).unwrap();
    let since = std::time::Instant::now();
    while rig.disk.started.load(Ordering::SeqCst) == 0 {
        assert!(
            since.elapsed() < Duration::from_secs(10),
            "no write started"
        );
        std::thread::yield_now();
    }
    let writer = rig.writer.clone();
    let saving = std::thread::spawn(move || writer.save(2, state(2)));
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !saving.is_finished(),
        "the save waits for the write in flight"
    );
    rig.disk.release();
    saving.join().unwrap().unwrap();
    assert_eq!(rig.next_event(), StatusEvent::Saved(1));
    assert_eq!(rig.next_event(), StatusEvent::Saved(2));
    assert_eq!(rig.disk.landed(), [state(1), state(2)]);
    rig.writer.exit();
}

/// The loop woke up for a save, but Exit took the mutex first: Exit writes
/// the latest, once; the loop's step then writes nothing.
#[test]
fn exit_between_the_wake_up_and_the_write_writes_the_latest_once() {
    let rig = Rig::new("exit-race");
    rig.writer.load();
    rig.writer.save(1, state(1)).unwrap();
    rig.writer.save(2, state(2)).unwrap();
    rig.writer.exit();
    assert!(!rig.writer.write_once(), "nothing after Exit");
    rig.writer.save(3, state(3)).unwrap();
    assert!(!rig.writer.write_once(), "a save after Exit is refused");
    rig.writer.exit();
    assert_eq!(rig.disk.landed(), [state(2)]);
}

#[test]
fn exit_with_an_empty_slot_writes_nothing() {
    let rig = Rig::new("exit-empty");
    rig.writer.load();
    rig.writer.exit();
    assert_eq!(rig.disk.landed(), Vec::<String>::new());
}

/// Every step of a write that fails gives Transient and keeps the text for
/// the retry, which then writes it.
#[test]
fn a_failed_step_gives_transient_and_the_retry_writes_it() {
    let counters: [(&str, Counter); 4] = [
        ("read", |d| &d.fail_read),
        ("write", |d| &d.fail_write),
        ("rename", |d| &d.fail_rename),
        ("fsync of the dir", |d| &d.fail_sync_dir),
    ];
    for (step, counter) in counters {
        let rig = Rig::new(&format!("transient-{}", step.replace(' ', "-")));
        rig.writer.load();
        counter(&rig.disk).store(1, Ordering::SeqCst);
        rig.writer.save(1, state(1)).unwrap();
        assert!(rig.writer.write_once(), "{step}");
        assert_eq!(rig.events(), [StatusEvent::Transient(1)], "{step}");
        assert!(rig.writer.write_once(), "{step}: the text was kept");
        assert_eq!(
            rig.events(),
            [StatusEvent::Transient(1), StatusEvent::Saved(1)],
            "{step}"
        );
        assert_eq!(rig.file(), Some(state(1)), "{step}");
    }
}

/// A corrupt file whose move aside fails: Transient, the file untouched;
/// the retry moves it and writes.
#[test]
fn a_failed_rename_aside_gives_transient_and_the_retry() {
    let rig = Rig::new("aside-fails");
    std::fs::write(rig.dir.join(PROGRESS_FILE), "{not json").unwrap();
    rig.writer.load();
    rig.disk.fail_rename.store(1, Ordering::SeqCst);
    rig.writer.save(1, state(1)).unwrap();
    rig.writer.write_once();
    assert_eq!(rig.events(), [StatusEvent::Transient(1)]);
    assert_eq!(rig.file().as_deref(), Some("{not json"));
    rig.writer.write_once();
    assert_eq!(rig.corrupt_copies(), ["{not json"]);
    assert_eq!(rig.file(), Some(state(1)));
}

/// Transient, then a newer save before the retry: the newer is written, the
/// old one never (the put-back did not replace it).
#[test]
fn a_retrys_put_back_never_replaces_a_newer_save() {
    let rig = Rig::new("put-back");
    rig.writer.load();
    rig.disk.fail_write.store(1, Ordering::SeqCst);
    rig.writer.save(1, state(1)).unwrap();
    rig.writer.write_once();
    rig.writer.save(2, state(2)).unwrap();
    rig.writer.write_once();
    assert!(!rig.writer.write_once());
    assert_eq!(rig.disk.landed(), [state(2)]);
    assert_eq!(
        rig.events(),
        [StatusEvent::Transient(1), StatusEvent::Saved(2)]
    );
}

/// The loop's own retry: Transient, the retry after the delay lands it,
/// then a newer save is written too.
#[test]
fn transient_then_the_retry_then_a_newer_save() {
    let rig = Rig::over(temp("retry-loop"), Duration::from_millis(50));
    rig.writer.load();
    let _loop = rig.writer.spawn();
    rig.disk.fail_write.store(1, Ordering::SeqCst);
    rig.writer.save(1, state(1)).unwrap();
    assert_eq!(rig.next_event(), StatusEvent::Transient(1));
    assert_eq!(rig.next_event(), StatusEvent::Saved(1));
    rig.writer.save(2, state(2)).unwrap();
    assert_eq!(rig.next_event(), StatusEvent::Saved(2));
    assert_eq!(rig.disk.landed(), [state(1), state(2)]);
    rig.writer.exit();
}

/// Exit while a retry waits (a long delay): Exit writes the latest once.
#[test]
fn exit_while_a_retry_waits_writes_the_latest_once() {
    let rig = Rig::over(temp("exit-retry"), Duration::from_secs(3600));
    rig.writer.load();
    let looping = rig.writer.spawn();
    rig.disk.fail_write.store(1, Ordering::SeqCst);
    rig.writer.save(1, state(1)).unwrap();
    assert_eq!(rig.next_event(), StatusEvent::Transient(1));
    rig.writer.exit();
    looping.join().expect("the loop ends after Exit");
    assert_eq!(rig.disk.landed(), [state(1)]);
}

#[test]
fn a_newer_file_is_refused_nothing_written_and_the_event_carries_it() {
    let rig = Rig::new("newer");
    let newer = r#"{"version": 9, "else": true}"#;
    std::fs::write(rig.dir.join(PROGRESS_FILE), newer).unwrap();
    rig.writer.load();
    rig.writer.save(1, state(1)).unwrap();
    rig.writer.write_once();
    assert_eq!(
        rig.events(),
        [StatusEvent::NewerVersion {
            seq: 1,
            raw: newer.into()
        }]
    );
    assert_eq!(rig.file().as_deref(), Some(newer));
    rig.writer.save(2, state(2)).unwrap();
    assert!(!rig.writer.write_once(), "closed after NewerVersion");
    rig.writer.exit();
    assert_eq!(rig.file().as_deref(), Some(newer), "exit writes nothing");
    assert_eq!(rig.disk.landed(), Vec::<String>::new());
}

#[test]
fn an_unreadable_file_shuts_the_writer_and_exit_writes_nothing() {
    let rig = Rig::new("unreadable");
    std::fs::write(rig.dir.join(PROGRESS_FILE), state(0)).unwrap();
    rig.disk.fail_read.store(1, Ordering::SeqCst);
    assert!(matches!(rig.writer.load(), LoadAnswer::Unreadable(_)));
    rig.writer.save(1, state(1)).unwrap();
    assert!(!rig.writer.write_once());
    rig.writer.exit();
    assert_eq!(rig.file(), Some(state(0)));
    assert_eq!(rig.events(), Vec::<StatusEvent>::new());
}

/// A second process (here a second writer: `flock` belongs to the open
/// file) cannot take the lock: it answers Unreadable and never writes.
#[test]
fn a_second_process_without_the_lock_answers_unreadable_and_writes_nothing() {
    let dir = temp("second");
    let first = Rig::over(dir.clone(), Duration::from_secs(3600));
    assert_eq!(first.writer.load(), LoadAnswer::Missing);
    let second = Rig::over(dir, Duration::from_secs(3600));
    assert_eq!(
        second.writer.load(),
        LoadAnswer::Unreadable(ALREADY_OPEN.into())
    );
    second.writer.save(1, state(1)).unwrap();
    assert!(!second.writer.write_once());
    second.writer.exit();
    assert_eq!(second.file(), None);
    assert_eq!(second.disk.landed(), Vec::<String>::new());
}

/// No load, no lock: a writer never writes without holding the lock.
#[test]
fn a_writer_never_writes_without_the_lock() {
    let dir = temp("no-lock");
    let holder = RealDisk.try_lock(&dir.join(LOCK_FILE)).unwrap();
    assert!(holder.is_some());
    let rig = Rig::over(dir, Duration::from_secs(3600));
    rig.writer.save(1, state(1)).unwrap();
    assert!(!rig.writer.write_once());
    rig.writer.exit();
    assert_eq!(rig.file(), None);
}

#[test]
fn a_corrupt_file_is_moved_aside_on_the_first_save_then_the_new_one_written() {
    for corrupt in ["{not json", r#"{"version": 0, "progress": []}"#] {
        let rig = Rig::new("corrupt");
        std::fs::write(rig.dir.join(PROGRESS_FILE), corrupt).unwrap();
        assert_eq!(rig.writer.load(), LoadAnswer::Text(corrupt.into()));
        assert_eq!(rig.file().as_deref(), Some(corrupt), "load moves nothing");
        rig.writer.save(1, state(1)).unwrap();
        rig.writer.write_once();
        assert_eq!(rig.corrupt_copies(), [corrupt]);
        assert_eq!(rig.file(), Some(state(1)));
        assert_eq!(rig.events(), [StatusEvent::Saved(1)]);
    }
}

#[test]
fn a_text_over_the_cap_is_refused_and_not_kept() {
    let rig = Rig::new("cap");
    rig.writer.load();
    let big = "x".repeat(MAX_TEXT_BYTES + 1);
    assert!(rig.writer.save(1, big).is_err());
    assert!(!rig.writer.write_once());
    let exact = " ".repeat(MAX_TEXT_BYTES);
    assert!(
        rig.writer.save(2, exact).is_ok(),
        "the cap itself is allowed"
    );
}
