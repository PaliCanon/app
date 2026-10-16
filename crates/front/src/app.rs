//! The app: one router, ui-lib's shell around the routes.

use leptos::prelude::*;
use leptos_router::components::{Redirect, Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;
use palicanon_ui_lib::{AppShell, Footer, NavList, language_signal, provide_language};

use std::sync::Arc;

use vizualisation::fragment::MapData;

use crate::data::{BrowserFetch, BundledData};
use crate::map_mount::{MapMount, MapSource};
use crate::{nav, strings};

/// The app root: the map's data is the files bundled in the app. Mounted by
/// `palicanon_ui_lib::csr::mount`.
#[component]
pub fn App() -> impl IntoView {
    let data: Arc<dyn MapData> = Arc::new(BundledData::<BrowserFetch>::default());
    view! { <AppWith data=data /> }
}

/// The app over a given source of the map's data files (the browser test
/// gives a fake): the language signal of the page, then the router around the
/// shell.
#[component]
pub fn AppWith(
    /// Where the map reads its data files from.
    data: Arc<dyn MapData>,
) -> impl IntoView {
    provide_language();
    provide_context(MapSource(data));
    view! {
        <Router>
            <Shell>
                <AppRoutes />
            </Shell>
        </Router>
    }
}

/// ui-lib's shell: the rail (the current entry follows the router's path),
/// the page in `<main>`, the footer with the app's links.
#[component]
fn Shell(children: Children) -> impl IntoView {
    let language = language_signal();
    let location = use_location();
    let sections = Signal::derive(move || nav::sections(language.get()));
    let current = Signal::derive(move || location.pathname.get());
    view! {
        <AppShell
            nav=NavList::new(sections, current)
            footer=move || {
                view! { <Footer links=nav::footer_links(language.get()) /> }
            }
        >
            <main class="flex flex-1 flex-col">{children()}</main>
        </AppShell>
    }
}

/// The routes alone, without the shell: `/` goes to `/map`, `/map` mounts the
/// map, anything else is the "Not found" page.
#[component]
pub fn AppRoutes() -> impl IntoView {
    view! {
        <Routes fallback=NotFound>
            <Route path=path!("") view=|| view! { <Redirect path="/map" /> } />
            <MapMount />
        </Routes>
    }
}

/// The page of an address the app does not know.
#[component]
fn NotFound() -> impl IntoView {
    let language = language_signal();
    view! {
        <section class="p-6">
            <h1 class="font-serif text-2xl text-heading">
                {move || strings::of(language.get()).not_found}
            </h1>
        </section>
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::{Arc, Mutex};

    use any_spawner::Executor;
    use leptos::tachys::view::RenderHtml;
    use leptos_router::MatchNestedRoutes;
    use leptos_router::components::{Router, provide_server_redirect};
    use leptos_router::location::RequestUrl;
    use palicanon_ui_lib::{Language, Theme};

    use super::*;

    /// What a request for `path` gives: the markup and the redirect target.
    fn render(path: &str) -> (String, Option<String>) {
        // The map spawns local tasks (its effects); they need a local set.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        let local = tokio::task::LocalSet::new();
        local.block_on(&runtime, async { render_in_place(path) })
    }

    fn render_in_place(path: &str) -> (String, Option<String>) {
        _ = Executor::init_tokio();
        let owner = Owner::new();
        owner.set();
        provide_context(RequestUrl::new(path));
        // The app root provides the page's language signal (`App`); a
        // browser-free test provides one itself.
        provide_context(RwSignal::new(Language::En));
        // The same for the theme the map reads: ui-lib's browser code (cookies)
        // does not run natively.
        provide_context(RwSignal::new(Theme::System));
        let target = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&target);
        provide_server_redirect(move |to| {
            *sink.lock().unwrap() = Some(to.to_string());
        });
        let html = view! {
            <Router>
                <AppRoutes />
            </Router>
        }
        .to_html();
        let to = target.lock().unwrap().clone();
        (html, to)
    }

    #[test]
    fn the_root_redirects_to_the_map() {
        let (_, to) = render("/");
        assert_eq!(to.as_deref(), Some("/map"));
    }

    /// The map's route table is the real `MapRoutes` under `/map`: the map's
    /// four addresses match entirely, nothing else does. (The map itself
    /// draws in a browser only: ui-lib's popovers and the engine need one;
    /// `tests/browser.rs` mounts it.)
    #[test]
    fn the_map_routes_are_the_fragments_four_addresses_under_map() {
        let owner = Owner::new();
        owner.set();
        let routes = MapMount();
        for path in ["/map", "/map/sim", "/map/mn10", "/map/sim/mn10"] {
            let (matched, rest) = routes.match_nested(path);
            assert!(matched.is_some(), "{path}");
            assert_eq!(rest, "", "{path}");
        }
        for path in [
            "/",
            "/mapx",
            "/learn",
            "/map/sim/a/b",
            "/map/a/b",
            "/search/map",
        ] {
            let (matched, rest) = routes.match_nested(path);
            assert!(matched.is_none() || !rest.is_empty(), "{path}");
        }
    }

    #[test]
    fn another_address_is_not_found() {
        for path in ["/nope", "/learn", "/search", "/map/sim/a/b", "/map/a/b/c"] {
            let (html, to) = render(path);
            assert!(html.contains("Not found"), "{path}: {html}");
            assert!(!html.contains("<canvas"), "{path}: {html}");
            assert_eq!(to, None, "{path}");
        }
    }
}
