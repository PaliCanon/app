//! Where Search is mounted: search-front 021's fragment under `/search`.
//! This is the only file that knows how: a `ParentRoute` at `/search` whose
//! view is the fragment's `SearchProvider` over the app's backend (the local
//! engine, [`crate::search::AppSearch`], given by the app's root) around the
//! fragment's routes.

use leptos::prelude::*;
use leptos_router::MatchNestedRoutes;
use leptos_router::components::{Outlet, ParentRoute};
use leptos_router::path;
use search_front::fragment::{SearchProvider, SearchRoutes};

use crate::search::AppSearch;

/// The `/search` routes.
#[component(transparent)]
pub fn SearchMount() -> impl MatchNestedRoutes + Clone + 'static {
    view! {
        <ParentRoute path=path!("/search") view=SearchHost>
            <SearchRoutes />
        </ParentRoute>
    }
    .into_inner()
}

/// The fragment's provider over the app's backend, around the outlet. Its
/// first mount reads `meaning()`, which starts the engine.
#[component]
fn SearchHost() -> impl IntoView {
    let backend = use_context::<AppSearch>().expect("the app root provides the search backend");
    view! {
        <SearchProvider backend=backend>
            <Outlet />
        </SearchProvider>
    }
}
