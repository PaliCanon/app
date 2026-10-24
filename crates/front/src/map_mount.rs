//! Where the map is mounted: the vizualisation 012 fragment under `/map`. This
//! is the only file that knows how: the host gives it the app's data
//! ([`crate::data::BundledData`]) and no head of its own (the window's title is the app's),
//! and mounts its routes (`''`, `:uid`, `sim`, `sim/:uid`) inside the parent
//! route.

use std::sync::Arc;

use leptos::prelude::*;
use leptos_router::MatchNestedRoutes;
use leptos_router::components::{Outlet, ParentRoute};
use leptos_router::path;
use vizualisation::fragment::{MapData, MapHead, MapProvider, MapRoutes};

/// Where the map's data files come from, given by the app's root (the bundled
/// files in the app, a fake in the browser test).
#[derive(Clone)]
pub struct MapSource(pub Arc<dyn MapData>);

/// The `/map` routes.
#[component(transparent)]
pub fn MapMount() -> impl MatchNestedRoutes + Clone + Send + 'static {
    view! {
        <ParentRoute path=path!("/map") view=MapHost>
            <MapRoutes />
        </ParentRoute>
    }
    .into_inner()
}

/// The map's provider (its data and its head renderer, one per page) around
/// the routes' outlet. Leaving `/map` disposes this view: the map stops its
/// loads and listeners and releases the GPU.
#[component]
fn MapHost() -> impl IntoView {
    let MapSource(data) = use_context::<MapSource>().expect("the app root provides the map's data");
    view! {
        <MapProvider data=data head=|_: MapHead| ()>
            <Outlet />
        </MapProvider>
    }
}
