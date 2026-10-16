//! Where the map is mounted: the ONE place to change when vizualisation
//! 012's first tag lands. Until then `/map` and its four children (`''`,
//! `:uid`, `sim`, `sim/:uid`, the shape of 012's `MapRoutes`) render a stub.
//!
//! With the tag: depend on the map crate (`csr`, no default features), wrap
//! the parent's view in `MapProvider { data: Arc::new(BundledData), head:
//! None }` around an `Outlet`, replace the four children by `MapRoutes`, and
//! add the crate to `xtask`'s `MAP_CRATE` and to the CSS sources.

use leptos::prelude::*;
use leptos_router::MatchNestedRoutes;
use leptos_router::components::{ParentRoute, Route};
use leptos_router::path;

/// The `/map` routes.
#[component(transparent)]
pub fn MapMount() -> impl MatchNestedRoutes + Clone + Send + 'static {
    view! {
        <ParentRoute path=path!("/map") view=MapStub>
            <Route path=path!("") view=Nothing />
            <Route path=path!(":uid") view=Nothing />
            <Route path=path!("sim") view=Nothing />
            <Route path=path!("sim/:uid") view=Nothing />
        </ParentRoute>
    }
    .into_inner()
}

/// The stand-in for the map: one line.
#[component]
fn MapStub() -> impl IntoView {
    view! { <p class="p-6 text-muted">"The map arrives with vizualisation 012"</p> }
}

/// A child route with no view of its own (the stub is the parent's).
#[component]
fn Nothing() -> impl IntoView {}
