# 011 The release script refused a good signature (SIGPIPE under pipefail)

status: merged · rank: 2 · level: simple · type: fix · request: the first real run of scripts/release.sh (007), 2026-10-25 · after: [007]
worktree: /Users/lsh/projects/palicanon/wt/app-011/app (branch `fix/app-release-pipefail`)

## Summary
Rank: difficulty 2 (one pattern in one script, a regression test), impact 2 (it blocked the first release; it never shipped a wrong one). Light path, no viewer, no review.

The first real `scripts/release.sh build` signed the sidecar and the app correctly (Developer ID, hardened runtime, `Timestamp=25 Oct 2026 at 08:35:45`, `codesign --verify --deep --strict` clean) and then refused with "the app's signature has no timestamp". Cause: `codesign -dvv "$app" 2>&1 | grep -q '^Timestamp='` under `set -o pipefail`: `grep -q` exits at its first match, `codesign` is still writing the lines after `Timestamp=` and dies of SIGPIPE, and pipefail makes the whole pipeline fail. The stubs printed one line, so the suite never saw it. Every such pipeline (`file`, `codesign`, `spctl` twice, the two header checks) now captures the tool's output first and searches it with a `has` helper. The codesign stub prints 200,000 lines after `Timestamp=` (the old script fails the suite on it), and the suite refuses any `| grep -q` in the script.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | ✓ | `has` helper, six pipelines rewritten; the codesign stub's long tail; the guard against `| grep -q`; suite 120 checks with stubs, 117 with the real disk tools; the old script fails it (build exit 1 "no timestamp") |
| Merge | ✓ | e434139 on main (squash); the release build re-run on main |

## Retro
One line: a stub that prints one line cannot show a pipe that breaks on a long output; the stubs of a tool now print as much as the tool.
