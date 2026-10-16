//! The loopback listener of the installed-app flow (RFC 8252 §7.3): a
//! socket on `127.0.0.1` at a port the system picks, waiting for Google's
//! redirect. It answers ONE request (the first connection that sends one;
//! a connection that sends nothing, a browser's preconnect, is skipped),
//! then is closed. The request's `state` must be this sign-in's, else the
//! answer is refused and nothing is exchanged. It also ends when cancelled
//! or after its timeout.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

/// How long the learner has to sign in.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_mins(5);

/// How long one connection has to send its request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// The most a request's head may hold.
const MAX_HEAD: usize = 16 * 1024;

/// Why the wait gave no code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitError {
    /// Cancelled (by the learner, or by a newer sign-in).
    Cancelled,
    /// Nobody answered in time.
    TimedOut,
    /// The answer's state is not this sign-in's: refused.
    WrongState,
    /// Google answered an error (the learner refused): its code.
    Refused(String),
    /// The answer had no code.
    NoCode,
}

/// What Google's redirect carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Redirect {
    /// The authorization code.
    pub code: Option<String>,
    /// The state sent with the sign-in address.
    pub state: Option<String>,
    /// Google's error (`access_denied`).
    pub error: Option<String>,
}

/// The redirect's parameters from a request's first line
/// (`GET /?code=…&state=… HTTP/1.1`); none when it is not a request.
pub fn parse_request_line(line: &str) -> Option<Redirect> {
    let mut parts = line.split(' ');
    let (_method, target) = (parts.next()?, parts.next()?);
    let query = target.split_once('?').map_or("", |(_, query)| query);
    let mut redirect = Redirect::default();
    for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match name.as_ref() {
            "code" => &mut redirect.code,
            "state" => &mut redirect.state,
            "error" => &mut redirect.error,
            _ => continue,
        };
        *slot = Some(value.into_owned());
    }
    Some(redirect)
}

/// The code of `redirect`, if its state is `expected`.
///
/// # Errors
/// [`WaitError::WrongState`] first (an answer for another sign-in is never
/// read further), then Google's error, then a missing code.
pub fn decide(redirect: Redirect, expected: &str) -> Result<String, WaitError> {
    if redirect.state.as_deref() != Some(expected) {
        return Err(WaitError::WrongState);
    }
    if let Some(error) = redirect.error {
        return Err(WaitError::Refused(error));
    }
    redirect.code.ok_or(WaitError::NoCode)
}

/// The listener, bound and not yet answered.
#[derive(Debug)]
pub struct Loopback {
    listener: TcpListener,
    port: u16,
}

impl Loopback {
    /// Binds `127.0.0.1:0`.
    ///
    /// # Errors
    /// The system refused the socket.
    pub async fn bind() -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        Ok(Loopback { listener, port })
    }

    /// The address Google redirects to.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Waits for the one request, until `cancel` fires or `timeout` runs
    /// out; answers the browser, then closes (the listener is dropped).
    ///
    /// # Errors
    /// See [`WaitError`].
    pub async fn wait(
        self,
        state: &str,
        timeout: Duration,
        mut cancel: oneshot::Receiver<()>,
    ) -> Result<String, WaitError> {
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut cancel => return Err(WaitError::Cancelled),
                () = &mut deadline => return Err(WaitError::TimedOut),
                accepted = self.listener.accept() => {
                    let Ok((stream, _)) = accepted else { continue };
                    if let Some(outcome) = answer(stream, state).await {
                        return outcome;
                    }
                }
            }
        }
    }
}

/// Reads one connection's request and answers it; none when it sent no
/// request (then the wait goes on).
async fn answer(mut stream: TcpStream, state: &str) -> Option<Result<String, WaitError>> {
    let line = tokio::time::timeout(REQUEST_TIMEOUT, first_line(&mut stream))
        .await
        .ok()??;
    let redirect = parse_request_line(&line)?;
    let outcome = decide(redirect, state);
    let (status, text) = match &outcome {
        Ok(_) => (
            "200 OK",
            "Signed in. You can close this tab and go back to PaliCanon.",
        ),
        Err(_) => (
            "400 Bad Request",
            "PaliCanon was not signed in. Go back to PaliCanon and try again.",
        ),
    };
    let page = format!("<!doctype html><meta charset=utf-8><title>PaliCanon</title><p>{text}</p>");
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
    Some(outcome)
}

/// The request's first line, once its head is in; none when the
/// connection closed first or sent too much.
async fn first_line(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut chunk = [0u8; 2048];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 || head.len() + read > MAX_HEAD {
            return None;
        }
        head.extend_from_slice(&chunk[..read]);
    }
    let head = String::from_utf8_lossy(&head);
    head.lines().next().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_redirect_is_read_from_the_request_line() {
        assert_eq!(
            parse_request_line("GET /?state=s%2B1&code=4%2F0A&scope=x HTTP/1.1"),
            Some(Redirect {
                code: Some("4/0A".into()),
                state: Some("s+1".into()),
                error: None,
            })
        );
        assert_eq!(
            parse_request_line("GET /favicon.ico HTTP/1.1"),
            Some(Redirect::default())
        );
        assert_eq!(parse_request_line(""), None);
    }

    #[test]
    fn only_this_sign_ins_state_gives_the_code() {
        let with = |state: Option<&str>, code: Option<&str>, error: Option<&str>| Redirect {
            code: code.map(Into::into),
            state: state.map(Into::into),
            error: error.map(Into::into),
        };
        assert_eq!(
            decide(with(Some("s"), Some("c"), None), "s"),
            Ok("c".into())
        );
        assert_eq!(
            decide(with(Some("t"), Some("c"), None), "s"),
            Err(WaitError::WrongState)
        );
        assert_eq!(
            decide(with(None, Some("c"), None), "s"),
            Err(WaitError::WrongState)
        );
        assert_eq!(
            decide(with(Some("s"), None, Some("access_denied")), "s"),
            Err(WaitError::Refused("access_denied".into()))
        );
        assert_eq!(
            decide(with(Some("s"), None, None), "s"),
            Err(WaitError::NoCode)
        );
    }
}
