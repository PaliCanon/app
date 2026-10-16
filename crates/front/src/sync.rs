//! The app's [`SyncTransport`] (task 006): learn-pali 050's sync, over the
//! shell's commands. The cycle (when to pull, the merge, when to push) is
//! learn-pali's; this only carries text and the account between the cycle
//! and the shell, which signs in, keeps the tokens and calls the Drive.
//!
//! - **Built from `sync_account`**, before the mount: `Unavailable` (a build
//!   without the OAuth secret) gives no transport, so no sync UI; a refresh
//!   token in the Keychain gives `SignedIn` at once, so the cycle runs at
//!   load.
//! - **Sign-in**: `sync_sign_in` answers Google's address, which is SHOWN
//!   ([`AppSync::link`], the [`SignInLink`](crate::sync_link::SignInLink)
//!   dialog: selectable, Copy, Open in browser, Cancel) while the shell's
//!   loopback waits; its outcome comes as a `sync-sign-in` event
//!   ([`AppSync::apply`]), by attempt number: an event of another attempt
//!   is ignored. A newer sign-in replaces the one waiting (its callback
//!   answers `Cancelled`). Cancel stops the shell's listener
//!   (`sync_cancel`) and answers `Cancelled` at once.
//! - **A refused token** (`SignedOut` from a pull or a push) turns the
//!   account remembered: the home's Sync chip signs in again.
//! - **The email**: the shell learns it at its first token refresh; after a
//!   pull, a signed-in account with no email asks `sync_account` again.

use std::cell::RefCell;
use std::rc::Rc;

use learn_pali::fragment::{SyncAccount, SyncError, SyncTransport};
use leptos::prelude::*;
use palicanon_app_bridge::{
    PullAnswer, PushAnswer, SignInEvent, SignInStart, SyncAccountAnswer, SyncFailure,
};

/// The shell's sync commands: Tauri's IPC in the app, a fake in the tests.
/// Every call answers through its callback, once.
pub trait SyncPort {
    /// `sync_sign_in()`.
    fn sign_in(&self, done: Box<dyn FnOnce(SignInStart)>);
    /// `sync_cancel()`.
    fn cancel(&self);
    /// `sync_pull()`.
    fn pull(&self, done: Box<dyn FnOnce(PullAnswer)>);
    /// `sync_push(text)`.
    fn push(&self, text: String, done: Box<dyn FnOnce(PushAnswer)>);
    /// `sync_account()`.
    fn account(&self, done: Box<dyn FnOnce(SyncAccountAnswer)>);
    /// `sync_sign_out()`.
    fn sign_out(&self);
}

/// learn-pali's reason for a shell's failure.
pub fn error_of(failure: SyncFailure) -> SyncError {
    match failure {
        SyncFailure::SignedOut => SyncError::SignedOut,
        SyncFailure::Cancelled => SyncError::Cancelled,
        SyncFailure::Network => SyncError::Network,
        SyncFailure::Drive(message) => SyncError::Drive(message),
    }
}

/// The account `sync_account` answered; none when this build has no sync.
pub fn account_of(answer: SyncAccountAnswer) -> Option<SyncAccount> {
    match answer {
        SyncAccountAnswer::Unavailable => None,
        SyncAccountAnswer::SignedOut => Some(SyncAccount::SignedOut),
        SyncAccountAnswer::SignedIn { email } => Some(SyncAccount::SignedIn { email }),
    }
}

/// The callback of the sign-in waiting, by its attempt number.
type Waiting = (u64, Box<dyn FnOnce(Result<(), SyncError>)>);

struct Inner {
    port: Rc<dyn SyncPort>,
    account: RwSignal<SyncAccount>,
    link: RwSignal<Option<String>>,
    waiting: RefCell<Option<Waiting>>,
}

/// The app's sync transport (cheap to clone: one shared state).
#[derive(Clone)]
pub struct AppSync(Rc<Inner>);

impl AppSync {
    /// The transport over `port`, from what `sync_account` answered; none
    /// when this build has no sync.
    pub fn new(answer: SyncAccountAnswer, port: Rc<dyn SyncPort>) -> Option<Self> {
        let account = account_of(answer)?;
        Some(AppSync(Rc::new(Inner {
            port,
            account: RwSignal::new(account),
            link: RwSignal::new(None),
            waiting: RefCell::new(None),
        })))
    }

    /// Google's sign-in address while a sign-in waits; none otherwise.
    pub fn link(&self) -> Signal<Option<String>> {
        self.0.link.into()
    }

