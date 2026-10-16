//! The PaliCanon app's front: client-side Leptos on ui-lib, built to wasm by
//! Trunk and embedded in the desktop shell.
//!
//! One router inside ui-lib's `AppShell`: a rail with the app's services (the
//! Map for now), the routes (`/` goes to `/map`, `/map` mounts the map, any
//! other address is "Not found"), ui-lib's footer with the app's own links.
//! The browser entry is `main.rs` (`palicanon_ui_lib::csr::mount(App)`).

pub mod app;
pub mod map_mount;
pub mod nav;
pub mod strings;
