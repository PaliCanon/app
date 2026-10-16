//! The sync through the learner's Google Drive (task 006; learn-pali 050's
//! design, its `app` node): the desktop transport, all in Rust.
//!
//! - The installed-app flow (RFC 8252): PKCE S256 and a state
//!   ([`pkce`]), a `127.0.0.1` listener on a port the system picks, for
//!   one request ([`loopback`]), Google's sign-in address shown in the app
//!   and opened once in the system browser.
//! - The Desktop OAuth client [`CLIENT_ID`] and its secret, injected at
//!   BUILD time ([`CLIENT_SECRET`], from the build machine's Keychain by
//!   `build.rs`, or a CI secret): Google calls it non-confidential for an
//!   installed app (PKCE is what protects the code). It is never in the
//!   repository, never printed, never logged. A build without it has no
//!   sync: `sync_account` answers `Unavailable` and the front shows no sync
//!   UI.
//! - The refresh token in the macOS Keychain ([`keychain`]), the access
//!   token in memory, the Drive calls over rustls ([`service`], [`google`]).

pub mod google;
pub mod keychain;
pub mod loopback;
pub mod pkce;
pub mod service;

use std::sync::Arc;

pub use google::{Endpoints, OAuthClient};
pub use keychain::{Keychain, MemorySecrets, Secrets};
pub use service::{PendingSignIn, SyncService};

/// The Desktop OAuth client of project `palicanon-app`.
pub const CLIENT_ID: &str =
    "873594463453-v23gmvb9ksptrunuti0j4871p46i7q84.apps.googleusercontent.com";

/// The environment variable the client's secret is compiled from.
pub const SECRET_VARIABLE: &str = "PALICANON_GOOGLE_DESKTOP_SECRET";

/// The Desktop client's secret, compiled in when the build had it; none:
/// no sync in this build.
pub const CLIENT_SECRET: Option<&str> = option_env!("PALICANON_GOOGLE_DESKTOP_SECRET");

/// The Desktop client with `secret`; none (no sync) without one, or with
/// an empty one.
pub fn client_with(secret: Option<&str>) -> Option<OAuthClient> {
    secret
        .filter(|secret| !secret.is_empty())
        .map(|secret| OAuthClient {
            id: CLIENT_ID.to_string(),
            secret: secret.to_string(),
        })
}

/// The sync this build has: Google's endpoints and the macOS Keychain, or
/// none when it was built without the secret.
pub fn from_build() -> Option<SyncService> {
    client_with(CLIENT_SECRET)
        .map(|client| SyncService::new(client, Endpoints::google(), Arc::new(Keychain)))
}

/// Opens an address in the system browser; the reason when it could not.
pub type Opener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// The sync the commands reach (Tauri's managed state): the service, or
/// none (no sync in this build: every command answers so), and how the
/// sign-in address is opened.
#[derive(Clone)]
pub struct SyncHost {
    /// The service; none without sync.
    pub service: Option<SyncService>,
    /// Opens the sign-in address in the system browser.
    pub open: Opener,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_secret_there_is_no_sync() {
        assert_eq!(client_with(None), None);
        assert_eq!(client_with(Some("")), None);
        assert_eq!(
            client_with(Some("s")),
            Some(OAuthClient {
                id: CLIENT_ID.into(),
                secret: "s".into(),
            })
        );
    }

    #[test]
    fn the_client_is_the_desktop_one() {
        assert!(CLIENT_ID.starts_with("873594463453-v23g"), "{CLIENT_ID}");
        assert_eq!(SECRET_VARIABLE, "PALICANON_GOOGLE_DESKTOP_SECRET");
    }
}
