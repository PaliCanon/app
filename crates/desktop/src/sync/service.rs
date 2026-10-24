//! The desktop's side of the sync: the sign-in, the tokens, and the Drive
//! calls, for the commands. Text only crosses to the webview; the cycle
//! (when to pull, the merge, when to push) is learn-pali's, in the front.
//!
//! - **Sign-in**: fresh [`Proofs`], a [`Loopback`] bound, Google's address
//!   built; a newer sign-in cancels the one waiting. When the loopback gives
//!   the code, it is exchanged with the verifier; the refresh token goes to
//!   the [`Secrets`] (the Keychain), the access token stays in memory with
//!   its expiry (a minute early), and the email is asked of userinfo (best
//!   effort).
//! - **Calls**: an access token in memory is used until it expires, then
//!   refreshed from the Keychain's refresh token. A Drive 401 drops it and
//!   refreshes ONCE, then the call is retried once; a second 401 is
//!   `SignedOut`. A refresh token Google refuses (`invalid_grant`) is
//!   deleted: signed out.
//! - **The file**: a pull lists the app folder (`drive_location`, the
//!   oldest file first) and reads the file; the push updates the file the
//!   last pull found, or creates it. The Drive file id is logged at every
//!   pull and push, for the user's check that the site and the app see the
//!   same file.
//! - **Sign-out**: the refresh token is revoked at Google (best effort,
//!   which ends the grant), then deleted; the memory is cleared.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use learn_pali::drive_location;
use palicanon_app_bridge::{PullAnswer, PushAnswer, SignInEvent, SyncAccountAnswer, SyncFailure};
use reqwest::header::CONTENT_TYPE;
use tokio::sync::oneshot;

use super::google::{self, Endpoints, OAuthClient, Tokens, checked, send};
use super::keychain::{SecretError, Secrets};
use super::loopback::{Loopback, SIGN_IN_TIMEOUT, WaitError};
use super::pkce::{AuthRequest, Proofs, auth_url};

/// What the learner reads when the Keychain keeps an item this build
/// cannot replace, even after a delete.
const OLDER_SIGN_IN: &str = "The Keychain holds an older PaliCanon sign-in this build cannot replace. Sign out, then sign in again.";

/// What the token is asked for: where the progress lives and `email`, to
/// show the account (as the site).
pub fn scope() -> String {
    format!("{} email", drive_location::SCOPE)
}

/// A sign-in waiting for Google's answer.
pub struct PendingSignIn {
    /// Its number: its [`SignInEvent`] carries it.
    pub attempt: u64,
    /// Google's sign-in address, to show and open.
    pub url: String,
    /// Waits for the answer, exchanges the code, keeps the tokens.
    pub finish: Pin<Box<dyn Future<Output = SignInEvent> + Send>>,
}

/// An access token and when it stops being used.
struct Access {
    token: String,
    until: Instant,
}

/// What the sync holds in memory.
#[derive(Default)]
struct Session {
    access: Option<Access>,
    file_id: Option<String>,
    email: Option<String>,
    cancel: Option<oneshot::Sender<()>>,
}

struct Inner {
    client: OAuthClient,
    endpoints: Endpoints,
    http: reqwest::Client,
    secrets: Arc<dyn Secrets>,
    session: Mutex<Session>,
    attempts: AtomicU64,
    sign_in_timeout: Duration,
}

/// The sync's native side. Cheap to clone: one shared state.
#[derive(Clone)]
pub struct SyncService(Arc<Inner>);

impl SyncService {
    /// The sync of `client` at `endpoints`, the refresh token in `secrets`.
    pub fn new(client: OAuthClient, endpoints: Endpoints, secrets: Arc<dyn Secrets>) -> Self {
        SyncService(Arc::new(Inner {
            client,
            endpoints,
            http: google::http_client(),
            secrets,
            session: Mutex::default(),
            attempts: AtomicU64::new(0),
            sign_in_timeout: SIGN_IN_TIMEOUT,
        }))
    }

