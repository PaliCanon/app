//! The app in a real browser (a headless Chrome through
//! `wasm-bindgen-test-runner`): `csr::mount` builds the page from an empty
//! body; the rail lists Learn, Search and the Map; `/` lands on `/learn`,
//! Learn's home shows over the app's store, and a Learn answer reaches the
//! fake `progress_save` (the next seq, a state's JSON); the rail's Map link
//! mounts the real map over a fake `MapData` (the staged data files), a
//! sibling route unmounts it and the engine's live views fall back to 0, the
//! Map link brings it back, and none of it panics (a panic fails the test).
//! One test, one page: `csr::mount` mounts into `<body>` and leaks its tree.
//!
//! The data files are the ones `cargo xtask data` stages in `target/map-data`
//! (run it first; the build fails without them). Headless Chrome draws with
//! `WebGL2` (`SwiftShader`), so the first draw is slow in a debug build.
//!
//! ```sh
//! cargo xtask data && cargo xtask assets
//! CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
//!   CHROMEDRIVER=<chromedriver matching Chrome> WASM_BINDGEN_TEST_TIMEOUT=600 \
//!   cargo test --locked -p palicanon-app-front --target wasm32-unknown-unknown --test browser
//! ```
#![cfg(target_arch = "wasm32")]

use std::sync::{Arc, Mutex};

use leptos::prelude::*;
use palicanon_app_front::app::AppWith;
use palicanon_app_front::files::{AppFiles, FilesPort};
use palicanon_app_front::progress::{AppStore, ProgressPort};
use palicanon_app_front::search::{AppSearch, PortFuture, SearchPort};
use vizualisation::fragment::{MapData, MapFile};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// One staged data file.
macro_rules! staged {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/map-data/",
            $name
        ))
    };
}

/// The staged data files, as the app bundles them.
fn bundled(name: &str) -> Option<Vec<u8>> {
    let bytes: &[u8] = match name {
        "graph.json" => staged!("graph.json"),
        "graph-sim.json" => staged!("graph-sim.json"),
        "graph-sim.bin" => staged!("graph-sim.bin"),
        "translations.json" => staged!("translations.json"),
        "blurbs-en.json" => staged!("blurbs-en.json"),
        "blurbs-fr.json" => staged!("blurbs-fr.json"),
        _ => return None,
    };
    Some(bytes.to_vec())
}

/// A `MapData` over the staged files that records what it is asked.
#[derive(Clone, Default)]
struct Fake {
    asked: Arc<Mutex<Vec<&'static str>>>,
}

impl MapData for Fake {
    fn file(&self, name: &'static str) -> MapFile {
        self.asked.lock().expect("asked").push(name);
        let bytes = bundled(name);
        Box::pin(async move { bytes })
    }
}

fn window() -> web_sys::Window {
    web_sys::window().expect("a window")
}

fn document() -> web_sys::Document {
    window().document().expect("a document")
}

fn page_text() -> String {
    document()
        .body()
        .map(|b| b.inner_text())
        .unwrap_or_default()
}

/// Waits `ms` milliseconds.
async fn sleep(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    _ = JsFuture::from(promise).await;
}

/// Waits (up to 60 s) until `condition` holds.
async fn until(what: &str, condition: impl Fn() -> bool) {
    for _ in 0..3000 {
        if condition() {
            return;
        }
        sleep(20).await;
    }
    panic!(
        "never: {what}; live views {}; the page says {:?}",
        map_engine::live_views(),
        page_text()
    );
}

/// Follows a link to `path` that the app itself does not have (a page of
/// the app never links to an unknown address): an anchor is added to the
/// page and clicked, so the router handles it like any link click. A raw
/// `pushState` would bypass the router's own history stack.
fn follow_foreign_link(path: &str) {
    let anchor = document().create_element("a").expect("an anchor");
    anchor.set_attribute("href", path).expect("href");
    let body = document().body().expect("a body");
    body.append_child(&anchor).expect("append");
    anchor
        .dyn_into::<web_sys::HtmlElement>()
        .expect("an element")
        .click();
}

fn pathname() -> String {
    window().location().pathname().unwrap_or_default()
}

