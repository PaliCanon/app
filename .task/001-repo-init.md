# 001 Repository init

status: merged · rank: 3 · level: standard · type: chore · request: app-001-palicanon-app (user, 2026-10-15: "créer le dépôt palicanon") · session: app-001-palicanon-app

## Summary
T1 of the request: the repository `PaliCanon/app` (private) holds the PaliCanon app, desktop and mobile, offline: Tauri 2 around the client-side Leptos code of the services. Display name "PaliCanon", bundle identifier `app.palicanon.learn`, fixed forever because it keys the user's data. The init gives it a Rust workspace with one crate, `palicanon_app_identity`, holding both facts as documented constants whose tests pin them (the first code the Tauri shell of T5a reads), the lint and deny configuration as the other repositories, a complete CI (no ui-lib dependency yet, so one checkout), CLAUDE.md, README.md and this folder. The init commits went straight to `main`, which the orchestrator protects afterwards; no deploy exists here.

Choices: `deny.toml` keeps learn-pali's licence policy (CC-BY-NC-4.0 and the permissive list) and drops its leptos-only advisory ignores and git source; rust-version 1.88 as learn-pali, no `rust-toolchain` file (learn-pali has none).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Init | done | 60b2f5c licence and .gitignore; 6500005 identity crate (5 tests); fc01708 clippy.toml, deny.toml; 89a50b4 CI; 36b9650 CLAUDE.md; b6888c5 README |
| Checks | done | fmt, clippy `-D warnings`, test (5 passed), rustdoc `-D warnings`, `cargo deny check`: all exit 0 |
| CI | done | run 38764018112 on b6888c5: rustfmt, clippy, test, rustdoc, msrv (1.88), cargo-deny, coverage all success; the Codecov upload step has no token (`CODECOV_TOKEN` empty) and, with `fail_ci_if_error: false`, does not fail the job |

## Retro
One line: clippy's `case_sensitive_file_extension_comparisons` rejected `ends_with(".app")` in a test (rewritten on the last label); run clippy before the first commit of any test.
