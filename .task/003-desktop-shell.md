# 003 The desktop shell with Learn and Search

status: design · rank: 7 · level: hard · type: feature · request: app-001-palicanon-app T5a (user, 2026-10-16: "build l'app que je puisse checker"; "Oui, construis 048 + 021") · session: app-003-desktop-shell · after: [learn-pali 048, search-front 021]

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

## Design
Viewer: app-003-desktop-shell/design (d185d447c3f9e4e), v1 (not yet validated).
```mermaid
flowchart TB
  subgraph t5a["PaliCanon/app 003 — the desktop shell with Learn and Search (T5a of app-001 v8), macOS first"]
    crates["workspace: crates/identity (exists); crates/front (the client-side Leptos app, csr, built to wasm by Trunk); crates/desktop (the Tauri 2 shell, its commands); front depends on learn-pali (tag after 048, features csr) and search-front (tag after 021, features csr) by git TAG, and palicanon-ui-lib v0.3.0 (csr); the lockstep check: cargo metadata shows exactly one palicanon-ui-lib source and rev, its features include csr and exclude ssr and hydrate (a test in CI)"]
    front["front: csr::mount(App); App = LearnProvider(app store, app files) above one Router; inside, ui-lib's AppShell (the rail: Learn /learn, Search /search, Map /map later; theme and language from the Local Store; Settings with LearnSettings); Routes: / redirects to /learn; ParentRoute /learn → LearnRoutes; ParentRoute /search → SearchProvider(the app's backend) + SearchRoutes; a Not found. Footer: ui-lib's, the app's own links (no site badge sha); window title 'PaliCanon'"]
    store["the app's ProgressStore (front, over Tauri commands): loads the progress file BEFORE mounting (progress_load) and builds its memory on it; save updates memory at once and starts progress_save at once (one in flight, later saves coalesced to the latest state); Transient failure: memory kept, status Failed, retried every 5 s, on the next save and on page hide; NewerVersion: writing stops, the latest memory handed to the error, the file re-read, on_change fired; Unreadable: no write ever; exit and page hide write the latest memory (never after Unreadable or NewerVersion)"]
    files["the app's FileTransfer: export through the dialog plugin's save dialog + fs write; import through the open dialog + fs read; a cancel is a no-op, a failure shown"]
    cmds["desktop commands (Rust): progress_load → Ok(text) | Empty | Unreadable(msg); a file that parses badly is moved aside to progress.json.corrupt-<time> and answered Invalid with its raw text; progress_save(text, version) writes temp, fsync, rename, fsync dir, and refuses when the file on disk carries a newer format version (NewerVersion); app data dir keyed by app.palicanon.learn; no other command in this task"]
    search_be["the app's SearchBackend in this task: a placeholder that answers Words searches as Failed with the reason 'local search arrives with the next version' and meaning() false — the real yuga backend is T5b; the Search screen, its home, verse and examples work fully offline already"]
    shell["Tauri: productName PaliCanon, identifier app.palicanon.learn (fixed forever), window 1100×800 min 360×500, CSP strict (self, no remote), the opener plugin opens every external link (suttacentral.net, the site links) in the system browser, never in the window; assets /lotus.png and /brand/*.svg served at the root; macOS .app bundle, unsigned in this task (signing is T7)"]
    tests["tests: front native tests of the routes (/ → /learn, /learn and /search render, Not found); the store's queue over a fake command layer (coalescing, Transient retry, NewerVersion stop and re-read, Unreadable no write, exit write); commands tested natively on a temp dir (atomic write, corrupt moved aside, newer refused); the lockstep test; one browser test of the front (csr::mount, the rail, a Learn answer saved through a fake command); CI: fmt, clippy, tests, the front's wasm build, deny; the .app built locally for the user's check (no CI runner minutes: Actions is blocked)"]
  end
  learn["learn-pali 048 (Learn fragment, tag)"]
  search["search-front 021 (Search fragment, tag)"]
  ui["ui-lib v0.3.0"]
  later["later: T5b yuga sidecar + bge-m3 int8 bundled (the real SearchBackend); T9 the Map under /map; T7 signing"]
  learn --> front
  search --> front
  ui --> front
  crates --> front
  front --> store --> cmds
  front --> files
  front --> search_be
  shell --> front
  tests -.-> front
  later -.-> t5a
```

## Lanes

## Reviews

## Merge
