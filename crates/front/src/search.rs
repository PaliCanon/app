//! The app's `SearchBackend` (search-front 021's trait): the local engine
//! through two commands, `search(request)` and `search_status()`; no network.
//!
//! `meaning()` is one signal, made at construction: false until the engine
//! reports meaning, then true. The first call of `meaning()` (the
//! `SearchProvider`'s, when Search is first opened) asks `search_status`,
//! which starts the engine natively and answers once its model has loaded or
//! failed; the signal turns true then if meaning is served. A search while
//! the engine starts answers `Cold` (the page's waking-up state); an error
//! answer, or a failed call, is `Failed`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use leptos::prelude::*;
use palicanon_app_bridge::{Hit, SearchAnswer, SearchRequest, SearchStatus};
use search_front::search::backend::{SearchBackend, SearchFuture, SearchQuery};
use search_front::search::{
    HitDto, Mode, SearchOutcome, SearchResponse, SnippetKind, Sources, Titles,
};

/// A boxed `Send` future (a wasm port wraps its own in `SendWrapper`).
pub type PortFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'static>>;

/// The two commands: the native ones in the app, a fake in the tests.
pub trait SearchPort: Send + Sync + 'static {
    /// `search(request)`.
    fn search(&self, request: SearchRequest) -> PortFuture<SearchAnswer>;
    /// `search_status()`.
    fn status(&self) -> PortFuture<SearchStatus>;
}

/// The app's search backend (cheap to clone).
#[derive(Clone)]
pub struct AppSearch {
    port: Arc<dyn SearchPort>,
    meaning: RwSignal<bool>,
    asked: Arc<AtomicBool>,
}

