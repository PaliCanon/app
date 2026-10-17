//! The local search engine: the `yuga-sidecar` process, spawned at the first
//! `search` or `search_status`, then KEPT with its model in memory until the
//! app quits (user, 2026-10-16: «Garde le modèle etc. en RAM»).
//!
//! - One request in flight at a time: one mutex over the process, one JSON
//!   line out, one JSON line back (`yuga::sidecar`'s protocol).
//! - Two-phase start (yuga-sidecar 0.5.2): words are served once the
//!   store is open; the model loads in the background and `status` says
//!   `loading: true` meanwhile. [`Engine::status`] asks again every
//!   [`POLL`] (natively, the mutex released between asks) until loading is
//!   over, then answers.
//! - A search the engine refuses because its model is still loading
//!   (`meaning_loading`) is `Cold`; any other refusal is `Failed`.
//! - A crash (the pipe closed) respawns the process once; a second crash
//!   makes every later answer `Failed`.
//! - [`Engine::shutdown`] (on `RunEvent::Exit`) kills the process.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use palicanon_app_bridge::{Hits, SearchAnswer, SearchRequest, SearchStatus};
use serde::Deserialize;

/// How often a loading model's status is asked again.
pub const POLL: Duration = Duration::from_millis(250);
/// How long [`Engine::status`] waits for the model at most.
pub const LOAD_LIMIT: Duration = Duration::from_secs(120);
/// The sidecar's executable name beside the app's (Tauri's `externalBin`
/// drops the target triple in the bundle).
pub const BINARY: &str = "yuga-sidecar";
/// How many hits a search asks for.
pub const K: usize = 20;

/// One running engine: a line out, a line back.
pub trait Process: Send {
    /// Sends `line` and reads the answer line.
    ///
    /// # Errors
    /// The pipe closed (the process died) or any I/O error.
    fn ask(&mut self, line: &str) -> std::io::Result<String>;
    /// Ends the process.
    fn kill(&mut self);
}

/// Starts an engine process.
pub trait Spawn: Send + Sync + 'static {
    /// A new process.
    ///
    /// # Errors
    /// It could not start.
    fn spawn(&self) -> std::io::Result<Box<dyn Process>>;
}

/// The engine, shared by the commands (cheap to clone).
#[derive(Clone)]
pub struct Engine {
    shared: Arc<Shared>,
}

struct Shared {
    spawner: Box<dyn Spawn>,
    poll: Duration,
    state: Mutex<Slot>,
}

#[derive(Default)]
struct Slot {
    process: Option<Box<dyn Process>>,
    respawned: bool,
    failed: Option<String>,
    shut: bool,
}

#[derive(Deserialize)]
struct StatusLine {
    ready: bool,
    meaning: bool,
    #[serde(default)]
    loading: bool,
    #[serde(default)]
    meaning_error: Option<String>,
}

#[derive(Deserialize)]
struct ErrorLine {
    error: String,
    #[serde(default)]
    message: String,
}

impl Engine {
    /// An engine whose process `spawner` starts, the model's status asked
    /// every `poll` while it loads. Nothing is started yet.
    pub fn new(spawner: impl Spawn, poll: Duration) -> Self {
        Engine {
            shared: Arc::new(Shared {
                spawner: Box::new(spawner),
                poll,
                state: Mutex::new(Slot::default()),
            }),
        }
    }

