//! The app: one router, ui-lib's shell around the routes.

use leptos::prelude::*;
use leptos_router::components::{Redirect, Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;
use palicanon_ui_lib::{AppShell, Footer, NavList, language_signal, provide_language};

use crate::map_mount::MapMount;
use crate::{nav, strings};

/// The app root: the language signal of the page, then the router around the
/// shell. Mounted by `palicanon_ui_lib::csr::mount`.
#[component]
pub fn App() -> impl IntoView {
    provide_language();
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
/// map, anything else is [`NotFound`].
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use any_spawner::Executor;
    use leptos::tachys::view::RenderHtml;
    use leptos_router::components::{Router, provide_server_redirect};
    use leptos_router::location::RequestUrl;
    use palicanon_ui_lib::Language;

    use super::*;

    /// What a request for `path` gives: the markup and the redirect target.
    fn render(path: &str) -> (String, Option<String>) {
        _ = Executor::init_tokio();
        let owner = Owner::new();
        owner.set();
        provide_context(RequestUrl::new(path));
        // The app root provides the page's language signal (`App`); a
        // browser-free test provides one itself.
        provide_context(RwSignal::new(Language::En));
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

    /// The stub's marker, and the map routes' (see `map_mount`).
    const MAP: &str = "The map arrives with vizualisation 012";

    #[test]
    fn the_root_redirects_to_the_map() {
        let (_, to) = render("/");
        assert_eq!(to.as_deref(), Some("/map"));
    }

    #[test]
    fn the_map_route_renders_the_map() {
        for path in ["/map", "/map/sim", "/map/some-uid", "/map/sim/some-uid"] {
            let (html, to) = render(path);
            assert!(html.contains(MAP), "{path}: {html}");
            assert_eq!(to, None, "{path}");
        }
    }

    #[test]
    fn another_address_is_not_found() {
        for path in ["/nope", "/learn", "/search", "/map/sim/a/b"] {
            let (html, to) = render(path);
            assert!(html.contains("Not found"), "{path}: {html}");
            assert!(!html.contains(MAP), "{path}: {html}");
            assert_eq!(to, None, "{path}");
        }
    }
}
