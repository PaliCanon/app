//! The wire between the PaliCanon app's front (wasm, in the webview) and its
//! desktop shell (native): the commands' names, what they answer, the
//! progress status event and the caps. One shape, read by both sides, so the
//! two cannot drift: the front serialises what the desktop deserialises.
//!
//! Serde only: this crate builds native and for `wasm32-unknown-unknown`.

use serde::{Deserialize, Serialize};

/// `progress_load()` → [`LoadAnswer`]: the progress file, read now.
pub const PROGRESS_LOAD: &str = "progress_load";
/// `progress_save(seq, text)` → `()` or a refusal: puts `text` in the
/// writer's one slot and returns at once; the verdict comes as a
/// [`PROGRESS_STATUS_EVENT`].
pub const PROGRESS_SAVE: &str = "progress_save";
/// `export_file(name, text)` → `()`: the native save dialog, then the write.
pub const EXPORT_FILE: &str = "export_file";
/// `import_file()` → [`ImportAnswer`]: the native open dialog, then the read.
pub const IMPORT_FILE: &str = "import_file";
/// `search(request)` → [`SearchAnswer`]: one search of the local engine.
pub const SEARCH: &str = "search";
/// `search_status()` → [`SearchStatus`]: whether the engine serves, and
/// whether it serves meaning.
pub const SEARCH_STATUS: &str = "search_status";

/// Every command the webview may call, and nothing else (the capability
/// grants exactly these).
pub const COMMANDS: [&str; 6] = [
    PROGRESS_LOAD,
    PROGRESS_SAVE,
    EXPORT_FILE,
    IMPORT_FILE,
    SEARCH,
    SEARCH_STATUS,
];

/// The event the native writer emits after each write attempt, its payload
/// a [`StatusEvent`].
pub const PROGRESS_STATUS_EVENT: &str = "progress-status";

/// The largest text a command carries, in bytes (a progress, an export, an
/// import): 4 MiB. Anything larger is refused, never cut.
pub const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// What `progress_load` answers: the I/O only, the front classifies the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadAnswer {
    /// The file's text, as is.
    Text(String),
    /// There is no progress file yet.
    Missing,
    /// The read failed (or another PaliCanon holds the progress): the
    /// writer is shut for the session, nothing is ever written over it.
    Unreadable(String),
}

/// The arguments of `progress_save`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveArgs {
    /// The save's number, increasing with each save of the session: the
    /// writer keeps the newest, the front orders the events by it.
    pub seq: u64,
    /// The progress, `learn_core`'s `State` JSON.
    pub text: String,
}

/// The verdict of a write, by the save's number (`seq`). Tauri does not
/// promise the delivery order of events: the front ignores one whose `seq`
/// is below the last it applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusEvent {
    /// The save `seq` is on disk.
    Saved(u64),
    /// The save `seq` is waiting to be written.
    Pending(u64),
    /// The save `seq` could not be written; it is kept and retried.
    Transient(u64),
    /// The file holds a newer version: nothing was written, nothing will be
    /// for the session. `raw` is the file's text.
    NewerVersion {
        /// The refused save's number.
        seq: u64,
        /// The file's text, as is.
        raw: String,
    },
}

impl StatusEvent {
    /// The save's number the verdict is about.
    pub fn seq(&self) -> u64 {
        match self {
            Self::Saved(seq)
            | Self::Pending(seq)
            | Self::Transient(seq)
            | Self::NewerVersion { seq, .. } => *seq,
        }
    }
}

/// The arguments of `export_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportArgs {
    /// The suggested file name (reduced to a plain `.json` name natively).
    pub name: String,
    /// The text to write.
    pub text: String,
}

/// What `import_file` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportAnswer {
    /// The chosen file's text.
    Text(String),
    /// The learner closed the dialog without choosing.
    Cancelled,
    /// The dialog or the read failed (too large, not UTF-8, an I/O error).
    Failed(String),
}

/// The arguments of `search`: the page's query, in the engine's codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    /// The query, never blank.
    pub query: String,
    /// `words`, `meaning` or `both`.
    pub mode: String,
    /// `all` or a collection code (`dn`, `mn`, `sn`, `an`, `kn`).
    pub collection: String,
}

/// What `search_status` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchStatus {
    /// The engine serves searches (words at least).
    pub ready: bool,
    /// The engine serves meaning and both (its model is loaded).
    pub meaning: bool,
}

/// Which branches found a hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Sources {
    /// The meaning (dense) branch.
    pub dense: bool,
    /// The words (sparse) branch.
    pub sparse: bool,
}

