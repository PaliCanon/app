//! The app's [`ProgressStore`]: a mirror of the native writer
//! (`crates/desktop`'s `writer`), in the webview.
//!
//! - **Load**: built from what `progress_load` answered, BEFORE the app is
//!   mounted: `Text(raw)` → `learn_pali::classify(raw)`, `Missing` → `Empty`,
//!   `Unreadable(msg)` → `Load::Unreadable(msg)`, and `Load::Unreadable` too
//!   when the call itself failed (the home shows its error and Reload).
//! - **Save**: the memory is updated at once, the next `seq` taken, the
//!   status set to `Pending`, and `progress_save(seq, json)` called
//!   fire-and-forget. Refused here while the memory is `Unreadable` or
//!   `NewerVersion` (048's guard is the store's).
//! - **Status**: follows the `progress-status` events by `seq`. Tauri does
//!   not promise their order: an event about a save older than the latest
//!   one taken is ignored, so neither an older `Saved` nor an older
//!   `Transient` overrides a newer verdict or a newer `Pending`; a
//!   `Transient` for a save already `Saved` is ignored too. `Transient` is
//!   `Failed(Transient)`, back to `Saved` when a later write lands.
//! - **NewerVersion(seq, raw)** is terminal: every event after it is
//!   ignored, whatever its `seq`. ONE synchronous run: `unsaved` = the memory
//!   before the swap (the latest save), memory = `classify(raw)`, status =
//!   `Failed(NewerVersion { unsaved })`, then the `on_change` callbacks.
//!
//! No timer and no page-hide hook: the native writer keeps the slot and
//! writes it on exit (`WebKit` freezes timers in a hidden window).

use std::cell::RefCell;
use std::rc::Rc;

use learn_core::progress::State;
use learn_pali::fragment::{Load, ProgressStore, SaveError, SaveStatus, classify};
use leptos::prelude::*;
use palicanon_app_bridge::{LoadAnswer, StatusEvent};

/// Where a save goes: `progress_save(seq, text)` in the app, a fake in the
/// tests. Fire-and-forget: the verdict comes back as an event.
pub trait ProgressPort {
    /// Sends the save `seq`.
    fn save(&self, seq: u64, text: String);
}

/// The app's progress store (cheap to clone: one shared state).
#[derive(Clone)]
pub struct AppStore {
    inner: Rc<RefCell<Inner>>,
    status: RwSignal<SaveStatus>,
    port: Rc<dyn ProgressPort>,
}

struct Inner {
    memory: Load,
    /// The latest save's number (0: none yet).
    taken: u64,
    /// The latest save is on disk.
    saved: bool,
    /// A `NewerVersion` was applied: nothing more is.
    terminal: bool,
    callbacks: Vec<Rc<dyn Fn()>>,
}

/// The memory `progress_load`'s answer (or its failure) gives.
pub fn memory_of(answer: Result<LoadAnswer, String>) -> Load {
    match answer {
        Ok(LoadAnswer::Text(raw)) => classify(Some(&raw)),
        Ok(LoadAnswer::Missing) => Load::Empty,
        Ok(LoadAnswer::Unreadable(why)) | Err(why) => Load::Unreadable(why),
    }
}

impl AppStore {
    /// The store over what `progress_load` answered, its saves to `port`.
    pub fn new(answer: Result<LoadAnswer, String>, port: Rc<dyn ProgressPort>) -> Self {
        AppStore {
            inner: Rc::new(RefCell::new(Inner {
                memory: memory_of(answer),
                taken: 0,
                saved: true,
                terminal: false,
                callbacks: Vec::new(),
            })),
            status: RwSignal::new(SaveStatus::Saved),
            port,
        }
    }

    /// Applies a `progress-status` event (see the module's rules).
    pub fn apply(&self, event: StatusEvent) {
        let mut inner = self.inner.borrow_mut();
        if inner.terminal {
            return;
        }
        if let StatusEvent::NewerVersion { raw, .. } = event {
            let unsaved = match &inner.memory {
                Load::Ok(state) => state.clone(),
                _ => State::empty(),
            };
            inner.memory = classify(Some(&raw));
            inner.terminal = true;
            let callbacks = inner.callbacks.clone();
            drop(inner);
            self.status
                .set(SaveStatus::Failed(SaveError::NewerVersion { unsaved }));
            for callback in callbacks {
                callback();
            }
            return;
        }
        if event.seq() < inner.taken {
            return;
        }
        match event {
            StatusEvent::Saved(_) => {
                inner.saved = true;
                drop(inner);
                self.status.set(SaveStatus::Saved);
            }
            StatusEvent::Transient(_) if !inner.saved => {
                drop(inner);
                self.status.set(SaveStatus::Failed(SaveError::Transient));
            }
            StatusEvent::Pending(_) if !inner.saved => {
                drop(inner);
                self.status.set(SaveStatus::Pending);
            }
            _ => {}
        }
    }
}