    /// The same, with another sign-in timeout (the tests').
    ///
    /// # Panics
    /// When the service was cloned already.
    #[must_use]
    pub fn with_sign_in_timeout(mut self, timeout: Duration) -> Self {
        Arc::get_mut(&mut self.0)
            .expect("set before the service is shared")
            .sign_in_timeout = timeout;
        self
    }

    fn session(&self) -> MutexGuard<'_, Session> {
        self.0
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Who is signed in: a refresh token in the Keychain, and the email
    /// when known. No network.
    pub fn account(&self) -> SyncAccountAnswer {
        match self.0.secrets.get() {
            Ok(Some(_)) => SyncAccountAnswer::SignedIn {
                email: self.session().email.clone(),
            },
            Ok(None) => SyncAccountAnswer::SignedOut,
            Err(why) => {
                log::warn!("sync: the Keychain could not be read: {why}");
                SyncAccountAnswer::SignedOut
            }
        }
    }

    /// Begins a sign-in: the loopback bound, Google's address built; the
    /// sign-in waiting before is cancelled.
    ///
    /// # Errors
    /// [`SyncFailure::Network`] when no loopback socket could be bound.
    pub async fn start_sign_in(&self) -> Result<PendingSignIn, SyncFailure> {
        let loopback = Loopback::bind().await.map_err(|error| {
            log::warn!("sync: no loopback socket: {error}");
            SyncFailure::Network
        })?;
        let proofs = Proofs::new();
        let redirect = loopback.redirect_uri();
        let url = auth_url(
            AuthRequest {
                endpoint: &self.0.endpoints.auth,
                client_id: &self.0.client.id,
                redirect_uri: &redirect,
                scope: &scope(),
            },
            &proofs,
        );
        let (cancel, cancelled) = oneshot::channel();
        if let Some(previous) = self.session().cancel.replace(cancel) {
            let _ = previous.send(());
        }
        let attempt = self.0.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        let service = self.clone();
        let finish = Box::pin(async move {
            let outcome = service
                .finish_sign_in(loopback, &proofs, &redirect, cancelled)
                .await;
            SignInEvent { attempt, outcome }
        });
        Ok(PendingSignIn {
            attempt,
            url,
            finish,
        })
    }

    /// Stops the sign-in waiting, if any (its listener is closed).
    pub fn cancel(&self) {
        if let Some(cancel) = self.session().cancel.take() {
            let _ = cancel.send(());
        }
    }

    async fn finish_sign_in(
        &self,
        loopback: Loopback,
        proofs: &Proofs,
        redirect: &str,
        cancelled: oneshot::Receiver<()>,
    ) -> Result<Option<String>, SyncFailure> {
        let code = loopback
            .wait(&proofs.state, self.0.sign_in_timeout, cancelled)
            .await
            .map_err(|ended| {
                log::info!("sync: the sign-in ended: {ended:?}");
                match ended {
                    WaitError::WrongState => {
                        SyncFailure::Drive("the sign-in answer was not this sign-in's".into())
                    }
                    _ => SyncFailure::Cancelled,
                }
            })?;
        let tokens = self.exchange(&code, &proofs.verifier, redirect).await?;
        let refresh = tokens
            .refresh
            .clone()
            .ok_or_else(|| SyncFailure::Drive("Google gave no refresh token".into()))?;
        self.store(&refresh)?;
        self.keep(&tokens);
        self.session().file_id = None;
        let email = self.fetch_email(&tokens.access).await;
        self.session().email.clone_from(&email);
        log::info!("sync: signed in");
        Ok(email)
    }