/// One hit, as the engine answers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    /// The SuttaCentral uid, e.g. `mn10`.
    pub sutta_uid: String,
    /// The reference shown, e.g. `MN 10`.
    pub display_id: String,
    /// The title.
    pub title: String,
    /// The collection code, when the store has one.
    #[serde(default)]
    pub collection: Option<String>,
    /// The passage found.
    pub snippet: String,
    /// The SuttaCentral address.
    pub url: String,
    /// Which branches found it.
    pub sources: Sources,
    /// The fused score, comparable within one answer only.
    pub score: f32,
}

/// The hits of one search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hits {
    /// The query as asked.
    pub query: String,
    /// The mode searched.
    pub mode: String,
    /// The hits, best first.
    pub hits: Vec<Hit>,
}

/// What `search` answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SearchAnswer {
    /// The search answered.
    Hits(Hits),
    /// The engine is starting (or its model is): ask again shortly.
    Cold,
    /// The engine could not answer; the reason, for the log.
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: &T) {
        let json = serde_json::to_string(value).expect("serialises");
        let back: T = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(&back, value, "{json}");
    }

    #[test]
    fn the_six_commands_are_the_designs() {
        assert_eq!(
            COMMANDS,
            [
                "progress_load",
                "progress_save",
                "export_file",
                "import_file",
                "search",
                "search_status"
            ]
        );
        assert_eq!(PROGRESS_STATUS_EVENT, "progress-status");
        assert_eq!(MAX_TEXT_BYTES, 4_194_304);
    }

    #[test]
    fn every_load_answer_round_trips() {
        for answer in [
            LoadAnswer::Text("{\"version\":1}".into()),
            LoadAnswer::Missing,
            LoadAnswer::Unreadable("denied".into()),
        ] {
            round_trip(&answer);
        }
    }

    #[test]
    fn every_status_event_round_trips_and_names_its_seq() {
        for (event, seq) in [
            (StatusEvent::Saved(1), 1),
            (StatusEvent::Pending(2), 2),
            (StatusEvent::Transient(3), 3),
            (
                StatusEvent::NewerVersion {
                    seq: 4,
                    raw: "{\"version\":9}".into(),
                },
                4,
            ),
        ] {
            round_trip(&event);
            assert_eq!(event.seq(), seq);
        }
    }

    #[test]
    fn every_import_answer_round_trips() {
        for answer in [
            ImportAnswer::Text("{}".into()),
            ImportAnswer::Cancelled,
            ImportAnswer::Failed("too large".into()),
        ] {
            round_trip(&answer);
        }
    }

    #[test]
    fn the_arguments_round_trip_with_the_names_the_commands_take() {
        let save = SaveArgs {
            seq: 7,
            text: "{}".into(),
        };
        round_trip(&save);
        assert_eq!(
            serde_json::to_value(&save).unwrap(),
            serde_json::json!({"seq": 7, "text": "{}"})
        );
        let export = ExportArgs {
            name: "pc_learn-2026-10-16.json".into(),
            text: "{}".into(),
        };
        round_trip(&export);
        assert_eq!(
            serde_json::to_value(&export).unwrap(),
            serde_json::json!({"name": "pc_learn-2026-10-16.json", "text": "{}"})
        );
        let request = SearchRequest {
            query: "the raft".into(),
            mode: "both".into(),
            collection: "mn".into(),
        };
        round_trip(&request);
    }

    #[test]
    fn every_search_answer_round_trips() {
        let hit = Hit {
            sutta_uid: "mn22".into(),
            display_id: "MN 22".into(),
            title: "The Simile of the Snake".into(),
            collection: Some("mn".into()),
            snippet: "the raft".into(),
            url: "https://suttacentral.net/mn22".into(),
            sources: Sources {
                dense: true,
                sparse: false,
            },
            score: 0.5,
        };
        for answer in [
            SearchAnswer::Hits(Hits {
                query: "the raft".into(),
                mode: "both".into(),
                hits: vec![hit],
            }),
            SearchAnswer::Cold,
            SearchAnswer::Failed("crashed".into()),
        ] {
            round_trip(&answer);
        }
        round_trip(&SearchStatus {
            ready: true,
            meaning: false,
        });
    }

    /// The engine's own hit line (yuga-sidecar's wire) reads as a [`Hit`],
    /// with or without its collection.
    #[test]
    fn the_engines_hit_line_reads_as_a_hit() {
        let line = r#"{"sutta_uid":"mn22","display_id":"MN 22","title":"T","snippet":"s","url":"https://suttacentral.net/mn22","sources":{"dense":false,"sparse":true},"score":1.5}"#;
        let hit: Hit = serde_json::from_str(line).expect("a hit");
        assert_eq!(hit.collection, None);
        assert!(hit.sources.sparse && !hit.sources.dense);
    }
}
