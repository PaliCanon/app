//! Where Learn is mounted: learn-pali 048's fragment under `/learn`. This is
//! the only file that knows how: a `ParentRoute` at `/learn` around the
//! fragment's routes (`''`, the home). The progress context
//! (`LearnProvider`) sits above the router, in `app.rs`: a store is not
//! `Send`, a router's children must be.

use learn_pali::fragment::LearnRoutes;
use leptos::prelude::*;
use leptos_router::MatchNestedRoutes;
use leptos_router::components::{Outlet, ParentRoute};
use leptos_router::path;

/// The `/learn` routes.
#[component(transparent)]
pub fn LearnMount() -> impl MatchNestedRoutes + Clone + Send + 'static {
    view! {
        <ParentRoute path=path!("/learn") view=|| view! { <Outlet /> }>
            <LearnRoutes />
        </ParentRoute>
    }
    .into_inner()
}
