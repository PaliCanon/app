//! The one navigation rule, applied to every navigation and every new
//! window of the webview: the app stays on `tauri://localhost`, an `https:`
//! or `mailto:` link opens in the system browser, and everything else is
//! dropped. No new window is ever created.

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

/// Applies [`decide`] for a navigation: `open` is called with the address
/// when it goes to the system browser. Returns whether the webview may
/// navigate itself.
pub fn allow_navigation(url: &Url, open: impl FnOnce(&str)) -> bool {
    match decide(url) {
        Navigation::Stay => true,
        Navigation::OpenOutside => {
            open(url.as_str());
            false
        }
        Navigation::Drop => false,
    }
}

/// Applies [`decide`] for a request to open a new window: the window is
/// never created (`false` always); an address bound for the system browser
/// is opened through `open`.
pub fn allow_new_window(url: &Url, open: impl FnOnce(&str)) -> bool {
    if decide(url) == Navigation::OpenOutside {
        open(url.as_str());
    }
    false
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
        assert!(!allow_navigation(&url, |u| opened.push(u.to_string())));
        assert_eq!(opened, ["https://suttacentral.net/dn1"]);
    }

    #[test]
    fn the_app_navigation_opens_nothing() {
        let url = Url::parse("tauri://localhost/map").unwrap();
        assert!(allow_navigation(&url, |u| panic!("opened {u}")));
    }

    #[test]
    fn a_dropped_navigation_opens_nothing_and_does_not_navigate() {
        let url = Url::parse("javascript:alert(1)").unwrap();
        assert!(!allow_navigation(&url, |u| panic!("opened {u}")));
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
                !allow_new_window(&Url::parse(a).unwrap(), |_| opened += 1),
                "{a}"
            );
            assert_eq!(opened, opens, "{a}");
        }
    }
}
