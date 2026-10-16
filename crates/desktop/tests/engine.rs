//! The search engine manager over a fake process: started at the first
//! request and not before, kept, one request at a time, the model's
//! loading polled natively, `meaning_loading` as Cold, a crash respawned
//! once then Failed, killed on shutdown.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use palicanon_app_bridge::{SearchAnswer, SearchRequest, SearchStatus};
use palicanon_app_desktop::engine::{Engine, Process, Spawn};

/// What the fake processes share with the test.
#[derive(Default)]
struct World {
    /// Processes started.
    spawns: AtomicUsize,
    /// Processes killed.
    kills: AtomicUsize,
    /// Requests in flight now, and the most ever at once.
    in_flight: AtomicUsize,
    most: AtomicUsize,
    /// Every line sent.
    sent: Mutex<Vec<String>>,
    /// The answers to give, in order; `None` is a crash (the pipe closed).
    script: Mutex<VecDeque<Option<String>>>,
    /// The spawner refuses to start.
    refuse: std::sync::atomic::AtomicBool,
}

struct Fake(Arc<World>);

impl Process for Fake {
    fn ask(&mut self, line: &str) -> std::io::Result<String> {
        let now = self.0.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.0.most.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(5));
        self.0.sent.lock().unwrap().push(line.to_string());
        let next = self.0.script.lock().unwrap().pop_front().flatten();
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
        next.ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "closed"))
    }

    fn kill(&mut self) {
        self.0.kills.fetch_add(1, Ordering::SeqCst);
    }
}

struct Spawner(Arc<World>);

impl Spawn for Spawner {
    fn spawn(&self) -> std::io::Result<Box<dyn Process>> {
        if self.0.refuse.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("no binary"));
        }
        self.0.spawns.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Fake(Arc::clone(&self.0))))
    }
}

fn engine(script: &[Option<&str>]) -> (Engine, Arc<World>) {
    let world = Arc::new(World::default());
    *world.script.lock().unwrap() = script.iter().map(|a| a.map(str::to_string)).collect();
    (
        Engine::new(Spawner(Arc::clone(&world)), Duration::from_millis(1)),
        world,
    )
}

fn request(mode: &str) -> SearchRequest {
    SearchRequest {
        query: "the raft".into(),
        mode: mode.into(),
        collection: "mn".into(),
    }
}

const LOADING: &str = r#"{"ready":true,"meaning":false,"loading":true}"#;
const MEANING: &str = r#"{"ready":true,"meaning":true}"#;
const HITS: &str = r#"{"query":"the raft","mode":"words","hits":[{"sutta_uid":"mn22","display_id":"MN 22","title":"T","collection":"mn","snippet":"s","url":"https://suttacentral.net/mn22","sources":{"dense":false,"sparse":true},"score":1.0}]}"#;

#[test]
fn nothing_starts_before_the_first_request() {
    let (engine, world) = engine(&[Some(MEANING)]);
    assert!(!engine.running());
    assert_eq!(world.spawns.load(Ordering::SeqCst), 0);
    engine.status().unwrap();
    assert!(engine.running());
    assert_eq!(world.spawns.load(Ordering::SeqCst), 1);
}

#[test]
fn status_is_asked_again_while_the_model_loads_then_answers() {
    let (engine, world) = engine(&[Some(LOADING), Some(LOADING), Some(MEANING)]);
    assert_eq!(
        engine.status().unwrap(),
        SearchStatus {
            ready: true,
            meaning: true
        }
    );
    assert_eq!(world.sent.lock().unwrap().len(), 3);
    assert_eq!(
        world.spawns.load(Ordering::SeqCst),
        1,
        "the process is kept"
    );
}

#[test]
fn a_model_that_failed_leaves_meaning_off() {
    let unavailable = r#"{"ready":true,"meaning":false,"meaning_error":"no model"}"#;
    let (engine, _) = engine(&[Some(unavailable)]);
    assert_eq!(
        engine.status().unwrap(),
        SearchStatus {
            ready: true,
            meaning: false
        }
    );
}

#[test]
fn a_search_sends_one_line_and_maps_the_answers() {
    let loading = r#"{"error":"meaning_loading","message":"ask again"}"#;
    let refused = r#"{"error":"meaning_unavailable","message":"no model"}"#;
    let (engine, world) = engine(&[Some(HITS), Some(loading), Some(refused), Some("not json")]);
    let SearchAnswer::Hits(hits) = engine.search(&request("words")) else {
        panic!("hits")
    };
    assert_eq!(hits.hits[0].sutta_uid, "mn22");
    let sent: serde_json::Value = serde_json::from_str(&world.sent.lock().unwrap()[0]).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({"search": "the raft", "mode": "words", "collection": "mn", "k": 20})
    );
    assert_eq!(engine.search(&request("both")), SearchAnswer::Cold);
    assert!(
        matches!(engine.search(&request("both")), SearchAnswer::Failed(m) if m.contains("meaning_unavailable"))
    );
    assert!(matches!(
        engine.search(&request("words")),
        SearchAnswer::Failed(_)
    ));
}

#[test]
fn a_crash_respawns_once_then_every_answer_is_failed() {
    let (engine, world) = engine(&[None, Some(HITS), None]);
    assert!(matches!(
        engine.search(&request("words")),
        SearchAnswer::Hits(_)
    ));
    assert_eq!(world.spawns.load(Ordering::SeqCst), 2);
    assert_eq!(world.kills.load(Ordering::SeqCst), 1);
    assert!(matches!(
        engine.search(&request("words")),
        SearchAnswer::Failed(_)
    ));
    assert!(matches!(
        engine.search(&request("words")),
        SearchAnswer::Failed(_)
    ));
    assert!(engine.status().is_err());
    assert_eq!(world.spawns.load(Ordering::SeqCst), 2, "no third start");
}

#[test]
fn an_engine_that_cannot_start_answers_failed() {
    let (engine, world) = engine(&[]);
    world
        .refuse
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(matches!(
        engine.search(&request("words")),
        SearchAnswer::Failed(_)
    ));
    assert!(engine.status().is_err());
}

#[test]
fn one_request_at_a_time() {
    let script: Vec<Option<&str>> = std::iter::repeat_n(Some(HITS), 16).collect();
    let (engine, world) = engine(&script);
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let engine = engine.clone();
            std::thread::spawn(move || {
                engine.search(&request("words"));
                engine.search(&request("words"));
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(world.sent.lock().unwrap().len(), 16);
    assert_eq!(world.most.load(Ordering::SeqCst), 1);
    assert_eq!(world.spawns.load(Ordering::SeqCst), 1);
}

#[test]
fn shutdown_kills_the_process_and_nothing_starts_after() {
    let (engine, world) = engine(&[Some(MEANING)]);
    engine.status().unwrap();
    engine.shutdown();
    assert_eq!(world.kills.load(Ordering::SeqCst), 1);
    assert!(!engine.running());
    assert!(matches!(
        engine.search(&request("words")),
        SearchAnswer::Failed(_)
    ));
    assert_eq!(world.spawns.load(Ordering::SeqCst), 1);
}
