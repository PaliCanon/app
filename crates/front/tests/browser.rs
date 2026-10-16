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
use wasm_bindgen::{JsCast, JsValue};
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

/// Goes to `path` the way the back button does: the address changes, then
/// the browser fires `popstate`, which the router follows.
fn go_to(path: &str) {
    let window = window();
    window
        .history()
        .expect("history")
        .push_state_with_url(&JsValue::NULL, "", Some(path))
        .expect("push state");
    let event = web_sys::Event::new("popstate").expect("an event");
    window.dispatch_event(&event).expect("dispatch");
}

fn pathname() -> String {
    window().location().pathname().unwrap_or_default()
}

#[wasm_bindgen_test]
async fn the_app_mounts_the_map_and_a_sibling_route_unmounts_it() {
    palicanon_ui_lib::csr::mount(App);

    // `/` redirects to `/map`; the map route shows.
    until("the map route mounts", || page_text().contains(STUB)).await;
    assert_eq!(pathname(), "/map");

    // The rail: a real link to /map, the current entry.
    let document = window().document().expect("a document");
    let link = document
        .query_selector(r#"a[href="/map"]"#)
        .expect("a selector")
        .expect("the rail's Map link");
    assert_eq!(link.get_attribute("aria-current").as_deref(), Some("page"));
    assert!(link.dyn_ref::<web_sys::HtmlElement>().is_some());

    // The footer's links go to the main site over https.
    let terms = document
        .query_selector(r#"a[href="https://palicanon.app/terms"]"#)
        .expect("a selector");
    assert!(terms.is_some());

    // A sibling route unmounts the map.
    go_to("/elsewhere");
    until("the map unmounts", || !page_text().contains(STUB)).await;
    assert!(page_text().contains("Not found"), "{}", page_text());

    // And the map comes back.
    go_to("/map/sim");
    until("the map mounts again", || page_text().contains(STUB)).await;
}
