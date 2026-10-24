//! External links (task 009): a page's `https:` or `mailto:` link is opened
//! by the desktop in the system browser through `open_outside`, under the
//! navigation rule. The webview's own `target="_blank"` did not reach the
//! rule's hook, so a click listener on the document takes every anchor
//! click: an external address is sent to the desktop and the click is
//! cancelled; the app's own links are left to the router.

/// The address an anchor click sends outside, if its `href` is external.
#[must_use]
pub fn external(href: &str) -> Option<String> {
    let lower = href.trim().to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("mailto:")).then(|| href.trim().to_string())
}

/// Installs the document's click listener (the browser only).
#[cfg(target_arch = "wasm32")]
pub fn install(open: impl Fn(String) + 'static) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let listener =
        Closure::<dyn Fn(web_sys::MouseEvent)>::new(move |event: web_sys::MouseEvent| {
            let Some(target) = event
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            else {
                return;
            };
            let Ok(Some(anchor)) = target.closest("a[href]") else {
                return;
            };
            let Some(href) = anchor.get_attribute("href") else {
                return;
            };
            if let Some(address) = external(&href) {
                event.prevent_default();
                open(address);
            }
        });
    let _ = document.add_event_listener_with_callback("click", listener.as_ref().unchecked_ref());
    listener.forget();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_and_mailto_go_outside_the_rest_stays() {
        assert_eq!(
            external("https://suttacentral.net/mn10").as_deref(),
            Some("https://suttacentral.net/mn10")
        );
        assert_eq!(
            external(" mailto:hi@example.org ").as_deref(),
            Some("mailto:hi@example.org")
        );
        assert_eq!(
            external("HTTPS://Example.org/A").as_deref(),
            Some("HTTPS://Example.org/A")
        );
        for inside in [
            "/learn",
            "/search?q=x",
            "#top",
            "http://example.org",
            "javascript:void(0)",
            "",
        ] {
            assert_eq!(external(inside), None, "{inside}");
        }
    }
}
