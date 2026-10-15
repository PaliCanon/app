//! The PaliCanon app's fixed facts.
//!
//! Constants the desktop and mobile shell (Tauri 2) reads, and that nothing
//! else may restate. They live in one place so a change is a deliberate edit
//! that fails a test.

/// The bundle identifier of the app on every platform.
///
/// It is fixed forever. Each operating system keys the app's data (the
/// learner's progress, the downloaded search stores, the settings) by this
/// identifier: the app-data folder on desktop, the sandbox container on
/// iOS and Android, the signing and store records. Changing it makes the
/// next version a different app that cannot see the user's data, and the
/// stores would list a new app instead of an update.
///
/// Reverse-DNS, lowercase, and it does not end in `.app` (Tauri warns on
/// that suffix: it clashes with the macOS `.app` bundle extension).
pub const BUNDLE_IDENTIFIER: &str = "app.palicanon.learn";

/// The name shown to the user: the window title, the home screen icon, the
/// store listing.
pub const DISPLAY_NAME: &str = "PaliCanon";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_identifier_is_pinned() {
        assert_eq!(BUNDLE_IDENTIFIER, "app.palicanon.learn");
    }

    #[test]
    fn display_name_is_pinned() {
        assert_eq!(DISPLAY_NAME, "PaliCanon");
    }

    #[test]
    fn identifier_is_lowercase() {
        assert_eq!(BUNDLE_IDENTIFIER, BUNDLE_IDENTIFIER.to_lowercase());
    }

    #[test]
    fn identifier_is_reverse_dns() {
        let parts: Vec<&str> = BUNDLE_IDENTIFIER.split('.').collect();
        assert!(parts.len() >= 3, "at least three labels: {parts:?}");
        for part in parts {
            assert!(!part.is_empty(), "no empty label");
            assert!(
                part.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "label {part:?} holds only a-z and 0-9"
            );
            assert!(
                !part.starts_with(|c: char| c.is_ascii_digit()),
                "label {part:?} starts with a letter"
            );
        }
    }

    #[test]
    fn identifier_does_not_end_in_dot_app() {
        assert_ne!(BUNDLE_IDENTIFIER.rsplit('.').next(), Some("app"));
    }
}
