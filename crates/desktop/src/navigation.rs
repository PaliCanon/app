//! The one navigation rule, applied to every navigation and every new
//! window of the webview: the app stays on `tauri://localhost`, an `https:`
//! or `mailto:` link opens in the system browser, and everything else is
//! dropped. No new window is ever created.
//!
//! The rule is for macOS, where the app is served at `tauri://localhost`.
//! Windows and Android serve it at `http://tauri.localhost`, which the rule
//! drops (it is not `tauri:`): those targets come with their own tasks, and
//! the rule gets that origin then, with its test.

use url::Url;

/// What to do with a navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    /// Let the webview go there (the app's own pages).
    Stay,
    /// Do not navigate; open the address in the system browser.
    OpenOutside,
    /// Do nothing at all.
    Drop,
}

/// The rule, as a pure function of the address.
pub fn decide(url: &Url) -> Navigation {
    match url.scheme() {
        "tauri" if is_app(url) => Navigation::Stay,
        "https" | "mailto" => Navigation::OpenOutside,
        _ => Navigation::Drop,
    }
}

/// Exactly `tauri://localhost`: no other host (`localhost.evil.example`), no
/// port, no credentials.
fn is_app(url: &Url) -> bool {
    url.host_str() == Some("localhost")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

/// What happened to a navigation or a request for a new window. `decide`
/// stays pure; this is the outcome once the system browser was asked, so a
/// refusal from the system is told apart from a designed drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The webview may go there (the app's own page).
    Stayed,
    /// Handed to the system browser, which accepted it.
    Opened,
    /// Handed to the system browser, which refused it (the reason).
    OpenRefused(String),
    /// Designed to be dropped: not an address the app follows.
    Dropped,
    /// A request for a new window onto the app's own address: denied (a window
    /// is never created), told apart from a drop in the log.
    WindowDenied,
}

/// Applies [`decide`] to a navigation: `open` is called with the address
/// when it goes to the system browser and says whether the system took it.
pub fn handle(url: &Url, open: impl FnOnce(&str) -> Result<(), String>) -> Outcome {
    match decide(url) {
        Navigation::Stay => Outcome::Stayed,
        Navigation::OpenOutside => match open(url.as_str()) {
            Ok(()) => Outcome::Opened,
            Err(why) => Outcome::OpenRefused(why),
        },
        Navigation::Drop => Outcome::Dropped,
    }
}

/// Applies [`decide`] to a request for a new window: the window is never
/// created, so the app's own address is [`Outcome::WindowDenied`] (it would
/// stay in a navigation, which is not what a new window asks).
pub fn handle_new_window(url: &Url, open: impl FnOnce(&str) -> Result<(), String>) -> Outcome {
    match handle(url, open) {
        Outcome::Stayed => Outcome::WindowDenied,
        other => other,
    }
}

/// The log line for an outcome: a level and a message, distinct for a
/// designed drop or a denied window (warnings, with their own words) and for a
/// refusal by the system (an error). An address that stays is logged at
/// debug, below the plugin's `info` filter: the app's own pages are not news.
pub fn record(url: &Url, outcome: &Outcome) -> (log::Level, String) {
    match outcome {
        Outcome::Stayed => (log::Level::Debug, format!("navigation stays: {url}")),
        Outcome::Opened => (
            log::Level::Info,
            format!("opened in the system browser: {url}"),
        ),
        Outcome::OpenRefused(why) => (
            log::Level::Error,
            format!("the system refused to open {url}: {why}"),
        ),
        Outcome::Dropped => (
            log::Level::Warn,
            format!("dropped by the navigation rule: {url}"),
        ),
        Outcome::WindowDenied => (
            log::Level::Warn,
            format!("new window denied (the app opens none): {url}"),
        ),
    }
}

/// Handles a navigation, logs its outcome, and returns whether the webview
/// may navigate itself.
pub fn allow_navigation(url: &Url, open: impl FnOnce(&str) -> Result<(), String>) -> bool {
    let outcome = handle(url, open);
    log_outcome(url, &outcome);
    outcome == Outcome::Stayed
}

/// Handles a request for a new window, logs its outcome, and returns
/// `false`: a window is never created.
pub fn allow_new_window(url: &Url, open: impl FnOnce(&str) -> Result<(), String>) -> bool {
    log_outcome(url, &handle_new_window(url, open));
    false
}

