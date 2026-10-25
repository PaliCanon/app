# 012 The release script asked gh for a field it does not have

status: building · rank: 2 · level: simple · type: fix · request: the first real run of scripts/release.sh (007), 2026-10-25 · after: [011]
worktree: /Users/lsh/projects/palicanon/wt/app-012/app (branch `fix/app-release-latest`)

## Summary
Rank: difficulty 2 (one read in one script, the stub made strict), impact 2 (it stopped the run after the draft was complete; `publish` would have failed the same way). Light path, no viewer, no review.

The first full run built, signed, notarized and stapled the app and the DMG, passed Gatekeeper, and left the draft `app-v0.1.0` on GitHub with both assets at the built size (500,072,235 bytes), then refused: `gh release view --json isDraft,isLatest,assets` answers `Unknown JSON field: "isLatest"` (gh 2.102.0). The stub accepted any field. The latest release is now read from the API (`gh api repos/<repo>/releases/latest --jq .tag_name`, empty on 404), and the gh stub rejects `isLatest` as the real one does (the old script fails the suite).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | ✓ | `latest_tag` over the API; `release_json` asks for isDraft and assets only; the gh stub rejects isLatest and answers the API's latest; suite 120 with stubs, 117 with the real disk tools; the old script fails it |

## Retro
