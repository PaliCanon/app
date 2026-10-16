//! The desktop sync against a fake Google (`support/fake_google.rs`): the
//! sign-in's code exchange (PKCE verifier, client, redirect), the refresh
//! token kept through the `Secrets` trait (a fake Keychain), the access
//! token refreshed and a 401 refreshed once, the Drive's app folder listed,
//! read, created and updated exactly where `drive_location` says, and
//! sign-out revoking then forgetting.

#[path = "support/fake_google.rs"]
mod fake_google;

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use palicanon_app_bridge::{PullAnswer, PushAnswer, SignInEvent, SyncAccountAnswer, SyncFailure};
use palicanon_app_desktop::sync::{Endpoints, MemorySecrets, OAuthClient, Secrets, SyncService};
use sha2::{Digest, Sha256};

use fake_google::FakeGoogle;

const LIST: &str = "/drive/v3/files";
const TOKEN: &str = "/token";

fn client() -> OAuthClient {
    OAuthClient {
        id: "desktop-id.apps.googleusercontent.com".into(),
        secret: "desktop-secret".into(),
    }
}

/// A service over the fake, its Keychain holding `refresh`.
fn service(google: &FakeGoogle, refresh: Option<&str>) -> (SyncService, Arc<MemorySecrets>) {
    let secrets = Arc::new(MemorySecrets::default());
    if let Some(refresh) = refresh {
        secrets.set(refresh).unwrap();
    }
    let service = SyncService::new(client(), Endpoints::at(&google.base), secrets.clone());
    (service, secrets)
}