    /// The code exchange (`authorization_code`, with the PKCE verifier).
    async fn exchange(
        &self,
        code: &str,
        verifier: &str,
        redirect: &str,
    ) -> Result<Tokens, SyncFailure> {
        let body = google::form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect),
            ("client_id", &self.0.client.id),
            ("client_secret", &self.0.client.secret),
        ]);
        let (status, answer) = send(self.form_post(&self.0.endpoints.token, body)).await?;
        google::tokens_of(status, &answer)
    }

    fn form_post(&self, url: &str, body: String) -> reqwest::RequestBuilder {
        self.0
            .http
            .post(url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body)
    }

    /// Keeps the access token in memory, until a minute before it expires.
    fn keep(&self, tokens: &Tokens) {
        let life = Duration::from_secs(tokens.expires_in.saturating_sub(60));
        self.session().access = Some(Access {
            token: tokens.access.clone(),
            until: Instant::now() + life,
        });
    }

    /// The account's email, from userinfo; none when it does not say.
    async fn fetch_email(&self, access: &str) -> Option<String> {
        let request = self
            .0
            .http
            .get(&self.0.endpoints.userinfo)
            .bearer_auth(access);
        match send(request).await {
            Ok((200, body)) => google::userinfo_email(&body),
            _ => None,
        }
    }
}

impl SyncService {
    /// The access token in memory, or a fresh one.
    async fn access_token(&self) -> Result<String, SyncFailure> {
        let kept = self
            .session()
            .access
            .as_ref()
            .filter(|access| Instant::now() < access.until)
            .map(|access| access.token.clone());
        match kept {
            Some(token) => Ok(token),
            None => self.refresh().await,
        }
    }

    /// A new access token from the Keychain's refresh token. A refused one
    /// is deleted: signed out.
    async fn refresh(&self) -> Result<String, SyncFailure> {
        let refresh = match self.0.secrets.get() {
            Ok(Some(refresh)) => refresh,
            Ok(None) => return Err(SyncFailure::SignedOut),
            Err(why) => {
                // An item this build may not read (or the learner refused
                // the Keychain prompt): said as it is, so the reason line
                // names the Keychain, not a sign-out (review 006, deep);
                // signing in again replaces the item.
                log::warn!("sync: the Keychain could not be read: {why}");
                return Err(SyncFailure::Storage(format!(
                    "the Keychain could not be read: {why}"
                )));
            }
        };
        let body = google::form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", &self.0.client.id),
            ("client_secret", &self.0.client.secret),
        ]);
        let (status, answer) = send(self.form_post(&self.0.endpoints.token, body)).await?;
        let tokens = match google::tokens_of(status, &answer) {
            Ok(tokens) => tokens,
            Err(SyncFailure::SignedOut) => {
                log::info!("sync: Google refused the refresh token; signed out");
                self.forget();
                return Err(SyncFailure::SignedOut);
            }
            Err(failure) => return Err(failure),
        };
        self.keep(&tokens);
        if self.session().email.is_none() {
            let email = self.fetch_email(&tokens.access).await;
            self.session().email = email;
        }
        Ok(tokens.access)
    }

    /// Keeps the refresh token in the Keychain. An item another build left
    /// (the duplicate) is deleted and the token set once more; a second
    /// refusal is local storage's failure, never Google's.
    fn store(&self, refresh: &str) -> Result<(), SyncFailure> {
        let first = match self.0.secrets.set(refresh) {
            Err(SecretError::Duplicate) => {
                log::warn!("sync: the Keychain holds an older sign-in; replacing it");
                if let Err(why) = self.0.secrets.delete() {
                    log::warn!("sync: the older Keychain item could not be deleted: {why}");
                }
                self.0.secrets.set(refresh)
            }
            other => other,
        };
        first.map_err(|error| {
            log::warn!("sync: the sign-in could not be stored: {error}");
            SyncFailure::Storage(match error {
                SecretError::Duplicate => OLDER_SIGN_IN.into(),
                SecretError::Other(why) => why,
            })
        })
    }

    /// Deletes the refresh token and clears the memory.
    fn forget(&self) {
        if let Err(why) = self.0.secrets.delete() {
            log::warn!("sync: the Keychain item could not be deleted: {why}");
        }
        let cancel = self.session().cancel.take();
        *self.session() = Session {
            cancel,
            ..Session::default()
        };
    }

    /// Sends `build(token)`; on a 401, refreshes once and sends it again.
    async fn authorized(
        &self,
        build: impl Fn(&str) -> reqwest::RequestBuilder,
    ) -> Result<(u16, String), SyncFailure> {
        let token = self.access_token().await?;
        let (status, body) = send(build(&token)).await?;
        if status != 401 {
            return Ok((status, body));
        }
        self.session().access = None;
        let token = self.refresh().await?;
        let answer = send(build(&token)).await?;
        if answer.0 == 401 {
            self.session().access = None;
        }
        Ok(answer)
    }
}

