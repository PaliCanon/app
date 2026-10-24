//! The engine over the real `yuga-sidecar`, store and model staged by
//! `cargo xtask yuga` in `target/yuga` (650 MB, not in CI: ignored by
//! default, run in the local check list):
//!
//! ```sh
//! cargo xtask yuga
//! cargo test --locked -p palicanon-app-desktop --test sidecar -- --ignored
//! ```
//!
//! A words search answers at once; the status answers once the model is
//! loaded, with meaning; a both search then answers too.

use std::path::Path;
use std::time::Instant;

use palicanon_app_bridge::{SearchAnswer, SearchRequest, SearchStatus};
use palicanon_app_desktop::engine::{Engine, POLL, SidecarSpawn};

fn staged() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/yuga")
}

fn request(mode: &str) -> SearchRequest {
    SearchRequest {
        query: "the simile of the raft".into(),
        mode: mode.into(),
        collection: "all".into(),
    }
}

#[test]
#[ignore = "needs `cargo xtask yuga` (650 MB in target/yuga)"]
fn words_at_once_then_meaning_and_both() {
    let yuga = staged();
    let engine = Engine::new(
        SidecarSpawn {
            binary: yuga.join("bin/yuga-sidecar-aarch64-apple-darwin"),
            store: yuga.join("store"),
            model: yuga.join("model"),
        },
        POLL,
    );
    let started = Instant::now();
    let SearchAnswer::Hits(words) = engine.search(&request("words")) else {
        panic!("a words search answers with hits")
    };
    let words_after = started.elapsed();
    assert_ne!(words.hits.len(), 0);
    assert_eq!(
        engine.status().expect("a status"),
        SearchStatus {
            ready: true,
            meaning: true
        }
    );
    let meaning_after = started.elapsed();
    let SearchAnswer::Hits(both) = engine.search(&request("both")) else {
        panic!("a both search answers with hits once meaning is on")
    };
    assert_ne!(both.hits.len(), 0);
    assert!(both.hits.iter().any(|h| h.sources.dense), "{both:?}");
    eprintln!("words after {words_after:?}, meaning after {meaning_after:?}");
    engine.shutdown();
}
