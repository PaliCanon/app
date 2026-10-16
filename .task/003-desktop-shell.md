# 003 The desktop shell with the Map first

status: design · rank: 7 · level: hard · type: feature · request: app-001-palicanon-app T5a (user, 2026-10-16: "build l'app que je puisse checker"; "Oui, construis 048 + 021") · session: app-003-desktop-shell · after: [vizualisation 012 (its first tag)]

## Summary
Rank (rank.md): difficulty 7 (a new Tauri app around two fragments from two repositories, a persistent store with a write queue and failure states, native commands with atomic writes), impact 7 (the first build of the PaliCanon app the user checks and later ships) → 7.
What: the PaliCanon desktop app, macOS first: a client-side Leptos front (Learn under /learn, Search under /search, one ui-lib rail with the three services, the Map later), a Tauri shell, the progress file through native commands following learn-pali 048's app-store contract, export and import through native dialogs, external links in the system browser.
Requirements from the user's checks of the preview (2026-10-16): one rail, no double menu; Search on the new interface; the Map runs only on its tab (T9). Local search with yuga and bge-m3 int8 is T5b (after the yuga binary release, T4): this task ships a placeholder backend.

## Workflow
```mermaid
flowchart LR
  Define:::doing --> Plan:::todo --> Build:::todo --> Verify:::todo --> Review:::todo --> Check:::todo --> Merge:::todo --> Retro:::todo
  classDef doing fill:#f9a825,color:#000
  classDef done fill:#2e7d32,color:#fff
  classDef todo fill:#9e9e9e,color:#fff
```

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | doing | orchestrator, 2026-10-16: session app-003-desktop-shell (design d185d447c3f9e4e v1, discussion d43790a2626d44b, process d8b678355433443); worktree /Users/lsh/projects/palicanon/wt/app003/app (feat/desktop-shell) |
| Define (consensus) | done | consensus agent (Opus), 3 rounds, v4: the latest progress kept in Rust (one mutex, a slot, RunEvent::Exit writes it), native export/import commands (no fs plugin, no webview permissions), a 4 MiB cap, one navigation rule (https/mailto out, nothing else), CSP with style-src 'unsafe-inline' and wasm-unsafe-eval (macOS 13+), single instance + progress.lock, crates/bridge for the messages, xtask pins (cargo metadata) for lockstep and CSS/assets paths; no Map in the rail until T9. User decision relayed: «Attendre yuga» — no user check at this task's end (the orchestrator checks the .app: no CSP error, an answer survives Cmd-Q, links open outside); the user's first check after T5b |
| Scope | changed | user, 2026-10-16: «commence par intégrer la carte» — v5: the shell mounts the Map first (vizualisation 012's contract, relayed 2026-10-16); Learn, Search and the progress writer (v4) move to 005; the user checks the Map at this task's end |

## Design
Viewer: app-003-desktop-shell/design (d185d447c3f9e4e), v5 — the Map first (user, 2026-10-16); v4 (Learn and Search) moves to task 005; awaiting the user's validation.
```mermaid
flowchart TB
  subgraph t5a["PaliCanon/app 003 — the desktop shell with the Map first (user, 2026-10-16: «commence par intégrer la carte»), macOS first — v5"]
    crates["workspace: crates/identity (exists); crates/front (csr, built to wasm by Trunk); crates/desktop (Tauri 2); crates/xtask (native, never shipped). front depends on vizualisation's crate by git TAG (012's first tag, features = [#quot;csr#quot;], default-features = false) and palicanon-ui-lib v0.3.0 (csr); no [patch]. Learn and Search, the progress file, its writer and native export/import (v4's nodes) move to a later task, 005, when 048/021 are merged and tagged"]
    pins["xtask pins: ONE reading of `cargo metadata --locked --filter-platform wasm32-unknown-unknown`: (1) the lockstep check: exactly one palicanon-ui-lib source and rev, features include csr and exclude ssr and hydrate; (2) the resolved checkout dirs of palicanon-ui-lib and the map crate, for the build. Never a sibling path"]
    build["build: Trunk builds crates/front; its pre_build hook runs `xtask css` (Tailwind input: @import ui-lib's style/theme.css, @source ui-lib/src, the map crate's app/src and front/src, all from pins; a missing @source path fails; the built CSS is checked to hold one map-only class) and `xtask data` (copies the six map data files from the map crate's public/ at the pinned tag into the dist under /map-data/: graph.json, graph-sim.json, graph-sim.bin, translations.json, blurbs-en.json, blurbs-fr.json, ~4.7 MB; a missing one fails). tauri.conf.json: frontendDist = the Trunk dist, beforeBuildCommand = the Trunk release build; Tauri embeds the dist and serves it at tauri://localhost/"]
    front["front: csr::mount(App); App = one Router; inside ui-lib's AppShell: the rail lists the Map (/map) alone for now (Learn and Search join with 005: nothing unreachable in the rail); theme and language in the Local Store; Routes: / redirects to /map; ParentRoute path!(#quot;/map#quot;) → MapProvider(data = BundledData, head = None) + MapRoutes (012: children '', ':uid', 'sim', 'sim/:uid'); a Not found. Footer: ui-lib's, the app's own links to the system browser; window title 'PaliCanon'"]
    data["BundledData: the app's impl of 012's MapData (Send + Sync + 'static): file(name) fetches tauri://localhost/map-data/<name> (a JsFuture wrapped in SendWrapper inside the impl), Some(bytes) on 200, None otherwise; the names are 012's six, any other name is None without a fetch; nothing from the network"]
    life["the Map's life (012 + user rule «je préfère que ça consomme pas de ressources»): leaving /map unmounts it: data loads aborted, the frame loop stopped, listeners removed, the GPU device and context destroyed; coming back re-reads and re-parses the data (~1 s)"]
    shell["Tauri: productName PaliCanon, identifier app.palicanon.learn (fixed forever), window 1100×800 min 360×500, macOS 13+ (wasm-unsafe-eval); CSP: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; one navigation rule (on_navigation + on_new_window): stay on tauri://localhost, open https: and mailto: in the system browser (opener, called from Rust only), drop everything else, no new windows; single-instance plugin; .app bundle unsigned (signing T7); no command exposed to the webview in this task"]
    tests["tests: front native route tests (/ → /map, /map and /map/sim render the map route, Not found); BundledData: the six names map to /map-data/<name>, an unknown name is None with no fetch; the lockstep test; the navigation rule as a pure function (localhost stays, https and mailto out, javascript:, file:, data: dropped); xtask css and data fail on a missing path or file; one browser test (csr::mount, the rail, /map mounts, a sibling route unmounts it with no panic); CI: fmt, clippy, tests, the front's wasm build, deny (Actions blocked: the local list is the gate)"]
    check["checks: the orchestrator first (the .app opens, the map draws, the console holds no CSP error, leaving the tab drops the GPU use and memory, a SuttaCentral link opens in the browser, Cmd-Q quits cleanly), then the USER checks the Map in the app"]
  end
  viz["vizualisation 012 (T9): MapRoutes, MapProvider, MapData, its first tag; build.rs skips the placeholder check when ../ui-lib is absent (until ui-lib 068)"]
  ui["ui-lib v0.3.0"]
  later["later: 005 Learn and Search mounted (048, 021 + 049, 022 tags; v4's progress writer and native files); T4/T5b yuga; T7 signing"]
  viz --> front
  ui --> front
  crates --> pins --> build --> front
  front --> data
  front --> life
  shell --> front
  tests -.-> front
  check -.-> front
  later -.-> t5a
```

## Lanes

## Reviews

## Merge
