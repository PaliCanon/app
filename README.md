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
| `crates/front` | The client-side Leptos front (`csr`) on ui-lib: the shell, the rail, the routes (`/` goes to `/map`), the bundled map data. Built to wasm by Trunk. `src/map_mount.rs` is the one place that mounts the map: vizualisation's fragment (tag `v0.2.0`) under `/map`, over the data files bundled in the app. |
| `crates/desktop` | The Tauri 2 shell: the window, the CSP, the one navigation rule (`tauri://localhost` stays, `https:` and `mailto:` open in the system browser, everything else is dropped), single instance. The progress writer (`src/writer.rs`: one mutex, one slot, `progress.json` in the app data directory, `progress.lock` held for the session, the exit write on `RunEvent::Exit`) and the commands the webview may call (`src/commands.rs`). |
| `crates/xtask` | Native build tasks, never shipped: `cargo xtask pins` (ui-lib is built once, from git, with `csr` and without `ssr` or `hydrate`; the checkout paths), `css` (Tailwind), `data` (the map data files). |

## Build and open the app

```sh
export RUSTUP_HOME=$HOME/.rustup-pc/rustup CARGO_HOME=$HOME/.rustup-pc/cargo PATH=$HOME/.rustup-pc/cargo/bin:$PATH RUSTC_WRAPPER=sccache
cargo install trunk tauri-cli --locked          # once
cd crates/desktop
cargo tauri build --bundles app                 # runs the Trunk build of the front first
open ../../target/release/bundle/macos/PaliCanon.app
```

The Tailwind CLI is `$TAILWIND`, else the one `cargo-leptos` cached. The app
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

MSRV 1.88 (`rust-version` in `Cargo.toml`).

## Checks

The commands CI runs (`.github/workflows/ci.yml`), from the repository root:

```sh
cargo fmt --check
leptosfmt --check crates
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked -p palicanon-app-front --lib --bins --target wasm32-unknown-unknown -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo +1.88 check --locked                      # MSRV
cargo deny check advisories bans sources licenses
(cd crates/front && trunk build --release --cargo-profile wasm-release)
cargo llvm-cov --locked --lcov --output-path lcov.info   # then Codecov
```

Coverage uploads to Codecov with the `CODECOV_TOKEN` secret. Jobs that fetch
ui-lib use the `private-deps` action (the GitHub App `palicanon-ci`).

## Licence

CC-BY-NC-4.0 (decision 2026-10-11), see `LICENSE`.
