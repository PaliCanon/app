//! Google's endpoints and answers: the OAuth token endpoint (the code
//! exchange, the refresh), revoke, userinfo (the account's email), and the
//! reading of Drive's answers. The Drive's addresses are `drive_location`'s
//! (learn-pali 050), the same as the site's; [`Endpoints::drive`] only
//! moves their host for the tests' fake Google.

use std::time::Duration;

use palicanon_app_bridge::SyncFailure;

/// Where Google is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// The sign-in page (the authorization endpoint).
    pub auth: String,
    /// The token endpoint: the code exchange and the refresh.
    pub token: String,
    /// The revoke endpoint.
    pub revoke: String,
    /// The account's email.
    pub userinfo: String,
    /// The host the Drive's addresses are sent to.
    pub googleapis: String,
}

/// The Drive's (and userinfo's) host in `drive_location`'s addresses.
const GOOGLEAPIS: &str = "https://www.googleapis.com";

impl Endpoints {
    /// Google's.
    pub fn google() -> Self {
        Endpoints {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            revoke: "https://oauth2.googleapis.com/revoke".into(),
            userinfo: format!("{GOOGLEAPIS}/oauth2/v3/userinfo"),
            googleapis: GOOGLEAPIS.into(),
        }
    }

    /// Every endpoint under `base` (a fake Google): `/auth`, `/token`,
    /// `/revoke`, `/oauth2/v3/userinfo` and the Drive's paths.
    pub fn at(base: &str) -> Self {
        Endpoints {
            auth: format!("{base}/auth"),
            token: format!("{base}/token"),
            revoke: format!("{base}/revoke"),
            userinfo: format!("{base}/oauth2/v3/userinfo"),
            googleapis: base.into(),
        }
    }

    /// A `drive_location` address, sent to [`Self::googleapis`].
    pub fn drive(&self, address: &str) -> String {
        match address.strip_prefix(GOOGLEAPIS) {
            Some(path) => format!("{}{path}", self.googleapis),
            None => address.to_string(),
        }
    }
}

/// The desktop OAuth client: its id and its (non-confidential, RFC 8252
/// §8.5) secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthClient {
    /// The client id.
    pub id: String,
    /// The client secret.
    pub secret: String,
}

/// A token endpoint's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokens {
    /// The access token.
    pub access: String,
    /// Its life, in seconds.
    pub expires_in: u64,
    /// A refresh token, when Google gave one (the code exchange).
    pub refresh: Option<String>,
}

/// An access token's life when Google's answer does not say: its usual
/// hour, less a second (as the site).
const DEFAULT_EXPIRES_IN: u64 = 3599;

/// The tokens of a token endpoint's answer `(status, body)`.
///
/// # Errors
/// `invalid_grant` (a refresh token expired or revoked), or a 401 with no
/// error code, is [`SyncFailure::SignedOut`]; another error is Google's
/// code (a refused client is never taken for a refused token).
pub fn tokens_of(status: u16, body: &str) -> Result<Tokens, SyncFailure> {
    let value: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    if (200..300).contains(&status) {
        let access = value["access_token"].as_str().unwrap_or_default();
        if access.is_empty() {
            return Err(SyncFailure::Drive("no access token".into()));
        }
        return Ok(Tokens {
            access: access.to_string(),
            expires_in: value["expires_in"].as_u64().unwrap_or(DEFAULT_EXPIRES_IN),
            refresh: value["refresh_token"].as_str().map(str::to_owned),
        });
    }
    match value["error"].as_str() {
        Some("invalid_grant") => Err(SyncFailure::SignedOut),
        Some(error) => Err(SyncFailure::Drive(format!("{status} {error}"))),
        None if status == 401 => Err(SyncFailure::SignedOut),
        None => Err(SyncFailure::Drive(status.to_string())),
    }
}

/// A Drive answer: `Ok` for a 2xx, [`SyncFailure::SignedOut`] for a 401
/// (the token is refused), else Google's message (as the site's).
///
/// # Errors
/// As above.
pub fn checked(status: u16, body: &str) -> Result<(), SyncFailure> {
    match status {
        200..=299 => Ok(()),
        401 => Err(SyncFailure::SignedOut),
        _ => {
            let message = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|value| value["error"]["message"].as_str().map(str::to_owned))
                .unwrap_or_default();
            Err(SyncFailure::Drive(
                format!("{status} {message}").trim().to_owned(),
            ))
        }
    }
}

