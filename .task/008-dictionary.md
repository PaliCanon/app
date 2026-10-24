# 008 The dictionary offline in the desktop app

status: review · rank: 5 · level: standard · type: feature · request: learn-pali 068 (user, 2026-10-23: «dico pas dans l'app ?» … «on a pas besoin de taper l'API, on a déjà tout en local non ?» … «Lance dictionnaire et app.») · after: [006]
session: learn-pali-068-passages-and-dictionary-port (archived 2026-10-24 with 068; what follows logs in the app's lasting session)
worktree: /Users/lsh/projects/palicanon/wt/app008/app (branch `feat/dictionary`, from `feat/desktop-sync`)

## Summary
Rank: difficulty 5 (three SQLite assets shipped by hash, three Tauri commands, the fragment's new port implemented over them; nothing new in the design), impact 5 (the app's Learn search, passages and tappable words work offline; the app grows by ≈ 64 MB). No agents (user, 2026-10-22): the orchestrator builds.

The app showed `Unreachable` on Learn's search: the dictionary lived behind the API, and the app holds no token. learn-pali 068 gives the fragment a `Dictionary` port (search, passage, lookup; one callback each) and a precomputed passages asset. This task implements the port in the app:
- **xtask `dictionary`** fetches three pinned assets into `target/dictionary/` under fixed names, checked by SHA-256 like yuga's (`dictionary.db` 23 MB, api 008; `dpd.db` 11 MB, the forms; `passages.db` 30.3 MB, learn-pali 068); the bundle copies them to `Resources/dictionary/`.
- **The desktop** opens each asset read-only the first time it is asked (`dictionary.rs`) and answers three commands: `dictionary_search(q)` (the API's own query and order: fold, prefix, exact first, then frequency, length, lemma, 20 at most), `dictionary_passage(lemma)` (the row, found, missed by name, or absent), `dictionary_lookup(form)` (the API's normalise, the keys, the senses in key order). A file missing answers `Unavailable` for its call only.
- **The front** maps the bridge's wire types to the fragment's (`dictionary.rs`: `AppDictionary` over a `DictionaryPort`, `TauriDictionary` over `invoke`) and passes it to `LearnProvider`.
- The capability grants the three commands (fifteen now); the bridge's `COMMANDS` lists them for tauri-build.

Ruled out: calling the API from the app (no token in a client, security.md); shipping the raw corpora (169 MB) for live passages.
Open: nothing on the pin (moved to `fragment-v0.5.0` once 068 merged). The app grows by ≈ 64 MB of dictionary assets.

## Design
Viewer: learn-pali-068-passages-and-dictionary-port/design (the `port` subgraph, `app` node), validated by the user's order 2026-10-23.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | ✓ | the design's app node; this file |
| Build | ✓ | a3bc639 bridge types and constants, xtask `dictionary` (3 tests), desktop `dictionary.rs` (6 tests) and commands, capability (fifteen), front `dictionary.rs` (4 tests) and `TauriDictionary`, `App dictionary=`; ab30c6c the reader checks the asset's schema and completeness (068 review); 30232da pins on `fragment-v0.5.0` (068 merged 966db20), local patch dropped |
| Verify | ✓ | on 30232da: workspace clippy -D warnings, doc, tests (bridge, xtask incl. pins, desktop lib + acl/config/commands, front); `xtask dictionary` staged dictionary.db 23 MB, dpd.db 11 MB, passages.db 30 MB from the local cache (sha checked) |
| Bundle | ✓ | `cargo tauri build --bundles app --config tauri.bundle.conf.json` on 30232da: PaliCanon.app 727 MB, `Resources/dictionary/` holds the three assets; opened for the user 2026-10-24 (the 006 bundle I had opened was quit first) |
| Bundle (v0.5.1) | ✓ | 02a348b pins on `fragment-v0.5.1` (069: the search icon beside the '+'); bundle rebuilt and opened 2026-10-24 |
| Check | ✓ | the user, 2026-10-24, on the bundle (02a348b + 009): « aller ! » |

## Retro