fn token(access: &str) -> String {
    format!(r#"{{"access_token":"{access}","expires_in":3599,"token_type":"Bearer"}}"#)
}

/// The query of `url`, decoded.
fn query(url: &str) -> std::collections::HashMap<String, String> {
    url::Url::parse(url)
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// A form body, decoded.
fn form(body: &str) -> std::collections::HashMap<String, String> {
    url::form_urlencoded::parse(body.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

#[tokio::test]
async fn the_account_is_signed_out_without_a_refresh_token_and_signed_in_with_one() {
    let google = FakeGoogle::start().await;
    let (signed_out, _) = service(&google, None);
    assert_eq!(signed_out.account(), SyncAccountAnswer::SignedOut);
    let (signed_in, _) = service(&google, Some("rt1"));
    assert_eq!(
        signed_in.account(),
        SyncAccountAnswer::SignedIn { email: None }
    );
    assert_eq!(google.seen().len(), 0, "the account asks no network");
}

/// The sign-in address asks Google's code flow of the Desktop client,
/// offline, with an S256 challenge; its query.
fn the_address_asks_the_code(url: &str, base: &str) -> std::collections::HashMap<String, String> {
    let asked = query(url);
    assert!(url.starts_with(&format!("{base}/auth?")), "{url}");
    assert_eq!(asked["client_id"], "desktop-id.apps.googleusercontent.com");
    assert_eq!(asked["response_type"], "code");
    assert_eq!(asked["code_challenge_method"], "S256");
    assert_eq!(asked["access_type"], "offline");
    assert_eq!(
        asked["scope"],
        "https://www.googleapis.com/auth/drive.appdata email"
    );
    asked
}

#[tokio::test]
async fn a_sign_in_exchanges_the_code_with_its_verifier_and_keeps_the_refresh_token() {
    let google = FakeGoogle::start().await;
    google.answer(
        "POST",
        TOKEN,
        200,
        r#"{"access_token":"at1","expires_in":3599,"refresh_token":"rt1"}"#,
    );
    google.answer(
        "GET",
        "/oauth2/v3/userinfo",
        200,
        r#"{"email":"a@b.example"}"#,
    );
    let (service, secrets) = service(&google, None);
    let pending = service.start_sign_in().await.expect("a sign-in starts");
    let asked = the_address_asks_the_code(&pending.url, &google.base);
    let redirect = asked["redirect_uri"].clone();
    assert!(redirect.starts_with("http://127.0.0.1:"), "{redirect}");
    let finish = tokio::spawn(pending.finish);
    let answer = get(&format!("{redirect}/?code=c0de&state={}", asked["state"])).await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    let event = finish.await.unwrap();
    assert_eq!(
        event,
        SignInEvent {
            attempt: pending.attempt,
            outcome: Ok(Some("a@b.example".into())),
        }
    );
    let exchange = &google.seen_at("POST", TOKEN)[0];
    let sent = form(&exchange.body);
    assert_eq!(sent["grant_type"], "authorization_code");
    assert_eq!(sent["code"], "c0de");
    assert_eq!(sent["redirect_uri"], redirect);
    assert_eq!(sent["client_id"], "desktop-id.apps.googleusercontent.com");
    assert_eq!(sent["client_secret"], "desktop-secret");
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(sent["code_verifier"].as_bytes()));
    assert_eq!(challenge, asked["code_challenge"]);
    assert_eq!(secrets.get().unwrap(), Some("rt1".into()));
    assert_eq!(
        service.account(),
        SyncAccountAnswer::SignedIn {
            email: Some("a@b.example".into())
        }
    );
}

#[tokio::test]
async fn a_sign_in_answer_with_another_state_is_refused_and_nothing_is_exchanged() {
    let google = FakeGoogle::start().await;
    let (service, secrets) = service(&google, None);
    let pending = service.start_sign_in().await.expect("a sign-in starts");
    let redirect = query(&pending.url)["redirect_uri"].clone();
    let finish = tokio::spawn(pending.finish);
    let answer = get(&format!("{redirect}/?code=c0de&state=forged")).await;
    assert!(answer.starts_with("HTTP/1.1 400"), "{answer}");
    let event = finish.await.unwrap();
    assert!(event.outcome.is_err(), "{event:?}");
    assert_eq!(google.seen_at("POST", TOKEN).len(), 0);
    assert_eq!(secrets.get().unwrap(), None);
    let address = redirect.trim_start_matches("http://");
    assert!(
        tokio::net::TcpStream::connect(address).await.is_err(),
        "one request, then the listener is closed"
    );
}

#[tokio::test]
async fn a_connection_that_sends_nothing_is_not_the_answer() {
    let google = FakeGoogle::start().await;
    google.answer(
        "POST",
        TOKEN,
        200,
        r#"{"access_token":"at1","expires_in":3599,"refresh_token":"rt1"}"#,
    );
    let (service, _) = service(&google, None);
    let pending = service.start_sign_in().await.expect("a sign-in starts");
    let asked = query(&pending.url);
    let redirect = asked["redirect_uri"].clone();
    let finish = tokio::spawn(pending.finish);
    drop(
        tokio::net::TcpStream::connect(redirect.trim_start_matches("http://"))
            .await
            .unwrap(),
    );
    let answer = get(&format!("{redirect}/?code=c0de&state={}", asked["state"])).await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert_eq!(finish.await.unwrap().outcome, Ok(None));
}

#[tokio::test]
async fn a_cancelled_sign_in_ends_cancelled_and_closes_its_listener() {
    let google = FakeGoogle::start().await;
    let (service, _) = service(&google, None);
    let pending = service.start_sign_in().await.expect("a sign-in starts");
    let redirect = query(&pending.url)["redirect_uri"].clone();
    let finish = tokio::spawn(pending.finish);
    service.cancel();
    let event = tokio::time::timeout(Duration::from_secs(5), finish)
        .await
        .expect("the cancel ends the wait")
        .unwrap();
    assert_eq!(event.outcome, Err(SyncFailure::Cancelled));
    let address = redirect.trim_start_matches("http://");
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
}

#[tokio::test]
async fn a_new_sign_in_cancels_the_one_waiting() {
    let google = FakeGoogle::start().await;
    let (service, _) = service(&google, None);
    let first = service.start_sign_in().await.expect("a sign-in starts");
    let first_finish = tokio::spawn(first.finish);
    let second = service.start_sign_in().await.expect("a second starts");
    assert!(second.attempt > first.attempt);
    let event = tokio::time::timeout(Duration::from_secs(5), first_finish)
        .await
        .expect("the first ends")
        .unwrap();
    assert_eq!(event.outcome, Err(SyncFailure::Cancelled));
}

#[tokio::test]
async fn a_sign_in_nobody_answers_times_out_cancelled() {
    let google = FakeGoogle::start().await;
    let (service, _) = service(&google, None);
    let service = service.with_sign_in_timeout(Duration::from_millis(200));
    let pending = service.start_sign_in().await.expect("a sign-in starts");
    let event = tokio::time::timeout(Duration::from_secs(5), pending.finish)
        .await
        .expect("the timeout ends the wait");
    assert_eq!(event.outcome, Err(SyncFailure::Cancelled));
}

#[tokio::test]
async fn a_pull_with_no_file_answers_absent_then_the_push_creates_it() {
    let google = FakeGoogle::start().await;
    google.answer("POST", TOKEN, 200, &token("at1"));
    google.answer("GET", LIST, 200, r#"{"files":[]}"#);
    google.answer("POST", "/upload/drive/v3/files", 200, r#"{"id":"f1"}"#);
    let (service, _) = service(&google, Some("rt1"));
    assert_eq!(service.pull().await, PullAnswer::Absent);
    assert_eq!(service.push(r#"{"version":1}"#).await, PushAnswer::Pushed);
    let list = &google.seen_at("GET", LIST)[0];
    assert_eq!(list.bearer(), Some("at1"));
    assert!(
        list.target.contains("spaces=appDataFolder"),
        "{}",
        list.target
    );
    let create = &google.seen_at("POST", "/upload/drive/v3/files")[0];
    assert!(
        create.target.contains("uploadType=multipart"),
        "{}",
        create.target
    );
    assert!(
        create.body.contains(r#""parents":["appDataFolder"]"#),
        "{}",
        create.body
    );
    assert!(create.body.contains(r#"{"version":1}"#), "{}", create.body);
    assert!(
        create.headers["content-type"].starts_with("multipart/related; boundary="),
        "{:?}",
        create.headers
    );
    let refresh = form(&google.seen_at("POST", TOKEN)[0].body);
    assert_eq!(refresh["grant_type"], "refresh_token");
    assert_eq!(refresh["refresh_token"], "rt1");
    assert_eq!(refresh["client_secret"], "desktop-secret");
}

#[tokio::test]
async fn a_pull_reads_the_file_and_the_push_updates_it() {
    let google = FakeGoogle::start().await;
    google.answer("POST", TOKEN, 200, &token("at1"));
    google.answer("GET", LIST, 200, r#"{"files":[{"id":"f1"},{"id":"f2"}]}"#);
    google.answer(
        "GET",
        "/drive/v3/files/f1",
        200,
        r#"{"version":1,"terms":[]}"#,
    );
    google.answer("PATCH", "/upload/drive/v3/files/f1", 200, r#"{"id":"f1"}"#);
    let (service, _) = service(&google, Some("rt1"));
    assert_eq!(
        service.pull().await,
        PullAnswer::File(r#"{"version":1,"terms":[]}"#.into())
    );
    assert_eq!(service.push("{}").await, PushAnswer::Pushed);
    let read = &google.seen_at("GET", "/drive/v3/files/f1")[0];
    assert!(read.target.ends_with("?alt=media"), "{}", read.target);
    let update = &google.seen_at("PATCH", "/upload/drive/v3/files/f1")[0];
    assert!(
        update.target.ends_with("?uploadType=media"),
        "{}",
        update.target
    );
    assert_eq!(update.body, "{}");
    assert_eq!(update.bearer(), Some("at1"));
    assert_eq!(
        google.seen_at("POST", TOKEN).len(),
        1,
        "the access token is reused"
    );
    assert_eq!(google.seen_at("POST", "/upload/drive/v3/files").len(), 0);
}

#[tokio::test]
async fn a_401_refreshes_the_token_once_and_retries() {
    let google = FakeGoogle::start().await;
    google.answer("POST", TOKEN, 200, &token("at1"));
    google.answer("POST", TOKEN, 200, &token("at2"));
    google.answer("GET", LIST, 401, r#"{"error":{"message":"expired"}}"#);
    google.answer("GET", LIST, 200, r#"{"files":[]}"#);
    let (service, _) = service(&google, Some("rt1"));
    assert_eq!(service.pull().await, PullAnswer::Absent);
    let lists = google.seen_at("GET", LIST);
    assert_eq!(lists.len(), 2);
    assert_eq!(lists[0].bearer(), Some("at1"));
    assert_eq!(lists[1].bearer(), Some("at2"));
    assert_eq!(google.seen_at("POST", TOKEN).len(), 2);
}

#[tokio::test]
async fn a_second_401_is_signed_out_after_one_refresh_only() {
    let google = FakeGoogle::start().await;
    google.answer("POST", TOKEN, 200, &token("at1"));
    google.answer("GET", LIST, 401, r#"{"error":{"message":"expired"}}"#);
    let (service, _) = service(&google, Some("rt1"));
    assert_eq!(
        service.pull().await,
        PullAnswer::Failed(SyncFailure::SignedOut)
    );
    assert_eq!(google.seen_at("GET", LIST).len(), 2);
    assert_eq!(google.seen_at("POST", TOKEN).len(), 2);
}

#[tokio::test]
async fn a_refused_refresh_token_is_forgotten_and_signed_out() {
    let google = FakeGoogle::start().await;
    google.answer("POST", TOKEN, 400, r#"{"error":"invalid_grant"}"#);
    let (service, secrets) = service(&google, Some("rt1"));
    assert_eq!(
        service.pull().await,
        PullAnswer::Failed(SyncFailure::SignedOut)
    );
    assert_eq!(secrets.get().unwrap(), None);
    assert_eq!(service.account(), SyncAccountAnswer::SignedOut);
    assert_eq!(google.seen_at("GET", LIST).len(), 0);
}

#[tokio::test]
async fn with_no_refresh_token_a_pull_is_signed_out_with_no_request() {
    let google = FakeGoogle::start().await;
    let (service, _) = service(&google, None);
    assert_eq!(
        service.pull().await,
        PullAnswer::Failed(SyncFailure::SignedOut)
    );
    assert_eq!(
        service.push("{}").await,
        PushAnswer::Failed(SyncFailure::SignedOut)
    );
    assert_eq!(google.seen().len(), 0);
}

#[tokio::test]
async fn a_drive_error_is_its_message_and_an_unreachable_google_is_network() {
    let google = FakeGoogle::start().await;
    google.answer("POST", TOKEN, 200, &token("at1"));
    google.answer("GET", LIST, 403, r#"{"error":{"message":"quota"}}"#);
    let (service, _) = service(&google, Some("rt1"));
    assert_eq!(
        service.pull().await,
        PullAnswer::Failed(SyncFailure::Drive("403 quota".into()))
    );
    let secrets = Arc::new(MemorySecrets::default());
    secrets.set("rt1").unwrap();
    let offline = SyncService::new(client(), Endpoints::at("http://127.0.0.1:9"), secrets);
    assert_eq!(
        offline.pull().await,
        PullAnswer::Failed(SyncFailure::Network)
    );
}

#[tokio::test]
async fn sign_out_revokes_the_refresh_token_then_forgets_it() {
    let google = FakeGoogle::start().await;
    google.answer("POST", "/revoke", 200, "{}");
    let (service, secrets) = service(&google, Some("rt1"));
    service.sign_out().await;
    let revoke = &google.seen_at("POST", "/revoke")[0];
    assert_eq!(form(&revoke.body)["token"], "rt1");
    assert_eq!(secrets.get().unwrap(), None);
    assert_eq!(service.account(), SyncAccountAnswer::SignedOut);
}

#[tokio::test]
async fn a_failed_revoke_still_forgets_the_token() {
    let google = FakeGoogle::start().await;
    google.answer("POST", "/revoke", 500, "{}");
    let (service, secrets) = service(&google, Some("rt1"));
    service.sign_out().await;
    assert_eq!(google.seen_at("POST", "/revoke").len(), 1);
    assert_eq!(secrets.get().unwrap(), None);
}

/// One plain HTTP GET, its whole answer as text.
async fn get(url: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let url = url::Url::parse(url).unwrap();
    let address = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap());
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let target = format!("{}?{}", url.path(), url.query().unwrap_or_default());
    let request = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await.unwrap();
    answer
}
