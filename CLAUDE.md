# CLAUDE.md — app

@../docs/README.md
@../docs/conventions/index.md
@../claude-memory/MEMORY.md

This repository: the PaliCanon app, desktop and mobile, offline — Tauri 2
around the client-side Leptos code of the services (Learn from
`../learn-pali`, Search from `../search-front`). It holds no service's server
code, no yuga sources, no secret and no user data. Its scope is defined task
by task in `.task/` before anything is built. Build with the isolated
toolchain (`~/.rustup-pc`, see README.md). The bundle identifier
`app.palicanon.learn` is fixed forever, because it keys the user's data on
every platform: it lives in `crates/identity` and is never edited, renamed or
restated elsewhere.
