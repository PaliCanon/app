# PaliCanon app

The PaliCanon app, desktop and mobile, offline: a single app holding Learn
and Search (the map later), built with Tauri 2 around the Leptos code of the
services run client-side. Display name "PaliCanon", bundle identifier
`app.palicanon.learn` (fixed forever: every platform keys the user's data by
it). Private until opened (the repository is for open source later).

## Holds, never holds

- Holds: the Tauri 2 shell, the client crates wired into it, the app's fixed
  identity (`crates/identity`, `palicanon_app_identity`), its CI, its signing
  and store pipelines.
- Never holds: a service's server code (it stays in `learn-pali`,
  `search-front`), yuga's sources (only a public yuga binary, later), secrets,
  user data.

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
- T5a — desktop shell and Learn (the Tauri shell; macOS build joins CI).
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

The exact commands CI runs (`.github/workflows/ci.yml`), from the repository
root:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo +1.88 check --locked                      # MSRV
cargo deny check advisories bans sources licenses
cargo llvm-cov --locked --lcov --output-path lcov.info   # then Codecov
```

Coverage uploads to Codecov with the `CODECOV_TOKEN` secret.

## Licence

CC-BY-NC-4.0 (decision 2026-10-11), see `LICENSE`.
