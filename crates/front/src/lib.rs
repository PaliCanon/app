//! The PaliCanon app's front: client-side Leptos on ui-lib, built to wasm by
//! Trunk and embedded in the desktop shell.
//!
//! The progress is loaded first (`progress_load`), the store built from it
//! ([`progress::AppStore`]), then the app mounted: Learn's progress context
//! above one router inside ui-lib's `AppShell`, a rail with the app's
//! services, the routes (`/learn` mounts Learn, `/search` Search over the
//! local engine, `/map` the map, any other address is "Not found"), Settings with Learn's 'Your progress', ui-lib's
//! footer with the app's own links. The browser entry is `main.rs`; the
//! bridge to the shell is the `tauri` module (browser only).

// The shell around Learn is a deep view type (learn-pali does the same).
#![recursion_limit = "512"]

pub mod app;
pub mod data;
pub mod files;
pub mod learn_mount;
pub mod map_mount;
pub mod nav;
pub mod progress;
pub mod search;
pub mod search_mount;
pub mod strings;
#[cfg(target_arch = "wasm32")]
pub mod tauri;
