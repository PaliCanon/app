//! The app in a real browser (a headless Chrome through
//! `wasm-bindgen-test-runner`): `csr::mount` builds the page from an empty
//! body, the rail is there, `/` lands on `/map` and the real map mounts over a
//! fake `MapData` (the staged data files), a sibling route unmounts it and the
//! engine's live views fall back to 0, the rail's Map link brings it back, and
//! none of it panics (a panic fails the test).
//!
//! The data files are the ones `cargo xtask data` stages in `target/map-data`
//! (run it first; the build fails without them). Headless Chrome draws with
//! `WebGL2` (`SwiftShader`), so the first draw is slow in a debug build.
//!
//! ```sh
//! cargo xtask data
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

#[wasm_bindgen_test]
async fn the_app_draws_the_map_and_a_sibling_route_releases_it() {
    let fake = Fake::default();
    let data: Arc<dyn MapData> = Arc::new(fake.clone());
    mount(data, &Ports::default());

    // `/` lands on `/map`; the real map mounts and its engine starts.
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