fn rail_map_link() -> web_sys::Element {
    document()
        .query_selector(r#"a[href="/map"]"#)
        .expect("a selector")
        .expect("the rail's Map link")
}

fn has_canvas() -> bool {
    document().query_selector("canvas").ok().flatten().is_some()
}

/// The progress and file ports, recording what Learn sends.
#[derive(Clone, Default)]
struct Ports {
    saves: std::rc::Rc<std::cell::RefCell<Vec<(u64, String)>>>,
}

impl ProgressPort for Ports {
    fn save(&self, seq: u64, text: String) {
        self.saves.borrow_mut().push((seq, text));
    }
}

impl FilesPort for Ports {
    fn export(&self, _name: String, _text: String) {}
    fn import(&self, _done: Box<dyn FnOnce(palicanon_app_bridge::ImportAnswer)>) {}
}

/// A search engine that is never ready (the browser test does not search).
struct NoEngine;

impl SearchPort for NoEngine {
    fn search(
        &self,
        _request: palicanon_app_bridge::SearchRequest,
    ) -> PortFuture<palicanon_app_bridge::SearchAnswer> {
        Box::pin(async { Ok(palicanon_app_bridge::SearchAnswer::Cold) })
    }
    fn status(&self) -> PortFuture<palicanon_app_bridge::SearchStatus> {
        Box::pin(async { Err("no engine in the browser test".to_string()) })
    }
}

/// Mounts the app over the fake map data and fake ports, the progress empty.
fn mount(data: Arc<dyn MapData>, ports: &Ports) {
    let store = AppStore::new(
        Ok(palicanon_app_bridge::LoadAnswer::Missing),
        std::rc::Rc::new(ports.clone()),
    );
    let files = AppFiles::new(std::rc::Rc::new(ports.clone()));
    let search = AppSearch::new(Arc::new(NoEngine));
    palicanon_ui_lib::csr::mount(
        move || view! { <AppWith data=data store=store files=files search=search /> },
    );
}

/// The first visible `<button>` whose text contains `label` (Learn renders
/// its grade buttons before the reveal, hidden).
fn button(label: &str) -> Option<web_sys::HtmlElement> {
    let buttons = document().query_selector_all("button").expect("buttons");
    (0..buttons.length())
        .filter_map(|at| buttons.item(at))
        .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
        .filter(|b| b.offset_parent().is_some() && !b.has_attribute("disabled"))
        .find(|b| {
            // An icon button has no text: its name is its `aria-label` (the
            // '+' is 'New terms' since Learn fragment-v0.4.0).
            b.text_content().is_some_and(|text| text.contains(label))
                || b.get_attribute("aria-label")
                    .is_some_and(|name| name.contains(label))
        })
}

/// Clicks the button `label` once it is there and visible.
async fn click(label: &str) {
    until(&format!("the {label} button"), || button(label).is_some()).await;
    button(label).expect("the button").click();
}

/// `/` lands on `/learn`, the home shows, the rail lists the three services,
/// and a Learn answer reaches the fake `progress_save`.
async fn learn_saves_an_answer(ports: &Ports) {
    until("the address is /learn", || pathname() == "/learn").await;
    until("Learn's home", || {
        document()
            .query_selector(r#"[data-page="home"]"#)
            .ok()
            .flatten()
            .is_some()
    })
    .await;
    for href in ["/learn", "/search", "/map"] {
        assert!(
            document()
                .query_selector(&format!(r#"nav a[href="{href}"]"#))
                .ok()
                .flatten()
                .is_some(),
            "the rail links to {href}: {}",
            page_text()
        );
    }
    assert_eq!(ports.saves.borrow().len(), 0, "mounting saves nothing");
    // An empty progress: the "+" opens the new terms' chooser, "Add" adds
    // them and starts the session, then the first card is answered.
    click("New terms").await;
    // The picker (learn-pali 063): a count, then Add.
    click("+5").await;
    click("Add").await;
    let before = ports.saves.borrow().len();
    click("Show the answer").await;
    click("Knew it").await;
    until("an answer is saved", || ports.saves.borrow().len() > before).await;
    let saves = ports.saves.borrow();
    let (seq, text) = saves.last().expect("a save");
    assert_eq!(
        *seq,
        u64::try_from(saves.len()).unwrap(),
        "seq counts the saves"
    );
    let state = learn_core::progress::State::from_json(text).expect("a state's JSON");
    assert_ne!(state.progress.len(), 0, "{text}");
}

/// The rail's Map link mounts the real map over the fake data; its engine
/// starts, the rail marks the Map current, the footer links to the main site.
async fn the_map_draws(fake: &Fake) {
    rail_map_link()
        .dyn_into::<web_sys::HtmlElement>()
        .expect("an element")
        .click();
    until("the address is /map", || pathname() == "/map").await;
    until("the engine has a live view", || {
        map_engine::live_views() == 1
    })
    .await;
    assert!(has_canvas(), "{}", page_text());
    assert!(
        document()
            .document_element()
            .and_then(|html| html.get_attribute("data-engine"))
            .is_some(),
        "<html data-engine> names the engine drawing"
    );
    assert!(
        fake.asked.lock().expect("asked").contains(&"graph.json"),
        "the map asked the host's MapData for its graph"
    );

    // The rail: a real link to /map, the current entry; the footer's links go
    // to the main site over https.
    assert_eq!(
        rail_map_link().get_attribute("aria-current").as_deref(),
        Some("page")
    );
    let terms = document()
        .query_selector(r#"a[href="https://palicanon.app/terms"]"#)
        .expect("a selector");
    assert!(terms.is_some());
}

#[wasm_bindgen_test]
async fn the_app_runs_learn_then_draws_the_map_and_a_sibling_route_releases_it() {
    let fake = Fake::default();
    let data: Arc<dyn MapData> = Arc::new(fake.clone());
    let ports = Ports::default();
    mount(data, &ports);
    learn_saves_an_answer(&ports).await;
    the_map_draws(&fake).await;

    // A sibling route unmounts the map: its engine is dropped, the GPU released.
    follow_foreign_link("/elsewhere");
    until("the live views fall to 0", || map_engine::live_views() == 0).await;
    until("Not found is shown", || page_text().contains("Not found")).await;
    assert!(!has_canvas());

    // The rail's Map link, clicked as a person does, brings the map back...
    rail_map_link()
        .dyn_into::<web_sys::HtmlElement>()
        .expect("an element")
        .click();
    until("the map is back", || map_engine::live_views() == 1).await;
    until("the address is /map", || pathname() == "/map").await;

    // ...and leaving it again releases it again.
    follow_foreign_link("/elsewhere");
    until("the live views fall to 0 again", || {
        map_engine::live_views() == 0
    })
    .await;
}
