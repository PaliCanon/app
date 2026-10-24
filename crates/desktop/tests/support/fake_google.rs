//! A fake Google on 127.0.0.1: the token, revoke, userinfo and Drive
//! endpoints answer what a test queued, by method and exact path, and every
//! request is recorded (method, target, headers, body). HTTP/1.1, one
//! request per connection. An unrouted request answers 404.

// Each test binary that includes it uses a part of it.
#![allow(dead_code, reason = "each test binary uses a part of the fake")]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A request the fake received.
#[derive(Debug, Clone)]
pub struct Seen {
    /// `GET`, `POST`, `PATCH`.
    pub method: String,
    /// The path and the query.
    pub target: String,
    /// The headers, names in lower case.
    pub headers: HashMap<String, String>,
    /// The body, as text.
    pub body: String,
}

impl Seen {
    /// The path, without the query.
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }

    /// The bearer token sent, if any.
    pub fn bearer(&self) -> Option<&str> {
        self.headers
            .get("authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
    }
}

type Answers = HashMap<(String, String), VecDeque<(u16, String)>>;

/// The fake. Cheap to clone: one server.
#[derive(Clone)]
pub struct FakeGoogle {
    /// `http://127.0.0.1:<port>`.
    pub base: String,
    answers: Arc<Mutex<Answers>>,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl FakeGoogle {
    /// Starts it on a random port, on the current tokio runtime.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let fake = FakeGoogle {
            base,
            answers: Arc::default(),
            seen: Arc::default(),
        };
        let server = fake.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let server = server.clone();
                tokio::spawn(async move { server.serve(stream).await });
            }
        });
        fake
    }

    /// Queues an answer to `method path`; the last one queued repeats.
    pub fn answer(&self, method: &str, path: &str, status: u16, body: &str) {
        self.answers
            .lock()
            .unwrap()
            .entry((method.to_string(), path.to_string()))
            .or_default()
            .push_back((status, body.to_string()));
    }

    /// Every request received, in order.
    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    /// The requests received at `method path`.
    pub fn seen_at(&self, method: &str, path: &str) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|seen| seen.method == method && seen.path() == path)
            .collect()
    }

    fn next_answer(&self, method: &str, path: &str) -> (u16, String) {
        let mut answers = self.answers.lock().unwrap();
        let Some(queue) = answers.get_mut(&(method.to_string(), path.to_string())) else {
            return (404, r#"{"error":{"message":"no route"}}"#.to_string());
        };
        if queue.len() > 1 {
            queue.pop_front().unwrap()
        } else {
            queue.front().cloned().unwrap()
        }
    }

    async fn serve(&self, mut stream: TcpStream) {
        let Some(seen) = read_request(&mut stream).await else {
            return;
        };
        let (status, body) = self.next_answer(&seen.method, seen.path());
        self.seen.lock().unwrap().push(seen);
        let answer = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(answer.as_bytes()).await;
        let _ = stream.shutdown().await;
    }
}

/// Reads one request: its head, then `Content-Length` bytes of body.
async fn read_request(stream: &mut TcpStream) -> Option<Seen> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&bytes[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target) = (first.next()?.to_string(), first.next()?.to_string());
    let headers: HashMap<String, String> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_string()))
        .collect();
    let length: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    while bytes.len() < head_end + length {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(&bytes[head_end..]).to_string();
    Some(Seen {
        method,
        target,
        headers,
        body,
    })
}
