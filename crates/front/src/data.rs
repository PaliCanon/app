//! The map's data, bundled in the app: [`BundledData`] answers the map's six
//! file requests from `/map-data/<name>`, which is `tauri://localhost/map-data/…`
//! in the app (the build task `xtask data` copies the files there). Nothing
//! comes from the network: the window's CSP allows only `'self'`.
//!
//! [`BundledData`] is the app's [`MapData`], the map's own trait (a file name
//! in, the bytes or `None` out, a `Send` future).

use std::future::Future;
use std::pin::Pin;

use js_sys::Uint8Array;
use send_wrapper::SendWrapper;
use vizualisation::fragment::{MapData, MapFile};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

/// Where the build task puts the files, under the app's origin.
const DIRECTORY: &str = "/map-data";

/// The map's six data files, by name: the only names [`BundledData`] answers.
pub const FILES: [&str; 6] = [
    "graph.json",
    "graph-sim.json",
    "graph-sim.bin",
    "translations.json",
    "blurbs-en.json",
    "blurbs-fr.json",
];

/// Fetches an address: the browser's `fetch`, or a double in a test. Its
/// future is not `Send` (a browser future never is); [`BundledData`] wraps it.
pub trait Fetch: Send + Sync + 'static {
    /// The body of `url` on a 200, `None` on any other answer or failure.
    fn get(&self, url: String) -> Pin<Box<dyn Future<Output = Option<Vec<u8>>>>>;
}

/// The address of the bundled file `name`, or `None` for a name that is not
/// one of the six (no request is made for it).
pub fn url_for(name: &str) -> Option<String> {
    FILES.contains(&name).then(|| format!("{DIRECTORY}/{name}"))
}

/// Whether a response is a data file: a 200 with a content type that is
/// present, not empty and not the HTML page (a missing or empty content type
/// is not data).
/// Tauri answers an address it has no file for with `index.html` and a 200
/// (seen in the app: a missing `/map-data/…` came back as `text/html`), and
/// none of the six files is HTML.
pub fn is_data(status: u16, content_type: Option<&str>) -> bool {
    let is_html = |t: &str| t.trim_start().to_ascii_lowercase().starts_with("text/html");
    status == 200 && content_type.is_some_and(|t| !t.trim().is_empty() && !is_html(t))
}

/// The map's data, read from the files bundled in the app.
#[derive(Debug, Default, Clone, Copy)]
pub struct BundledData<F = BrowserFetch> {
    fetch: F,
}

impl<F: Fetch> BundledData<F> {
    /// Bundled data read through `fetch`.
    pub fn with_fetch(fetch: F) -> Self {
        Self { fetch }
    }
}

impl<F: Fetch> MapData for BundledData<F> {
    fn file(&self, name: &'static str) -> MapFile {
        let Some(url) = url_for(name) else {
            return Box::pin(std::future::ready(None));
        };
        // A browser future is not `Send`; the webview is one thread, so the
        // wrapper is never polled elsewhere.
        let request = SendWrapper::new(self.fetch.get(url));
        Box::pin(request)
    }
}

/// `fetch` in the browser (the webview): a 200 that [`is_data`].
#[derive(Debug, Default, Clone, Copy)]
pub struct BrowserFetch;

impl Fetch for BrowserFetch {
    fn get(&self, url: String) -> Pin<Box<dyn Future<Output = Option<Vec<u8>>>>> {
        Box::pin(async move {
            let response = JsFuture::from(web_sys::window()?.fetch_with_str(&url))
                .await
                .ok()?
                .dyn_into::<Response>()
                .ok()?;
            let content_type = response.headers().get("content-type").ok().flatten();
            if !is_data(response.status(), content_type.as_deref()) {
                return None;
            }
            let body = JsFuture::from(response.array_buffer().ok()?).await.ok()?;
            Some(Uint8Array::new(&body).to_vec())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;

    use super::*;

    /// A double that records the addresses asked and answers from a table.
    #[derive(Clone, Default)]
    struct Recorder {
        asked: Arc<Mutex<Vec<String>>>,
        answer: Option<Vec<u8>>,
    }

    impl Fetch for Recorder {
        fn get(&self, url: String) -> Pin<Box<dyn Future<Output = Option<Vec<u8>>>>> {
            self.asked.lock().unwrap().push(url);
            let answer = self.answer.clone();
            Box::pin(std::future::ready(answer))
        }
    }

    /// Polls a future that is ready at once (the doubles never wait).
    fn ready<T>(future: impl Future<Output = T>) -> T {
        let mut future = Box::pin(future);
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("not ready"),
        }
    }

    #[test]
    fn the_files_are_the_designs_six_names() {
        assert_eq!(
            FILES,
            [
                "graph.json",
                "graph-sim.json",
                "graph-sim.bin",
                "translations.json",
                "blurbs-en.json",
                "blurbs-fr.json"
            ]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_files_are_the_build_tasks_files() {
        assert_eq!(FILES, xtask::data::FILES);
    }

    #[test]
    fn the_six_names_map_to_the_bundled_directory() {
        for name in FILES {
            assert_eq!(url_for(name), Some(format!("/map-data/{name}")));
        }
    }

    #[test]
    fn any_other_name_has_no_address() {
        for name in [
            "",
            "graph",
            "../graph.json",
            "graph.json/",
            "/graph.json",
            "GRAPH.JSON",
            "https://example.org/graph.json",
            "graph.json?x=1",
        ] {
            assert_eq!(url_for(name), None, "{name:?}");
        }
    }

    #[test]
    fn a_known_file_is_fetched_once_from_its_address() {
        let double = Recorder {
            answer: Some(vec![1, 2, 3]),
            ..Recorder::default()
        };
        let data = BundledData::with_fetch(double.clone());
        assert_eq!(ready(data.file("graph-sim.bin")), Some(vec![1, 2, 3]));
        assert_eq!(*double.asked.lock().unwrap(), ["/map-data/graph-sim.bin"]);
    }

    #[test]
    fn only_a_200_that_is_not_html_is_data() {
        for (status, content_type, data) in [
            (200, Some("application/json"), true),
            (200, Some("application/octet-stream"), true),
            (200, None, false),
            (200, Some(""), false),
            (200, Some("   "), false),
            (200, Some("text/html"), false),
            (200, Some("Text/HTML; charset=utf-8"), false),
            (404, Some("application/json"), false),
            (500, None, false),
            (304, None, false),
        ] {
            assert_eq!(
                is_data(status, content_type),
                data,
                "{status} {content_type:?}"
            );
        }
    }

    #[test]
    fn a_failed_fetch_is_none() {
        let double = Recorder::default();
        let data = BundledData::with_fetch(double.clone());
        assert_eq!(ready(data.file("graph.json")), None);
        assert_eq!(double.asked.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_unknown_name_is_none_without_a_fetch() {
        let double = Recorder {
            answer: Some(vec![9]),
            ..Recorder::default()
        };
        let data = BundledData::with_fetch(double.clone());
        assert_eq!(ready(data.file("secrets.json")), None);
        assert_eq!(double.asked.lock().unwrap().len(), 0);
    }

    #[test]
    fn the_data_is_what_the_map_needs_it_to_be() {
        fn needs<T: MapData>() {}
        fn send<T: Send>(_: &T) {}
        needs::<BundledData>();
        let data = BundledData::<BrowserFetch>::default();
        send(&data.file("graph.json"));
    }
}