impl ProgressStore for AppStore {
    fn load(&self) -> Load {
        self.inner.borrow().memory.clone()
    }

    fn save(&self, state: &State) {
        let mut inner = self.inner.borrow_mut();
        if inner.terminal
            || matches!(
                inner.memory,
                Load::Unreadable(_) | Load::NewerVersion { .. }
            )
        {
            return;
        }
        inner.memory = Load::Ok(state.clone());
        inner.taken += 1;
        inner.saved = false;
        let seq = inner.taken;
        drop(inner);
        self.status.set(SaveStatus::Pending);
        self.port.save(seq, state.to_json());
    }

    fn status(&self) -> Signal<SaveStatus> {
        self.status.into()
    }

    fn on_change(&self, callback: Box<dyn Fn()>) {
        self.inner.borrow_mut().callbacks.push(Rc::from(callback));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use learn_core::date::Date;
    use learn_core::deck::TermId;
    use learn_core::progress::{Progress, State};

    use super::*;

    /// A port that records the saves it was sent.
    #[derive(Default)]
    struct Sent(RefCell<Vec<(u64, String)>>);

    impl ProgressPort for Sent {
        fn save(&self, seq: u64, text: String) {
            self.0.borrow_mut().push((seq, text));
        }
    }

    /// A state with `n` terms, told apart by `n`.
    fn state(n: usize) -> State {
        let today = Date::parse("2026-10-16").unwrap();
        let mut state = State::empty();
        state.last_today = Some(today);
        for i in 0..n {
            state
                .progress
                .push(Progress::new(TermId(format!("term {i}")), today));
        }
        state
    }

    fn store_over(answer: Result<LoadAnswer, String>) -> (AppStore, Rc<Sent>) {
        let sent = Rc::new(Sent::default());
        let port: Rc<dyn ProgressPort> = sent.clone();
        (AppStore::new(answer, port), sent)
    }

    fn status(store: &AppStore) -> SaveStatus {
        store.status().get_untracked()
    }

    #[test]
    fn the_load_answer_gives_the_memory() {
        let valid = state(1).to_json();
        assert_eq!(memory_of(Ok(LoadAnswer::Text(valid))), Load::Ok(state(1)));
        assert_eq!(
            memory_of(Ok(LoadAnswer::Text("{nope".into()))),
            Load::Invalid {
                raw: "{nope".into()
            }
        );
        assert_eq!(memory_of(Ok(LoadAnswer::Missing)), Load::Empty);
        assert_eq!(
            memory_of(Ok(LoadAnswer::Unreadable("denied".into()))),
            Load::Unreadable("denied".into())
        );
        let newer = r#"{"version": 9}"#;
        assert_eq!(
            memory_of(Ok(LoadAnswer::Text(newer.into()))),
            Load::NewerVersion { raw: newer.into() }
        );
    }

    #[test]
    fn a_failed_load_call_gives_unreadable() {
        Owner::new().with(|| {
            let (store, _) = store_over(Err("ipc refused".into()));
            assert_eq!(store.load(), Load::Unreadable("ipc refused".into()));
        });
    }

    #[test]
    fn a_save_updates_the_memory_at_once_and_sends_the_next_seq() {
        Owner::new().with(|| {
            let (store, sent) = store_over(Ok(LoadAnswer::Missing));
            assert_eq!(status(&store), SaveStatus::Saved);
            store.save(&state(1));
            assert_eq!(store.load(), Load::Ok(state(1)));
            assert_eq!(status(&store), SaveStatus::Pending);
            store.save(&state(2));
            let seqs: Vec<u64> = sent.0.borrow().iter().map(|(s, _)| *s).collect();
            assert_eq!(seqs, [1, 2]);
            assert_eq!(sent.0.borrow()[1].1, state(2).to_json());
        });
    }

    /// Tauri does not promise the events' order: an older Saved or
    /// Transient arriving late changes nothing; the latest's verdict does.
    #[test]
    fn an_old_saved_or_transient_delivered_late_changes_nothing() {
        Owner::new().with(|| {
            let (store, _) = store_over(Ok(LoadAnswer::Missing));
            store.save(&state(1));
            store.save(&state(2));
            store.apply(StatusEvent::Saved(1));
            assert_eq!(status(&store), SaveStatus::Pending);
            store.apply(StatusEvent::Transient(1));
            assert_eq!(status(&store), SaveStatus::Pending);
            store.apply(StatusEvent::Saved(2));
            assert_eq!(status(&store), SaveStatus::Saved);
            store.apply(StatusEvent::Transient(2));
            assert_eq!(
                status(&store),
                SaveStatus::Saved,
                "a late Transient of a saved seq"
            );
            store.apply(StatusEvent::Transient(1));
            assert_eq!(status(&store), SaveStatus::Saved);
        });
    }

    #[test]
    fn transient_is_failed_then_saved_when_a_later_write_lands() {
        Owner::new().with(|| {
            let (store, _) = store_over(Ok(LoadAnswer::Missing));
            store.save(&state(1));
            store.apply(StatusEvent::Transient(1));
            assert_eq!(status(&store), SaveStatus::Failed(SaveError::Transient));
            store.apply(StatusEvent::Saved(1));
            assert_eq!(status(&store), SaveStatus::Saved);
        });
    }

    /// `NewerVersion` is one synchronous run: by the time `apply` returns, the
    /// callback has run and seen the swapped memory and the status.
    #[test]
    fn newer_version_swaps_sets_the_status_and_calls_on_change_at_once() {
        Owner::new().with(|| {
            let (store, sent) = store_over(Ok(LoadAnswer::Missing));
            let seen = Rc::new(RefCell::new(None));
            let calls = Rc::new(Cell::new(0));
            let (in_callback, count) = (Rc::clone(&seen), Rc::clone(&calls));
            let reader = store.clone();
            store.on_change(Box::new(move || {
                count.set(count.get() + 1);
                *in_callback.borrow_mut() = Some((reader.load(), reader.status().get_untracked()));
            }));
            store.save(&state(1));
            store.save(&state(2));
            let raw = r#"{"version": 9}"#.to_string();
            store.apply(StatusEvent::NewerVersion {
                seq: 1,
                raw: raw.clone(),
            });
            assert_eq!(calls.get(), 1);
            let expected_status = SaveStatus::Failed(SaveError::NewerVersion { unsaved: state(2) });
            assert_eq!(
                seen.borrow().clone(),
                Some((
                    Load::NewerVersion { raw: raw.clone() },
                    expected_status.clone()
                ))
            );
            // Terminal: later events change nothing, whatever their seq.
            store.apply(StatusEvent::Saved(2));
            store.apply(StatusEvent::Saved(99));
            assert_eq!(status(&store), expected_status);
            store.apply(StatusEvent::NewerVersion {
                seq: 3,
                raw: "{}".into(),
            });
            assert_eq!(calls.get(), 1);
            // The guard: no save after it.
            store.save(&state(3));
            assert_eq!(sent.0.borrow().len(), 2);
            assert_eq!(store.load(), Load::NewerVersion { raw });
        });
    }

    #[test]
    fn saves_are_refused_while_unreadable_or_newer() {
        Owner::new().with(|| {
            for answer in [
                Ok(LoadAnswer::Unreadable("denied".into())),
                Err("ipc".into()),
                Ok(LoadAnswer::Text(r#"{"version": 5}"#.into())),
            ] {
                let (store, sent) = store_over(answer);
                let before = store.load();
                store.save(&state(1));
                assert_eq!(sent.0.borrow().len(), 0);
                assert_eq!(store.load(), before);
                assert_eq!(status(&store), SaveStatus::Saved);
            }
        });
    }

    /// An invalid file is not a refusal: the first save replaces it (the
    /// native writer moves it aside).
    #[test]
    fn an_invalid_file_is_saved_over() {
        Owner::new().with(|| {
            let (store, sent) = store_over(Ok(LoadAnswer::Text("{nope".into())));
            store.save(&state(1));
            assert_eq!(sent.0.borrow().len(), 1);
        });
    }
}