/// The first file id of a list answer (an entry without one is skipped);
/// none when the folder has no file.
///
/// # Errors
/// [`SyncFailure::Drive`] when the answer is not a list.
pub fn first_file_id(body: &str) -> Result<Option<String>, SyncFailure> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| SyncFailure::Drive("unexpected list answer".into()))?;
    let files = value["files"]
        .as_array()
        .ok_or_else(|| SyncFailure::Drive("unexpected list answer".into()))?;
    Ok(files
        .iter()
        .find_map(|file| file["id"].as_str())
        .map(str::to_owned))
}

/// The id of a created file.
///
/// # Errors
/// [`SyncFailure::Drive`] when the answer holds none.
pub fn created_id(body: &str) -> Result<String, SyncFailure> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value["id"].as_str().map(str::to_owned))
        .ok_or_else(|| SyncFailure::Drive("unexpected creation answer".into()))
}

/// The email of a userinfo answer.
pub fn userinfo_email(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value["email"].as_str().map(str::to_owned))
}

/// The HTTP client of every Google call: rustls with ring (installed once
/// as the process' provider), the system's roots, 30 s per call.
///
/// # Panics
/// When the TLS stack cannot be built (never with ring).
pub fn http_client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("the HTTP client")
}

/// A call's status and body; a failure to reach Google is
/// [`SyncFailure::Network`].
///
/// # Errors
/// As above.
pub async fn send(request: reqwest::RequestBuilder) -> Result<(u16, String), SyncFailure> {
    let response = request.send().await.map_err(|error| {
        log::warn!("sync: Google unreachable: {error}");
        SyncFailure::Network
    })?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|_| SyncFailure::Network)?;
    Ok((status, body))
}

/// A form body (`application/x-www-form-urlencoded`).
pub fn form(pairs: &[(&str, &str)]) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in pairs {
        form.append_pair(name, value);
    }
    form.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drive_addresses_move_to_a_fake_host_and_googles_stay() {
        let address = learn_pali::drive_location::list_url();
        assert_eq!(Endpoints::google().drive(&address), address);
        let moved = Endpoints::at("http://127.0.0.1:9").drive(&address);
        assert!(
            moved.starts_with("http://127.0.0.1:9/drive/v3/files?"),
            "{moved}"
        );
    }

    #[test]
    fn googles_endpoints_are_the_installed_app_ones() {
        let google = Endpoints::google();
        assert_eq!(google.token, "https://oauth2.googleapis.com/token");
        assert_eq!(google.revoke, "https://oauth2.googleapis.com/revoke");
        assert_eq!(google.auth, "https://accounts.google.com/o/oauth2/v2/auth");
        assert_eq!(
            google.userinfo,
            "https://www.googleapis.com/oauth2/v3/userinfo"
        );
    }

    #[test]
    fn a_token_answer_gives_its_tokens_or_the_reason() {
        assert_eq!(
            tokens_of(
                200,
                r#"{"access_token":"a","refresh_token":"r","expires_in":10}"#
            ),
            Ok(Tokens {
                access: "a".into(),
                expires_in: 10,
                refresh: Some("r".into()),
            })
        );
        assert_eq!(
            tokens_of(200, r#"{"access_token":"a"}"#).map(|t| t.expires_in),
            Ok(3599)
        );
        assert_eq!(
            tokens_of(400, r#"{"error":"invalid_grant"}"#),
            Err(SyncFailure::SignedOut)
        );
        assert_eq!(tokens_of(401, "{}"), Err(SyncFailure::SignedOut));
        assert_eq!(
            tokens_of(401, r#"{"error":"invalid_client"}"#),
            Err(SyncFailure::Drive("401 invalid_client".into()))
        );
        assert!(matches!(tokens_of(200, "{}"), Err(SyncFailure::Drive(_))));
    }

    #[test]
    fn drive_answers_are_read_as_the_site_reads_them() {
        assert_eq!(checked(204, ""), Ok(()));
        assert_eq!(checked(401, ""), Err(SyncFailure::SignedOut));
        assert_eq!(
            checked(403, r#"{"error":{"message":"quota"}}"#),
            Err(SyncFailure::Drive("403 quota".into()))
        );
        assert_eq!(checked(500, "x"), Err(SyncFailure::Drive("500".into())));
        assert_eq!(
            first_file_id(r#"{"files":[{"name":"x"},{"id":"f2"}]}"#),
            Ok(Some("f2".into()))
        );
        assert_eq!(first_file_id(r#"{"files":[]}"#), Ok(None));
        assert!(first_file_id("{}").is_err());
        assert_eq!(created_id(r#"{"id":"f1"}"#), Ok("f1".into()));
        assert!(created_id("{}").is_err());
        assert_eq!(userinfo_email(r#"{"email":"a@b"}"#), Some("a@b".into()));
        assert_eq!(userinfo_email("{}"), None);
    }
}
