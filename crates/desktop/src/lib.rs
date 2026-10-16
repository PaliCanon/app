//! The PaliCanon desktop app: a Tauri 2 shell around the front. The window
//! loads the embedded front from `tauri://localhost`; what it may navigate
//! to is decided in one place, [`navigation`].

pub mod navigation;