impl SyncService {
    /// `sync_pull`: the progress file, as is, or none yet.
    pub async fn pull(&self) -> PullAnswer {
        match self.try_pull().await {
            Ok(None) => PullAnswer::Absent,
            Ok(Some(text)) => PullAnswer::File(text),
            Err(failure) => {
                log::warn!("sync: pull failed: {failure:?}");
                PullAnswer::Failed(failure)
            }
        }
    }

    /// `sync_push`: `text` written as the progress file.
    pub async fn push(&self, text: &str) -> PushAnswer {
        match self.try_push(text).await {
            Ok(()) => PushAnswer::Pushed,
            Err(failure) => {
                log::warn!("sync: push failed: {failure:?}");
                PushAnswer::Failed(failure)
            }
        }
    }

    async fn try_pull(&self) -> Result<Option<String>, SyncFailure> {
        let list = self.0.endpoints.drive(&drive_location::list_url());
        let (status, body) = self
            .authorized(|token| self.0.http.get(&list).bearer_auth(token))
            .await?;
        checked(status, &body)?;
        let id = google::first_file_id(&body)?;
        self.session().file_id.clone_from(&id);
        let Some(id) = id else {
            log::info!("sync: no Drive file yet");
            return Ok(None);
        };
        log::info!("sync: Drive file id {id}");
        let read = self.0.endpoints.drive(&drive_location::read_url(&id));
        let (status, body) = self
            .authorized(|token| self.0.http.get(&read).bearer_auth(token))
            .await?;
        checked(status, &body)?;
        Ok(Some(body))
    }

    /// Updates the file the last pull found, or creates it.
    async fn try_push(&self, text: &str) -> Result<(), SyncFailure> {
        let known = self.session().file_id.clone();
        if let Some(id) = known {
            let update = self.0.endpoints.drive(&drive_location::update_url(&id));
            let (status, answer) = self
                .authorized(|token| {
                    let request = self.0.http.patch(&update).bearer_auth(token);
                    request
                        .header(CONTENT_TYPE, "application/json")
                        .body(text.to_owned())
                })
                .await?;
            checked(status, &answer)?;
            log::info!("sync: Drive file id {id} (updated)");
            return Ok(());
        }
        let (content_type, body) = drive_location::create_body(text);
        let create = self.0.endpoints.drive(drive_location::CREATE_URL);
        let (status, answer) = self
            .authorized(|token| {
                let request = self.0.http.post(&create).bearer_auth(token);
                request
                    .header(CONTENT_TYPE, &content_type)
                    .body(body.clone())
            })
            .await?;
        checked(status, &answer)?;
        let id = google::created_id(&answer)?;
        log::info!("sync: Drive file id {id} (created)");
        self.session().file_id = Some(id);
        Ok(())
    }

    /// `sync_sign_out`: the sign-in waiting cancelled, the refresh token
    /// revoked at Google (best effort), then deleted; the memory cleared.
    pub async fn sign_out(&self) {
        self.cancel();
        let kept = self.0.secrets.get().ok().flatten();
        let token = kept.or_else(|| self.session().access.as_ref().map(|a| a.token.clone()));
        if let Some(token) = token {
            let body = google::form(&[("token", &token)]);
            match send(self.form_post(&self.0.endpoints.revoke, body)).await {
                Ok((200..=299, _)) => log::info!("sync: revoked at Google"),
                Ok((status, _)) => log::warn!("sync: Google answered {status} to the revoke"),
                Err(failure) => log::warn!("sync: the revoke failed: {failure:?}"),
            }
        }
        self.forget();
    }
}
