//! The sign-in link (task 006; user, 2026-10-16: «afficher le lien pas
//! juste ouvrir le navigateur»): while the shell waits for Google's answer,
//! a ui-lib [`Dialog`] shows Google's sign-in address, selectable, with
//! Copy, Open in browser and Cancel. The shell opened it once already; the
//! dialog is for a browser that did not come forward, or another browser.
//!
//! - **Open in browser** is a real link: the app's navigation rule hands an
//!   `https:` address to the system browser from Rust (the opener), the
//!   webview never leaves the app.
//! - **Copy** writes the address to the clipboard (`navigator.clipboard`),
//!   and the button says Copied.
//! - **Cancel**, the dialog's close button, Escape and a backdrop click all
//!   cancel the sign-in ([`AppSync::cancel`]): the shell's listener is
//!   closed.
//!
//! Below `sm` the dialog is a full-screen sheet (ui-lib's); the address
//! wraps anywhere. The dialog needs a browser (its focus trap); its body
//! and its open state are tested natively.

use leptos::prelude::*;
use palicanon_ui_lib::{Button, Dialog, Disclosure, LinkButton, Variant, language_signal};

use crate::strings;
use crate::sync::AppSync;

/// The dialog's id.
const DIALOG_ID: &str = "pc-sync-link";

/// The dialog's state: open while a link is shown; closing it cancels.
#[derive(Clone, Copy)]
struct LinkShown {
    link: Signal<Option<String>>,
    sync: StoredValue<AppSync, LocalStorage>,
}

impl Disclosure for LinkShown {
    fn id(&self) -> &'static str {
        DIALOG_ID
    }

    fn is_open(&self) -> bool {
        self.link.with(Option::is_some)
    }

    fn set_open(&self, open: bool) {
        if !open {
            self.sync.with_value(AppSync::cancel);
        }
    }
}

/// The sign-in link's dialog, over the app's sync.
#[component]
pub fn SignInLink(
    /// The app's sync transport.
    sync: AppSync,
) -> impl IntoView {
    let language = language_signal();
    let link = sync.link();
    let shown = LinkShown {
        link,
        sync: StoredValue::new_local(sync),
    };
    let title = Signal::derive(move || strings::of(language.get()).sync_link_title.to_string());
    view! {
        <Dialog controls=shown title=title layout="sm:max-w-[32rem]">
            <LinkBody shown=shown />
        </Dialog>
    }
}

/// The dialog's body: what the link is for, the address (selectable), then
/// the actions. Apart from the dialog so it renders without a browser.
#[component]
fn LinkBody(shown: LinkShown) -> impl IntoView {
    let language = language_signal();
    let link = shown.link;
    let copied = RwSignal::new(false);
    view! {
        <p class="text-sm text-muted">{move || strings::of(language.get()).sync_link_explain}</p>
        <p
            data-sync-link=""
            class="mt-3 select-all break-all rounded-lg border border-line px-3 py-2 font-mono text-xs text-heading"
        >
            {move || link.get().unwrap_or_default()}
        </p>
        <LinkActions link=link copied=copied shown=shown />
    }
}

/// Copy, Open in browser, Cancel.
#[component]
fn LinkActions(
    link: Signal<Option<String>>,
    copied: RwSignal<bool>,
    shown: LinkShown,
) -> impl IntoView {
    let language = language_signal();
    let t = move || strings::of(language.get());
    let copy = move |_| {
        if let Some(address) = link.get_untracked() {
            copy_to_clipboard(&address);
            copied.set(true);
        }
    };
    view! {
        <div class="mt-4 flex flex-wrap gap-3">
            {move || {
                link.get()
                    .map(|address| {
                        view! {
                            <LinkButton href=address variant=Variant::Accent>
                                {move || t().sync_link_open}
                            </LinkButton>
                        }
                    })
            }} <Button variant=Variant::Chip on_click=copy>
                {move || if copied.get() { t().sync_link_copied } else { t().sync_link_copy }}
            </Button> <Button variant=Variant::Ghost on_click=move |_| shown.set_open(false)>
                {move || t().sync_link_cancel}
            </Button>
        </div>
    }
}

/// Writes `text` to the clipboard (`navigator.clipboard.writeText`), when
/// the webview has one; the address stays selectable otherwise.
#[cfg(target_arch = "wasm32")]
fn copy_to_clipboard(text: &str) {
    use wasm_bindgen::JsCast;
    let get = |target: &wasm_bindgen::JsValue, name: &str| {
        js_sys::Reflect::get(target, &name.into())
            .ok()
            .filter(|value| !value.is_undefined() && !value.is_null())
    };
    let clipboard = get(&js_sys::global(), "navigator").and_then(|n| get(&n, "clipboard"));
    let write = clipboard
        .as_ref()
        .and_then(|clipboard| get(clipboard, "writeText"))
        .and_then(|write| write.dyn_into::<js_sys::Function>().ok());
    if let (Some(clipboard), Some(write)) = (clipboard, write) {
        let _ = write.call1(&clipboard, &text.into());
    } else {
        web_sys::console::warn_1(&"sync: no clipboard in this webview".into());
    }
}

