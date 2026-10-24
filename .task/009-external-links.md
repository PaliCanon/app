# 009 External links open in the system browser

status: building · rank: 2 · level: simple · type: fix · request: the user, 2026-10-24 (« read on sutta central ne marche pas ? ») · after: [008]
worktree: /Users/lsh/projects/palicanon/wt/app008/app (branch `feat/dictionary`, with 008)

## Summary
Rank: difficulty 2 (one command under the existing navigation rule, one click listener), impact 2 (every external link of the three fragments). No agents: the orchestrator builds.

'Read on SuttaCentral' (a `target="_blank"` anchor of the Search fragment) did nothing in the app: no line reached the log, so neither the `on_new_window` nor the `on_navigation` hook saw the click. The fix does not wait for the webview: a document click listener in the front (`links.rs`) takes every anchor click, and an `https:` or `mailto:` address is sent to the desktop's new `open_outside(url)` command, the click cancelled; the app's own links stay with the router. The command applies the same navigation rule (`navigation::open_request`: `https:`/`mailto:` only, the refusal logged and returned) and opens through the opener plugin from Rust, as the hooks do. The capability grants sixteen commands. The hooks stay as the second line.

Ruled out: granting the webview the opener plugin (`opener:allow-open-url`): the rule would not apply.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | ✓ | 97f82ff; bridge `OPEN_OUTSIDE`, desktop `open_request` + command, capability, front `links.rs` + `tauri::open_outside`, installed before the mount |

| Verify | ✓ | workspace clippy, front clippy on wasm32, tests (bridge 16 commands, acl, config, navigation `open_request`, front `external`) |
| Check | doing | bundle rebuilt and opened 2026-10-24; the user clicks 'Read on SuttaCentral' in Search: the system browser opens the sutta; the log says 'opened in the system browser' |

## Retro