fn log_outcome(url: &Url, outcome: &Outcome) {
    let (level, message) = record(url, outcome);
    log::log!(level, "{message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide_str(address: &str) -> Navigation {
        decide(&Url::parse(address).expect("a valid address"))
    }

    #[test]
    fn the_app_stays() {
        for a in [
            "tauri://localhost",
            "tauri://localhost/",
            "tauri://localhost/map",
            "tauri://localhost/map/sim?x=1#top",
        ] {
            assert_eq!(decide_str(a), Navigation::Stay, "{a}");
        }
    }

    #[test]
    fn https_and_mailto_go_to_the_system_browser() {
        for a in [
            "https://suttacentral.net/dn1",
            "https://palicanon.app/terms",
            "HTTPS://Example.org/x",
            "mailto:support@palicanon.app",
        ] {
            assert_eq!(decide_str(a), Navigation::OpenOutside, "{a}");
        }
    }

    #[test]
    fn everything_else_is_dropped() {
        for a in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>1</script>",
            "http://example.org/",
            "ftp://example.org/",
            "blob:tauri://localhost/uuid",
            "about:blank",
            "ipc://localhost/cmd",
            "tel:+33123456789",
            "tauri://evil",
            "tauri://localhost.evil.example/",
            "tauri://localhost:8080/",
            "tauri://user@localhost/",
            "http://tauri.localhost/",
            "http://localhost:1420/",
        ] {
            assert_eq!(decide_str(a), Navigation::Drop, "{a}");
        }
    }

    #[test]
    fn a_navigation_opens_outside_once_and_does_not_navigate() {
        let url = Url::parse("https://suttacentral.net/dn1").unwrap();
        let mut opened = Vec::new();
        assert!(!allow_navigation(&url, |u| {
            opened.push(u.to_string());
            Ok(())
        }));
        assert_eq!(opened, ["https://suttacentral.net/dn1"]);
    }

    #[test]
    fn the_app_navigation_opens_nothing() {
        let url = Url::parse("tauri://localhost/map").unwrap();
        assert!(allow_navigation(&url, |u| -> Result<(), String> {
            panic!("opened {u}")
        }));
    }

    #[test]
    fn a_dropped_navigation_opens_nothing_and_does_not_navigate() {
        let url = Url::parse("javascript:alert(1)").unwrap();
        assert!(!allow_navigation(&url, |u| -> Result<(), String> {
            panic!("opened {u}")
        }));
    }

    /// The system browser takes any address (a double with the `open` shape).
    #[allow(clippy::unnecessary_wraps)]
    fn accepted(_: &str) -> Result<(), String> {
        Ok(())
    }

    fn refused(_: &str) -> Result<(), String> {
        Err("no handler".to_string())
    }

    #[test]
    fn the_outcome_maps_each_decision() {
        let url = |a: &str| Url::parse(a).unwrap();
        assert_eq!(
            handle(&url("tauri://localhost/map"), accepted),
            Outcome::Stayed
        );
        assert_eq!(
            handle(&url("https://suttacentral.net/"), accepted),
            Outcome::Opened
        );
        assert_eq!(
            handle(&url("https://suttacentral.net/"), refused),
            Outcome::OpenRefused("no handler".to_string())
        );
        assert_eq!(
            handle(&url("javascript:alert(1)"), refused),
            Outcome::Dropped
        );
        // A refusal is not a drop, and a drop never asks the system.
        assert_ne!(
            handle(&url("https://a.example/"), refused),
            handle(&url("http://a.example/"), refused)
        );
    }

    #[test]
    fn a_new_window_for_the_apps_own_address_is_denied_not_dropped() {
        let url = Url::parse("tauri://localhost/map").unwrap();
        assert_eq!(handle_new_window(&url, accepted), Outcome::WindowDenied);
        let url = Url::parse("javascript:alert(1)").unwrap();
        assert_eq!(handle_new_window(&url, accepted), Outcome::Dropped);
        let url = Url::parse("tauri://localhost/map").unwrap();
        let url = Url::parse("https://a.example/").unwrap();
        assert_eq!(
            handle_new_window(&url, refused),
            Outcome::OpenRefused("no handler".into())
        );
    }

    #[test]
    fn a_refusal_is_logged_apart_from_a_designed_drop() {
        let url = Url::parse("https://a.example/").unwrap();
        let (drop_level, drop_text) = record(&url, &Outcome::Dropped);
        let (refused_level, refused_text) =
            record(&url, &Outcome::OpenRefused("no handler".into()));
        assert_eq!(drop_level, log::Level::Warn);
        assert_eq!(refused_level, log::Level::Error);
        assert!(
            drop_text.contains("dropped by the navigation rule"),
            "{drop_text}"
        );
        assert!(
            refused_text.contains("refused") && refused_text.contains("no handler"),
            "{refused_text}"
        );
        assert_ne!(drop_text, refused_text);
        // A denied window is a warning with its own words, not a "dropped".
        let (denied_level, denied_text) = record(&url, &Outcome::WindowDenied);
        assert_eq!(denied_level, log::Level::Warn);
        assert!(denied_text.contains("new window denied"), "{denied_text}");
        assert!(!denied_text.contains("dropped"), "{denied_text}");
        assert_eq!(record(&url, &Outcome::Opened).0, log::Level::Info);
        // The app's own pages are below the plugin's info filter (lib.rs): not logged.
        assert!(record(&url, &Outcome::Stayed).0 > log::Level::Info);
        assert_eq!(record(&url, &Outcome::Stayed).0, log::Level::Debug);
    }

    #[test]
    fn a_new_window_is_never_created() {
        for (a, opens) in [
            ("https://suttacentral.net/dn1", 1),
            ("mailto:a@b.example", 1),
            ("tauri://localhost/map", 0),
            ("javascript:alert(1)", 0),
            ("file:///etc/hosts", 0),
        ] {
            let mut opened = 0;
            assert!(
                !allow_new_window(&Url::parse(a).unwrap(), |_| {
                    opened += 1;
                    Ok(())
                }),
                "{a}"
            );
            assert_eq!(opened, opens, "{a}");
        }
    }
}