impl AppSearch {
    /// The backend over `port`; nothing is asked before `meaning()`.
    pub fn new(port: Arc<dyn SearchPort>) -> Self {
        AppSearch {
            port,
            meaning: RwSignal::new(false),
            asked: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// The request the engine is asked for `query`.
pub fn request_of(query: &SearchQuery) -> SearchRequest {
    SearchRequest {
        query: query.q.clone(),
        mode: query.mode.code().to_owned(),
        collection: query.collection.code().to_owned(),
    }
}

/// What the page shows for the engine's answer (or a failed call).
pub fn outcome_of(answer: Result<SearchAnswer, String>) -> SearchOutcome {
    match answer {
        Ok(SearchAnswer::Hits(hits)) => SearchOutcome::Hits(SearchResponse {
            query: hits.query,
            mode: Mode::parse(&hits.mode),
            hits: hits.hits.into_iter().map(hit_of).collect(),
            fell_back: false,
        }),
        Ok(SearchAnswer::Cold) => SearchOutcome::Cold { retry_after: None },
        Ok(SearchAnswer::Failed(_)) | Err(_) => SearchOutcome::Failed,
    }
}

/// One engine hit as the page's: the bundled store is English, so its
/// title is the English one; the snippet is the passage found.
fn hit_of(hit: Hit) -> HitDto {
    HitDto {
        sutta_uid: hit.sutta_uid,
        display_id: hit.display_id,
        titles: Titles {
            en: Some(hit.title.clone()),
            fr: None,
            vi: None,
        },
        title: hit.title,
        pali_title: None,
        snippet: hit.snippet,
        snippet_kind: SnippetKind::Passage,
        url: hit.url,
        sources: Sources {
            dense: hit.sources.dense,
            sparse: hit.sources.sparse,
        },
        score: hit.score,
    }
}

impl SearchBackend for AppSearch {
    fn search(&self, query: SearchQuery) -> SearchFuture {
        let asked = self.port.search(request_of(&query));
        Box::pin(async move { outcome_of(asked.await) })
    }

    fn meaning(&self) -> Signal<bool> {
        if !self.asked.swap(true, Ordering::SeqCst) {
            let status = self.port.status();
            let meaning = self.meaning;
            leptos::task::spawn(async move {
                if let Ok(SearchStatus { meaning: true, .. }) = status.await {
                    meaning.set(true);
                }
            });
        }
        self.meaning.into()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    use palicanon_app_bridge::{Hit, Hits, Sources as WireSources};
    use search_front::search::Collection;

    use super::*;

    /// A port answering a fixed status and search answer, counting calls.
    struct Fake {
        status: Result<SearchStatus, String>,
        answer: Mutex<Option<Result<SearchAnswer, String>>>,
        asked: Mutex<Vec<SearchRequest>>,
        statuses: AtomicUsize,
    }

    impl Fake {
        fn new(
            status: Result<SearchStatus, String>,
            answer: Result<SearchAnswer, String>,
        ) -> Arc<Self> {
            Arc::new(Fake {
                status,
                answer: Mutex::new(Some(answer)),
                asked: Mutex::new(Vec::new()),
                statuses: AtomicUsize::new(0),
            })
        }
    }

    impl SearchPort for Fake {
        fn search(&self, request: SearchRequest) -> PortFuture<SearchAnswer> {
            self.asked.lock().unwrap().push(request);
            let answer = self.answer.lock().unwrap().take().expect("one search");
            Box::pin(async move { answer })
        }
        fn status(&self) -> PortFuture<SearchStatus> {
            self.statuses.fetch_add(1, Ordering::SeqCst);
            let status = self.status.clone();
            Box::pin(async move { status })
        }
    }

    /// Runs `test` on a local tokio set (the status is awaited by a task).
    fn local(test: impl std::future::Future<Output = ()>) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _ = any_spawner::Executor::init_tokio();
        tokio::task::LocalSet::new().block_on(&runtime, test);
    }

    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    fn hits() -> SearchAnswer {
        SearchAnswer::Hits(Hits {
            query: "the raft".into(),
            mode: "words".into(),
            hits: vec![Hit {
                sutta_uid: "mn22".into(),
                display_id: "MN 22".into(),
                title: "The Simile of the Snake".into(),
                collection: Some("mn".into()),
                snippet: "a raft".into(),
                url: "https://suttacentral.net/mn22".into(),
                sources: WireSources {
                    dense: false,
                    sparse: true,
                },
                score: 2.5,
            }],
        })
    }

    fn query(mode: Mode) -> SearchQuery {
        SearchQuery {
            q: "the raft".into(),
            lang: "en",
            mode,
            collection: Collection::Mn,
        }
    }

    #[test]
    fn the_request_carries_the_engines_codes() {
        assert_eq!(
            request_of(&query(Mode::Both)),
            SearchRequest {
                query: "the raft".into(),
                mode: "both".into(),
                collection: "mn".into(),
            }
        );
    }

    #[test]
    fn the_answers_map_to_the_pages_outcomes() {
        let SearchOutcome::Hits(response) = outcome_of(Ok(hits())) else {
            panic!("hits")
        };
        assert_eq!(response.mode, Mode::Words);
        assert!(!response.fell_back);
        let hit = &response.hits[0];
        assert_eq!(hit.sutta_uid, "mn22");
        assert_eq!(hit.title, "The Simile of the Snake");
        assert_eq!(hit.titles.en.as_deref(), Some("The Simile of the Snake"));
        assert_eq!(hit.snippet_kind, SnippetKind::Passage);
        assert!(hit.sources.sparse && !hit.sources.dense);
        assert_eq!(
            outcome_of(Ok(SearchAnswer::Cold)),
            SearchOutcome::Cold { retry_after: None }
        );
        assert_eq!(
            outcome_of(Ok(SearchAnswer::Failed("x".into()))),
            SearchOutcome::Failed
        );
        assert_eq!(outcome_of(Err("ipc".into())), SearchOutcome::Failed);
    }

    #[test]
    fn a_search_asks_the_port_and_maps_its_answer() {
        local(async {
            let fake = Fake::new(
                Ok(SearchStatus {
                    ready: true,
                    meaning: false,
                }),
                Ok(hits()),
            );
            let backend = AppSearch::new(fake.clone());
            let outcome = backend.search(query(Mode::Words)).await;
            assert!(matches!(outcome, SearchOutcome::Hits(_)));
            assert_eq!(fake.asked.lock().unwrap()[0].mode, "words");
            assert_eq!(
                fake.statuses.load(Ordering::SeqCst),
                0,
                "a search alone asks no status"
            );
        });
    }

    /// Nothing is asked before `meaning()`; then one status, once; the
    /// signal turns true only when the engine reports meaning.
    #[test]
    fn meaning_turns_true_when_the_engine_reports_it() {
        local(async {
            let fake = Fake::new(
                Ok(SearchStatus {
                    ready: true,
                    meaning: true,
                }),
                Ok(hits()),
            );
            let backend = AppSearch::new(fake.clone());
            assert_eq!(fake.statuses.load(Ordering::SeqCst), 0);
            let meaning = backend.meaning();
            assert!(!meaning.get_untracked());
            settle().await;
            assert!(meaning.get_untracked());
            let again = backend.meaning();
            settle().await;
            assert!(again.get_untracked());
            assert_eq!(fake.statuses.load(Ordering::SeqCst), 1, "asked once");
        });
    }

    #[test]
    fn meaning_stays_false_without_the_model_or_the_engine() {
        for status in [
            Ok(SearchStatus {
                ready: true,
                meaning: false,
            }),
            Err("no engine".to_string()),
        ] {
            local(async move {
                let fake = Fake::new(status, Ok(hits()));
                let backend = AppSearch::new(fake);
                let meaning = backend.meaning();
                settle().await;
                assert!(!meaning.get_untracked());
            });
        }
    }
}
