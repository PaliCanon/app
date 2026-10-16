//! The app's build tasks, native and never shipped: `pins` reads the one
//! `cargo metadata` of the browser build for the lockstep check and the
//! checkout paths; `css` and `data` (added next) build from those paths.

pub mod css;
pub mod data;
pub mod pins;