    /// The `sync-sign-in` event: the sign-in it names ends (signed in, or
    /// why not); an event of another attempt is ignored.
    pub fn apply(&self, event: SignInEvent) {
        let waiting = self
            .0
            .waiting
            .borrow_mut()
            .take_if(|(attempt, _)| *attempt == event.attempt);
        let Some((_, done)) = waiting else {
            return;
        };
        self.0.link.set(None);
        match event.outcome {
            Ok(email) => {
                self.0.account.set(SyncAccount::SignedIn { email });
                done(Ok(()));
            }
            Err(failure) => done(Err(error_of(failure))),
        }
    }

    /// The link's Cancel (and its Escape): the shell stops waiting, the
    /// link goes, the sign-in answers `Cancelled`.
    pub fn cancel(&self) {
        let waiting = self.0.waiting.borrow_mut().take();
        self.0.link.set(None);
        if let Some((_, done)) = waiting {
            self.0.port.cancel();
            done(Err(SyncError::Cancelled));
        }
    }

    /// A refused token: the account is remembered, with its email.
    fn lost_token(&self) {
        let email = match self.0.account.get_untracked() {
            SyncAccount::SignedIn { email } | SyncAccount::Remembered { email } => email,
            SyncAccount::SignedOut => return,
        };
        self.0.account.set(SyncAccount::Remembered { email });
    }

    /// After a pull: a signed-in account with no email asks the shell,
    /// which knows it after its first refresh.
    fn learn_email(&self) {
        if self.0.account.get_untracked() != (SyncAccount::SignedIn { email: None }) {
            return;
        }
        let account = self.0.account;
        self.0.port.account(Box::new(move |answer| {
            if let SyncAccountAnswer::SignedIn { email: Some(email) } = answer {
                account.set(SyncAccount::SignedIn { email: Some(email) });
            }
        }));
    }

    /// A failed call's reason; a refused token turns the account
    /// remembered.
    fn failed(&self, failure: SyncFailure) -> SyncError {
        if failure == SyncFailure::SignedOut {
            self.lost_token();
        }
        error_of(failure)
    }
}

