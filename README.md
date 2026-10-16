# PaliCanon app

The PaliCanon app, desktop and mobile, offline: a single app holding the Map
(first), then Learn and Search, built with Tauri 2 around the Leptos code of
the services run client-side. Display name "PaliCanon", bundle identifier
`app.palicanon.learn` (fixed forever: every platform keys the user's data by
it). Private until opened (the repository is for open source later).

## Holds, never holds

- Holds: the Tauri 2 shell, the client crates wired into it, the app's fixed
  identity (`crates/identity`, `palicanon_app_identity`), its CI, its signing
  and store pipelines.
- Never holds: a service's server code (it stays in `learn-pali`,
  `search-front`), yuga's sources (only a public yuga binary, later), secrets,
  user data.

## Crates

| Crate | What |
| --- | --- |
| `crates/identity` | The fixed bundle identifier and display name. |
| `crates/bridge` | The wire between the front and the shell, serde only, built native and wasm: the six commands' names, their answers, the `progress-status` event, the 4 MiB cap. |
| `crates/front` | The client-side Leptos front (`csr`) on ui-lib: the progress loaded first (`progress_load`), then Learn's progress context above one router in the shell; the rail lists Learn, Search and the Map, `/` goes to `/learn`; the bundled map data. Built to wasm by Trunk. One file per mounted fragment: `src/learn_mount.rs` (learn-pali `fragment-v0.1.0` under `/learn`; the store `src/progress.rs` mirrors the native writer, `src/files.rs` the native export and import), `src/search_mount.rs` (search-front `fragment-v0.1.0` under `/search`, over `src/search.rs`, the local engine through the shell), `src/map_mount.rs` (vizualisation `v0.2.0` under `/map`). `src/tauri.rs` is the webview's side of the bridge. |
| `crates/desktop` | The Tauri 2 shell: the window, the CSP, the one navigation rule (`tauri://localhost` stays, `https:` and `mailto:` open in the system browser, everything else is dropped), single instance. The CSP's `connect-src` holds `'self'` and Tauri's IPC (`ipc: http://ipc.localhost`); the webview's one capability (`capabilities/default.json`) grants the six commands and `core:event:allow-listen`, nothing else. The progress writer (`src/writer.rs`: one mutex, one slot, `progress.json` in the app data directory, `progress.lock` held for the session, the exit write on `RunEvent::Exit`) the local search engine (`src/engine.rs`: `yuga-sidecar` started at the first search, kept with its model in memory until the app quits, one request at a time, respawned once, killed on exit) and the commands the webview may call (`src/commands.rs`). |
| `crates/xtask` | Native build tasks, never shipped: `cargo xtask pins` (ui-lib is built once, from git, with `csr` and without `ssr` or `hydrate`; `learn_core` and `learn_pali` come from one revision; the checkout paths), `css` (Tailwind, over ui-lib's, the fragments' and the front's sources), `data` (the map data files), `assets` (`/lotus.png` from Learn's checkout, `/brand/*.svg` from Search's), `yuga` (the sidecar, the store and the model, checked and staged in `target/yuga` for the bundle). |

## Build and open the app

```sh
export RUSTUP_HOME=$HOME/.rustup-pc/rustup CARGO_HOME=$HOME/.rustup-pc/cargo PATH=$HOME/.rustup-pc/cargo/bin:$PATH RUSTC_WRAPPER=sccache
cargo install trunk tauri-cli --locked          # once
cd crates/desktop
cargo tauri build --bundles app --config tauri.bundle.conf.json
# runs `cargo xtask yuga` (the sidecar, the store, the model: fetched and
# checked against their SHA-256) and the Trunk build of the front first
open ../../target/release/bundle/macos/PaliCanon.app
```

The Tailwind CLI is `$TAILWIND`, else the one `cargo-leptos` cached. `xtask
yuga` downloads yuga-sidecar (public release), the model (Hugging Face) and the
English store (`gsutil`, the builder's Google credentials); `XTASK_YUGA_CACHE`
names a directory that may already hold them, used when their digest is the
pinned one. The app
is unsigned (signing is task T7): macOS asks to confirm the first opening.
ui-lib is a private git dependency by tag (`ssh://git@github.com/PaliCanon/ui-lib.git`):
cargo needs the ssh key of an account that reads it.

## Decisions

Both in `../docs/decisions.md`:

- 2026-10-15 — One PaliCanon app, desktop and mobile, offline: one app,
  Tauri 2, in its own repository under PaliCanon, CC-BY-NC-4.0. Proven by the
  learn-pali spikes 044 (Learn in a Tauri window, progress kept) and 045
  (yuga's local search on the production store).
- 2026-10-15 — Local search: hybrid on desktop, keywords on mobile; yuga as a
  public binary (a sidecar on desktop, a prebuilt library on mobile).

## Tasks

Request `app-001-palicanon-app`, design v4:

- T1 — this init: the repository, its CI, the identity crate.
- T2 — ui-lib `csr`: the shared UI runs client-side.
- T3 — learn-pali client crate: Learn without its server.
- T8 — search-front client crate: Search without its server.
- T4 — yuga public binary and its licence.
- T5a — desktop shell with the Map first (task 003; Learn and Search follow in
  task 005; the macOS build is in CI).
- T5b — desktop Search.
- U1 — the user's Apple and Google accounts.
- T6 — mobile (iOS build joins CI).
- T7 — signing and the stores.

## Toolchain

The isolated toolchain, apart from the system Rust:

```sh
export RUSTUP_HOME=$HOME/.rustup-pc/rustup CARGO_HOME=$HOME/.rustup-pc/cargo PATH=$HOME/.rustup-pc/cargo/bin:$PATH RUSTC_WRAPPER=sccache
```

MSRV 1.91 (`rust-version` in `Cargo.toml`; search-front's fragment tag requires it).

## Checks

The commands CI runs (`.github/workflows/ci.yml`), from the repository root:

```sh
cargo fmt --check
leptosfmt --check crates
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked -p palicanon-app-front --lib --bins --target wasm32-unknown-unknown -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo +1.91 check --locked                      # MSRV
cargo deny check advisories bans sources licenses
(cd crates/front && trunk build --release --cargo-profile wasm-release)
cargo llvm-cov --locked --lcov --output-path lcov.info   # then Codecov
```

Coverage uploads to Codecov with the `CODECOV_TOKEN` secret. Jobs that fetch
ui-lib use the `private-deps` action (the GitHub App `palicanon-ci`).

## Licence

CC-BY-NC-4.0 (decision 2026-10-11), see `LICENSE`.