/// Natively (the tests) there is no clipboard.
#[cfg(not(target_arch = "wasm32"))]
fn copy_to_clipboard(_text: &str) {}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use learn_pali::fragment::SyncTransport;
    use leptos::tachys::view::RenderHtml;
    use palicanon_app_bridge::{PullAnswer, PushAnswer, SignInStart, SyncAccountAnswer};
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::sync::SyncPort;

    const ADDRESS: &str = "https://accounts.google.com/o/oauth2/v2/auth?client_id=x&state=s";

    /// A shell whose sign-in starts at once, counting its cancels.
    #[derive(Default)]
    struct Shell {
        cancels: Cell<usize>,
    }

    impl SyncPort for Shell {
        fn sign_in(&self, done: Box<dyn FnOnce(SignInStart)>) {
            done(SignInStart::Started {
                attempt: 1,
                url: ADDRESS.into(),
            });
        }
        fn cancel(&self) {
            self.cancels.set(self.cancels.get() + 1);
        }
        fn pull(&self, _: Box<dyn FnOnce(PullAnswer)>) {}
        fn push(&self, _: String, _: Box<dyn FnOnce(PushAnswer)>) {}
        fn account(&self, _: Box<dyn FnOnce(SyncAccountAnswer)>) {}
        fn sign_out(&self) {}
    }

    /// The dialog's markup, a sign-in waiting or not; the shell.
    fn render(waiting: bool) -> (String, Rc<Shell>) {
        let owner = Owner::new();
        owner.set();
        provide_context(RwSignal::new(Language::En));
        let shell = Rc::new(Shell::default());
        let sync = AppSync::new(SyncAccountAnswer::SignedOut, shell.clone()).expect("a sync");
        if waiting {
            sync.sign_in(Box::new(|_| {}));
        }
        let shown = LinkShown {
            link: sync.link(),
            sync: StoredValue::new_local(sync),
        };
        let html = view! { <LinkBody shown=shown /> }.to_html();
        (html, shell)
    }

    #[test]
    fn a_waiting_sign_in_shows_its_address_with_copy_open_and_cancel() {
        let (html, _) = render(true);
        let shown = ADDRESS.replace('&', "&amp;");
        assert!(html.contains("data-sync-link"), "{html}");
        assert!(
            html.contains(&format!(">{shown}<")),
            "the address as text: {html}"
        );
        assert!(
            html.contains(&format!(r#"href="{shown}""#)),
            "a real link: {html}"
        );
        assert!(html.contains("select-all"), "selectable: {html}");
        for label in [
            "PaliCanon waits here",
            "Open in browser",
            "Copy link",
            "Cancel",
        ] {
            assert!(html.contains(label), "{label}: {html}");
        }
        assert!(!html.contains('\u{2014}'), "no em dash: {html}");
    }

    #[test]
    fn with_no_sign_in_waiting_the_body_holds_no_address() {
        let (html, _) = render(false);
        assert!(!html.contains("accounts.google.com"), "{html}");
        assert!(!html.contains("Open in browser"), "{html}");
    }

    #[test]
    fn closing_the_dialog_cancels_the_sign_in() {
        let owner = Owner::new();
        owner.set();
        let shell = Rc::new(Shell::default());
        let sync = AppSync::new(SyncAccountAnswer::SignedOut, shell.clone()).expect("a sync");
        let closed = LinkShown {
            link: sync.link(),
            sync: StoredValue::new_local(sync.clone()),
        };
        assert!(!closed.is_open(), "no sign-in waiting: closed");
        let into = Rc::new(Cell::new(false));
        let cancelled = into.clone();
        sync.sign_in(Box::new(move |outcome| cancelled.set(outcome.is_err())));
        let shown = LinkShown {
            link: sync.link(),
            sync: StoredValue::new_local(sync.clone()),
        };
        assert!(shown.is_open());
        shown.set_open(true);
        assert_eq!(shell.cancels.get(), 0, "opening cancels nothing");
        shown.set_open(false);
        assert_eq!(shell.cancels.get(), 1);
        assert!(!shown.is_open());
        assert!(into.get(), "the sign-in answered cancelled");
    }
}
