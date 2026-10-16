//! The browser entry: Trunk builds this binary to wasm and runs it. `<body>`
//! is empty; ui-lib's `csr::mount` mounts the app in it, with the theme and
//! the language kept in `localStorage`.

fn main() {
    console_error_panic_hook::set_once();
    palicanon_ui_lib::csr::mount(palicanon_app_front::app::App);
}