    /// Whether a process is running now.
    pub fn running(&self) -> bool {
        self.slot().process.is_some()
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Slot> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Sends `line` under the mutex, starting the process if none runs; a
    /// crash respawns once.
    fn ask(&self, line: &str) -> Result<String, String> {
        let mut slot = self.slot();
        loop {
            if let Some(why) = &slot.failed {
                return Err(why.clone());
            }
            if slot.shut {
                return Err("the app is quitting".into());
            }
            if slot.process.is_none() {
                match self.shared.spawner.spawn() {
                    Ok(process) => slot.process = Some(process),
                    Err(error) => {
                        let why = format!("the search engine could not start: {error}");
                        log::error!("{why}");
                        slot.failed = Some(why.clone());
                        return Err(why);
                    }
                }
            }
            let process = slot.process.as_mut().expect("started above");
            match process.ask(line) {
                Ok(answer) => return Ok(answer),
                Err(error) => {
                    process.kill();
                    slot.process = None;
                    if slot.respawned {
                        let why = format!("the search engine stopped twice: {error}");
                        log::error!("{why}");
                        slot.failed = Some(why.clone());
                        return Err(why);
                    }
                    log::warn!("the search engine stopped, started again: {error}");
                    slot.respawned = true;
                }
            }
        }
    }

    /// Whether the engine serves, and serves meaning: starts it if needed,
    /// then asks its status again every poll while its model loads (at most
    /// [`LOAD_LIMIT`]), the mutex released between asks.
    ///
    /// # Errors
    /// The engine could not start, or stopped twice.
    pub fn status(&self) -> Result<SearchStatus, String> {
        let since = Instant::now();
        loop {
            let line = self.ask(r#"{"status":true}"#)?;
            let status: StatusLine =
                serde_json::from_str(&line).map_err(|e| format!("a status answer: {e}"))?;
            if !status.loading || since.elapsed() >= LOAD_LIMIT {
                if let Some(why) = status.meaning_error {
                    log::warn!("search: meaning is off: {why}");
                }
                return Ok(SearchStatus {
                    ready: status.ready,
                    meaning: status.meaning,
                });
            }
            std::thread::sleep(self.shared.poll);
        }
    }

    /// One search.
    pub fn search(&self, request: &SearchRequest) -> SearchAnswer {
        let line = serde_json::json!({
            "search": request.query,
            "mode": request.mode,
            "collection": request.collection,
            "k": K,
        })
        .to_string();
        match self.ask(&line) {
            Ok(answer) => answer_of(&answer),
            Err(why) => SearchAnswer::Failed(why),
        }
    }

    /// Kills the process (the app quits); nothing starts after.
    pub fn shutdown(&self) {
        let mut slot = self.slot();
        slot.shut = true;
        if let Some(mut process) = slot.process.take() {
            process.kill();
        }
    }
}

/// The engine's answer line as the bridge's answer.
pub fn answer_of(line: &str) -> SearchAnswer {
    if let Ok(hits) = serde_json::from_str::<Hits>(line) {
        return SearchAnswer::Hits(hits);
    }
    match serde_json::from_str::<ErrorLine>(line) {
        Ok(error) if error.error == "meaning_loading" => SearchAnswer::Cold,
        Ok(error) => SearchAnswer::Failed(format!("{}: {}", error.error, error.message)),
        Err(e) => SearchAnswer::Failed(format!("not an answer: {e}")),
    }
}

/// The real sidecar process.
struct Sidecar {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Process for Sidecar {
    fn ask(&mut self, line: &str) -> std::io::Result<String> {
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        let mut answer = String::new();
        if self.stdout.read_line(&mut answer)? == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "the engine closed its output",
            ));
        }
        Ok(answer)
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Starts `yuga-sidecar` with the bundled store and model.
#[derive(Debug, Clone)]
pub struct SidecarSpawn {
    /// The binary (`Contents/MacOS/yuga-sidecar` in the bundle).
    pub binary: PathBuf,
    /// The store directory (read only).
    pub store: PathBuf,
    /// The model directory (read only).
    pub model: PathBuf,
}

impl Spawn for SidecarSpawn {
    fn spawn(&self) -> std::io::Result<Box<dyn Process>> {
        let mut child = Command::new(&self.binary)
            .arg("--store")
            .arg(&self.store)
            .arg("--model")
            .arg(&self.model)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("no stdout"))?;
        Ok(Box::new(Sidecar {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        }))
    }
}
