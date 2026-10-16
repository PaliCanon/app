//! The app's [`FileTransfer`] (048's contract) over the native commands:
//! `export(name, text)` calls `export_file(name, text)` (the Rust side opens
//! the save dialog and shows its own failure); `pick_import(on_done)` calls
//! `import_file()`, whose answer is `Text` (`on_done(Ok(text))`),
//! `Cancelled` (nothing called) or `Failed` (`on_done(Err(PickFailed))`).
//! A pick superseded by a later one calls nothing. The front never holds a
//! path.

use std::cell::Cell;
use std::rc::Rc;

use learn_pali::fragment::{FileTransfer, PickFailed};
use palicanon_app_bridge::ImportAnswer;

/// Where the files go: the native commands in the app, a fake in the tests.
pub trait FilesPort {
    /// `export_file(name, text)`, fire-and-forget.
    fn export(&self, name: String, text: String);
    /// `import_file()`; `done` is called with its answer (a failed call is
    /// `Failed`).
    fn import(&self, done: Box<dyn FnOnce(ImportAnswer)>);
}

/// The app's file transfer.
#[derive(Clone)]
pub struct AppFiles {
    port: Rc<dyn FilesPort>,
    /// The latest pick's number: an older pick's answer calls nothing.
    picks: Rc<Cell<u64>>,
}

impl AppFiles {
    /// The transfer over `port`.
    pub fn new(port: Rc<dyn FilesPort>) -> Self {
        AppFiles {
            port,
            picks: Rc::new(Cell::new(0)),
        }
    }
}

impl FileTransfer for AppFiles {
    fn export(&self, name: &str, text: &str) {
        self.port.export(name.to_owned(), text.to_owned());
    }

    fn pick_import(&self, on_done: Box<dyn FnOnce(Result<String, PickFailed>)>) {
        let pick = self.picks.get() + 1;
        self.picks.set(pick);
        let picks = Rc::clone(&self.picks);
        self.port.import(Box::new(move |answer| {
            if picks.get() != pick {
                return;
            }
            match answer {
                ImportAnswer::Text(text) => on_done(Ok(text)),
                ImportAnswer::Cancelled => {}
                ImportAnswer::Failed(why) => {
                    log(&why);
                    on_done(Err(PickFailed));
                }
            }
        }));
    }
}

/// A failed import's reason, for the console.
fn log(why: &str) {
    #[cfg(target_arch = "wasm32")]
    web_sys::console::warn_1(&format!("import failed: {why}").into());
    #[cfg(not(target_arch = "wasm32"))]
    let _ = why;
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// An import's callback, kept until the test answers it.
    type Done = Box<dyn FnOnce(ImportAnswer)>;

    /// A port that records the exports and keeps the import callbacks.
    #[derive(Default)]
    struct Fake {
        exported: RefCell<Vec<(String, String)>>,
        pending: RefCell<Vec<Done>>,
    }

    impl FilesPort for Fake {
        fn export(&self, name: String, text: String) {
            self.exported.borrow_mut().push((name, text));
        }
        fn import(&self, done: Done) {
            self.pending.borrow_mut().push(done);
        }
    }

    type Got = Rc<RefCell<Vec<Result<String, PickFailed>>>>;

    fn pick(files: &AppFiles) -> Got {
        let got: Got = Rc::default();
        let sink = Rc::clone(&got);
        files.pick_import(Box::new(move |r| sink.borrow_mut().push(r)));
        got
    }

    fn answer(fake: &Fake, index: usize, answer: ImportAnswer) {
        let done = fake.pending.borrow_mut().remove(index);
        done(answer);
    }

    #[test]
    fn export_hands_the_name_and_text_to_the_shell() {
        let fake = Rc::new(Fake::default());
        let files = AppFiles::new(fake.clone());
        files.export("pc_learn-2026-10-16.json", "{}");
        assert_eq!(
            *fake.exported.borrow(),
            [("pc_learn-2026-10-16.json".to_string(), "{}".to_string())]
        );
    }

    #[test]
    fn an_import_answer_maps_to_the_contract() {
        let fake = Rc::new(Fake::default());
        let files = AppFiles::new(fake.clone());
        let got = pick(&files);
        answer(&fake, 0, ImportAnswer::Text("{\"version\":1}".into()));
        assert_eq!(*got.borrow(), [Ok("{\"version\":1}".to_string())]);

        let got = pick(&files);
        answer(&fake, 0, ImportAnswer::Cancelled);
        assert_eq!(got.borrow().len(), 0, "a cancelled pick calls nothing");

        let got = pick(&files);
        answer(&fake, 0, ImportAnswer::Failed("too large".into()));
        assert_eq!(*got.borrow(), [Err(PickFailed)]);
    }

    #[test]
    fn a_superseded_pick_calls_nothing() {
        let fake = Rc::new(Fake::default());
        let files = AppFiles::new(fake.clone());
        let first = pick(&files);
        let second = pick(&files);
        answer(&fake, 0, ImportAnswer::Text("old".into()));
        assert_eq!(first.borrow().len(), 0);
        answer(&fake, 0, ImportAnswer::Text("new".into()));
        assert_eq!(*second.borrow(), [Ok("new".to_string())]);
    }
}