impl SyncTransport for AppSync {
    fn sign_in(&self, on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {
        let sync = self.clone();
        self.0.port.sign_in(Box::new(move |start| match start {
            SignInStart::Started { attempt, url } => {
                let replaced = sync.0.waiting.borrow_mut().replace((attempt, on_done));
                sync.0.link.set(Some(url));
                if let Some((_, done)) = replaced {
                    done(Err(SyncError::Cancelled));
                }
            }
            SignInStart::Failed(failure) => on_done(Err(error_of(failure))),
        }));
    }

    fn sign_out(&self) {
        let waiting = self.0.waiting.borrow_mut().take();
        self.0.link.set(None);
        self.0.account.set(SyncAccount::SignedOut);
        self.0.port.sign_out();
        if let Some((_, done)) = waiting {
            done(Err(SyncError::Cancelled));
        }
    }

    fn account(&self) -> Signal<SyncAccount> {
        self.0.account.into()
    }

    fn pull(&self, on_done: Box<dyn FnOnce(Result<Option<String>, SyncError>)>) {
        let sync = self.clone();
        self.0.port.pull(Box::new(move |answer| match answer {
            PullAnswer::Absent => {
                sync.learn_email();
                on_done(Ok(None));
            }
            PullAnswer::File(text) => {
                sync.learn_email();
                on_done(Ok(Some(text)));
            }
            PullAnswer::Failed(failure) => on_done(Err(sync.failed(failure))),
        }));
    }

    fn push(&self, text: &str, on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {
        let sync = self.clone();
        self.0.port.push(
            text.to_owned(),
            Box::new(move |answer| match answer {
                PushAnswer::Pushed => on_done(Ok(())),
                PushAnswer::Failed(failure) => on_done(Err(sync.failed(failure))),
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;

    /// A fake shell: what each command answers, and what it was asked.
    #[derive(Default)]
    struct FakePort {
        start: RefCell<Option<SignInStart>>,
        pulled: RefCell<Option<PullAnswer>>,
        pushed: RefCell<Option<PushAnswer>>,
        account: RefCell<Option<SyncAccountAnswer>>,
        texts: RefCell<Vec<String>>,
        cancels: Cell<usize>,
        sign_outs: Cell<usize>,
        accounts_asked: Cell<usize>,
    }

    impl SyncPort for FakePort {
        fn sign_in(&self, done: Box<dyn FnOnce(SignInStart)>) {
            done(self.start.borrow().clone().expect("a start answer"));
        }
        fn cancel(&self) {
            self.cancels.set(self.cancels.get() + 1);
        }
        fn pull(&self, done: Box<dyn FnOnce(PullAnswer)>) {
            done(self.pulled.borrow().clone().expect("a pull answer"));
        }
        fn push(&self, text: String, done: Box<dyn FnOnce(PushAnswer)>) {
            self.texts.borrow_mut().push(text);
            done(self.pushed.borrow().clone().expect("a push answer"));
        }
        fn account(&self, done: Box<dyn FnOnce(SyncAccountAnswer)>) {
            self.accounts_asked.set(self.accounts_asked.get() + 1);
            done(self.account.borrow().clone().expect("an account answer"));
        }
        fn sign_out(&self) {
            self.sign_outs.set(self.sign_outs.get() + 1);
        }
    }

    fn sync_over(answer: SyncAccountAnswer) -> (AppSync, Rc<FakePort>) {
        let port = Rc::new(FakePort::default());
        let sync = AppSync::new(answer, port.clone()).expect("a sync");
        (sync, port)
    }

    fn email(email: &str) -> SyncAccount {
        SyncAccount::SignedIn {
            email: Some(email.into()),
        }
    }

    /// What a callback received, and the callback.
    type Recorded<T> = (Rc<RefCell<Vec<T>>>, Box<dyn FnOnce(T)>);

    /// A callback that records what it receives.
    fn recorder<T: 'static>() -> Recorded<T> {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let into = seen.clone();
        (seen, Box::new(move |value| into.borrow_mut().push(value)))
    }

    fn started(attempt: u64) -> SignInStart {
        SignInStart::Started {
            attempt,
            url: format!("https://accounts.google.com/o/oauth2/v2/auth?n={attempt}"),
        }
    }

    #[test]
    fn a_build_without_sync_has_no_transport() {
        let port = Rc::new(FakePort::default());
        assert!(AppSync::new(SyncAccountAnswer::Unavailable, port).is_none());
    }

    #[test]
    fn the_account_at_start_is_the_shells() {
        let (signed_out, _) = sync_over(SyncAccountAnswer::SignedOut);
        assert_eq!(signed_out.account().get_untracked(), SyncAccount::SignedOut);
        let (signed_in, _) = sync_over(SyncAccountAnswer::SignedIn {
            email: Some("a@b".into()),
        });
        assert_eq!(signed_in.account().get_untracked(), email("a@b"));
    }

    #[test]
    fn a_sign_in_shows_the_link_then_its_event_signs_in() {
        let (sync, port) = sync_over(SyncAccountAnswer::SignedOut);
        *port.start.borrow_mut() = Some(started(1));
        let (seen, done) = recorder();
        sync.sign_in(done);
        assert_eq!(
            sync.link().get_untracked().as_deref(),
            Some("https://accounts.google.com/o/oauth2/v2/auth?n=1")
        );
        assert_eq!(seen.borrow().len(), 0, "the sign-in waits for Google");
        sync.apply(SignInEvent {
            attempt: 1,
            outcome: Ok(Some("a@b".into())),
        });
        assert_eq!(*seen.borrow(), vec![Ok(())]);
        assert_eq!(sync.link().get_untracked(), None);
        assert_eq!(sync.account().get_untracked(), email("a@b"));
    }

    #[test]
    fn an_event_of_another_attempt_is_ignored() {
        let (sync, port) = sync_over(SyncAccountAnswer::SignedOut);
        *port.start.borrow_mut() = Some(started(2));
        let (seen, done) = recorder();
        sync.sign_in(done);
        sync.apply(SignInEvent {
            attempt: 1,
            outcome: Err(SyncFailure::Cancelled),
        });
        assert_eq!(seen.borrow().len(), 0);
        assert!(sync.link().get_untracked().is_some());
    }

    #[test]
    fn a_failed_sign_in_is_the_reason_and_the_link_goes() {
        let (sync, port) = sync_over(SyncAccountAnswer::SignedOut);
        *port.start.borrow_mut() = Some(started(1));
        let (seen, done) = recorder();
        sync.sign_in(done);
        sync.apply(SignInEvent {
            attempt: 1,
            outcome: Err(SyncFailure::Network),
        });
        assert_eq!(*seen.borrow(), vec![Err(SyncError::Network)]);
        assert_eq!(sync.link().get_untracked(), None);
        assert_eq!(sync.account().get_untracked(), SyncAccount::SignedOut);
        *port.start.borrow_mut() = Some(SignInStart::Failed(SyncFailure::Network));
        let (seen, done) = recorder();
        sync.sign_in(done);
        assert_eq!(*seen.borrow(), vec![Err(SyncError::Network)]);
        assert_eq!(sync.link().get_untracked(), None);
    }

    #[test]
    fn cancel_stops_the_shell_hides_the_link_and_answers_cancelled() {
        let (sync, port) = sync_over(SyncAccountAnswer::SignedOut);
        *port.start.borrow_mut() = Some(started(1));
        let (seen, done) = recorder();
        sync.sign_in(done);
        sync.cancel();
        assert_eq!(port.cancels.get(), 1);
        assert_eq!(*seen.borrow(), vec![Err(SyncError::Cancelled)]);
        assert_eq!(sync.link().get_untracked(), None);
        sync.apply(SignInEvent {
            attempt: 1,
            outcome: Err(SyncFailure::Cancelled),
        });
        assert_eq!(seen.borrow().len(), 1, "the late event changes nothing");
        sync.cancel();
        assert_eq!(port.cancels.get(), 1, "nothing waits: nothing to cancel");
    }

    #[test]
    fn a_newer_sign_in_answers_the_one_waiting_cancelled() {
        let (sync, port) = sync_over(SyncAccountAnswer::SignedOut);
        *port.start.borrow_mut() = Some(started(1));
        let (first, done) = recorder();
        sync.sign_in(done);
        *port.start.borrow_mut() = Some(started(2));
        let (second, done) = recorder();
        sync.sign_in(done);
        assert_eq!(*first.borrow(), vec![Err(SyncError::Cancelled)]);
        assert_eq!(second.borrow().len(), 0);
        assert!(sync.link().get_untracked().unwrap().ends_with("n=2"));
    }

    #[test]
    fn a_pull_and_a_push_carry_the_text_both_ways() {
        let (sync, port) = sync_over(SyncAccountAnswer::SignedIn {
            email: Some("a@b".into()),
        });
        *port.pulled.borrow_mut() = Some(PullAnswer::File("{\"version\":1}".into()));
        let (seen, done) = recorder();
        sync.pull(done);
        assert_eq!(
            *seen.borrow(),
            vec![Ok(Some("{\"version\":1}".to_string()))]
        );
        *port.pulled.borrow_mut() = Some(PullAnswer::Absent);
        let (seen, done) = recorder();
        sync.pull(done);
        assert_eq!(*seen.borrow(), vec![Ok(None)]);
        *port.pushed.borrow_mut() = Some(PushAnswer::Pushed);
        let (seen, done) = recorder();
        sync.push("{}", done);
        assert_eq!(*seen.borrow(), vec![Ok(())]);
        assert_eq!(*port.texts.borrow(), vec!["{}".to_string()]);
        assert_eq!(port.accounts_asked.get(), 0, "the email is known");
    }

    #[test]
    fn a_refused_token_turns_the_account_remembered() {
        let (sync, port) = sync_over(SyncAccountAnswer::SignedIn {
            email: Some("a@b".into()),
        });
        *port.pulled.borrow_mut() = Some(PullAnswer::Failed(SyncFailure::SignedOut));
        let (seen, done) = recorder();
        sync.pull(done);
        assert_eq!(*seen.borrow(), vec![Err(SyncError::SignedOut)]);
        assert_eq!(
            sync.account().get_untracked(),
            SyncAccount::Remembered {
                email: Some("a@b".into())
            }
        );
        *port.pushed.borrow_mut() =
            Some(PushAnswer::Failed(SyncFailure::Drive("403 quota".into())));
        let (seen, done) = recorder();
        sync.push("{}", done);
        assert_eq!(
            *seen.borrow(),
            vec![Err(SyncError::Drive("403 quota".into()))]
        );
    }

    #[test]
    fn after_a_pull_an_unknown_email_is_asked_of_the_shell() {
        let (sync, port) = sync_over(SyncAccountAnswer::SignedIn { email: None });
        *port.pulled.borrow_mut() = Some(PullAnswer::Absent);
        *port.account.borrow_mut() = Some(SyncAccountAnswer::SignedIn {
            email: Some("a@b".into()),
        });
        let (_, done) = recorder();
        sync.pull(done);
        assert_eq!(port.accounts_asked.get(), 1);
        assert_eq!(sync.account().get_untracked(), email("a@b"));
    }

    #[test]
    fn sign_out_tells_the_shell_and_forgets_the_account() {
        let (sync, port) = sync_over(SyncAccountAnswer::SignedIn {
            email: Some("a@b".into()),
        });
        sync.sign_out();
        assert_eq!(port.sign_outs.get(), 1);
        assert_eq!(sync.account().get_untracked(), SyncAccount::SignedOut);
    }

    #[test]
    fn every_failure_is_learn_palis_reason() {
        assert_eq!(error_of(SyncFailure::SignedOut), SyncError::SignedOut);
        assert_eq!(error_of(SyncFailure::Cancelled), SyncError::Cancelled);
        assert_eq!(error_of(SyncFailure::Network), SyncError::Network);
        assert_eq!(
            error_of(SyncFailure::Drive("x".into())),
            SyncError::Drive("x".into())
        );
    }
}
