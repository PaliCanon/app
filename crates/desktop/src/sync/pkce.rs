//! The installed-app flow's proofs (RFC 7636, RFC 8252): a PKCE verifier
//! and its S256 challenge, the state that ties Google's answer to this
//! sign-in, and Google's sign-in address built from them.
//!
//! PKCE makes a code intercepted on the loopback useless: only the holder
//! of the verifier (this process, in memory) can exchange it.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// `bytes` bytes of the system's randomness, base64url without padding
/// (only `A-Z a-z 0-9 - _`). 32 bytes give 43 characters.
///
/// # Panics
/// When the system has no randomness to give (never on macOS).
pub fn random_token(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).expect("the system's randomness");
    URL_SAFE_NO_PAD.encode(buffer)
}

/// The S256 challenge of `verifier`: base64url(SHA-256(verifier)), no
/// padding.
pub fn challenge_of(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// One sign-in's secrets: the PKCE verifier (sent only with the code
/// exchange) and its challenge (sent in the sign-in address), and the state.
#[derive(Debug, Clone)]
pub struct Proofs {
    /// The PKCE verifier: 43 characters of randomness.
    pub verifier: String,
    /// Its S256 challenge.
    pub challenge: String,
    /// The state: Google's answer must carry it back.
    pub state: String,
}

impl Proofs {
    /// Fresh proofs for one sign-in.
    pub fn new() -> Self {
        let verifier = random_token(32);
        Proofs {
            challenge: challenge_of(&verifier),
            verifier,
            state: random_token(16),
        }
    }
}

impl Default for Proofs {
    fn default() -> Self {
        Self::new()
    }
}

/// What a sign-in address carries besides the proofs.
#[derive(Debug, Clone, Copy)]
pub struct AuthRequest<'a> {
    /// Google's authorization endpoint.
    pub endpoint: &'a str,
    /// The OAuth client's id.
    pub client_id: &'a str,
    /// The loopback address Google redirects to.
    pub redirect_uri: &'a str,
    /// The scopes, space separated.
    pub scope: &'a str,
}

/// Google's sign-in address: the code flow with PKCE (S256) and the state,
/// offline access (a refresh token) and the consent screen (so a refresh
/// token is given again after a sign-out).
pub fn auth_url(request: AuthRequest<'_>, proofs: &Proofs) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", request.client_id)
        .append_pair("redirect_uri", request.redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", request.scope)
        .append_pair("code_challenge", &proofs.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &proofs.state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .finish();
    format!("{}?{query}", request.endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7636, appendix B.
    #[test]
    fn the_challenge_is_the_rfcs_example() {
        assert_eq!(
            challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_verifier_is_43_unreserved_characters_and_never_twice_the_same() {
        let (a, b) = (Proofs::new(), Proofs::new());
        assert_eq!(a.verifier.len(), 43);
        assert!(
            a.verifier
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "{}",
            a.verifier
        );
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(a.state, b.state);
        assert_eq!(a.challenge, challenge_of(&a.verifier));
        assert!(a.state.len() >= 22, "{}", a.state);
    }

    #[test]
    fn the_address_asks_the_code_with_the_challenge_and_the_state() {
        let proofs = Proofs::new();
        let url = auth_url(
            AuthRequest {
                endpoint: "https://accounts.google.com/o/oauth2/v2/auth",
                client_id: "id",
                redirect_uri: "http://127.0.0.1:5000",
                scope: "s1 email",
            },
            &proofs,
        );
        let parsed = url::Url::parse(&url).unwrap();
        assert_eq!(parsed.host_str(), Some("accounts.google.com"));
        let pairs: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(pairs["client_id"], "id");
        assert_eq!(pairs["redirect_uri"], "http://127.0.0.1:5000");
        assert_eq!(pairs["response_type"], "code");
        assert_eq!(pairs["scope"], "s1 email");
        assert_eq!(pairs["code_challenge"], proofs.challenge);
        assert_eq!(pairs["code_challenge_method"], "S256");
        assert_eq!(pairs["state"], proofs.state);
        assert_eq!(pairs["access_type"], "offline");
        assert!(!url.contains(&proofs.verifier), "the verifier never leaves");
    }
}
