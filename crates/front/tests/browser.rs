//! The app in a real browser (a headless Chrome through
//! `wasm-bindgen-test-runner`): `csr::mount(App)` builds the page from an
//! empty body, the rail is there, `/` lands on `/map` and mounts the map, a
//! sibling route unmounts it, and none of it panics (a panic fails the test).
//!
//! ```sh
//! CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
//!   CHROMEDRIVER=<chromedriver matching Chrome> \
//!   cargo test --locked -p palicanon-app-front --target wasm32-unknown-unknown --test browser
//! ```
#![cfg(target_arch = "wasm32")]

use palicanon_app_front::app::App;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const STUB: &str = "The map arrives with vizualisation 012";

fn window() -> web_sys::Window {
    web_sys::window().expect("a window")
}

fn page_text() -> String {
    let body = window().document().and_then(|d| d.body());
    body.map(|b| b.inner_text()).unwrap_or_default()
}

/// Waits `ms` milliseconds.
async fn sleep(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    _ = JsFuture::from(promise).await;
}

/// Waits (up to 3 s) until `condition` holds.
async fn until(what: &str, condition: impl Fn() -> bool) {
    for _ in 0..60 {
        if condition() {
            return;
        }
        sleep(50).await;
    }
    panic!("never: {what}; the page says {:?}", page_text());
}

/// Follows a link to `path` that the app itself does not have (a page of
/// the app never links to an unknown address): an anchor is added to the
/// page and clicked, so the router handles it like any link click. A raw
/// `pushState` would bypass the router's own history stack.
fn follow_foreign_link(path: &str) {
    let document = window().document().expect("a document");
    let anchor = document.create_element("a").expect("an anchor");
    anchor.set_attribute("href", path).expect("href");
    let body = document.body().expect("a body");
    body.append_child(&anchor).expect("append");
    anchor
        .dyn_into::<web_sys::HtmlElement>()
        .expect("an element")
        .click();
}

fn pathname() -> String {
    window().location().pathname().unwrap_or_default()
}

#[wasm_bindgen_test]
async fn the_app_mounts_the_map_and_a_sibling_route_unmounts_it() {
    palicanon_ui_lib::csr::mount(App);

    // `/` redirects to `/map`; the map route shows.
    until("the map route mounts", || page_text().contains(STUB)).await;
    until("the address is /map", || pathname() == "/map").await;

    // The rail: a real link to /map, the current entry.
    let document = window().document().expect("a document");
    let link = document
        .query_selector(r#"a[href="/map"]"#)
        .expect("a selector")
        .expect("the rail's Map link");
    assert_eq!(link.get_attribute("aria-current").as_deref(), Some("page"));

    // The footer's links go to the main site over https.
    let terms = document
        .query_selector(r#"a[href="https://palicanon.app/terms"]"#)
        .expect("a selector");
    assert!(terms.is_some());

    // A sibling route unmounts the map (clicked, never pushed by hand).
    follow_foreign_link("/elsewhere");
    until("the map unmounts", || !page_text().contains(STUB)).await;
    assert!(page_text().contains("Not found"), "{}", page_text());

    // And the map comes back by a click on the rail's Map link, as a person
    // does it (the router follows the anchor, no state is pushed by hand).
    let link = document
        .query_selector(r#"a[href="/map"]"#)
        .expect("a selector")
        .expect("the rail's Map link");
    link.dyn_into::<web_sys::HtmlElement>()
        .expect("an element")
        .click();
    until("the map mounts again", || page_text().contains(STUB)).await;
    // The address follows the render by a tick.
    until("the address is /map", || pathname() == "/map").await;
}
