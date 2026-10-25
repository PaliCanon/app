# 013 The desktop app on Windows and Linux

status: building · rank: 7 · level: hard · type: feat · request: the user, 2026-10-25 (« build les qu'on ait tous les desktop ! », « aller ! qu'on ait les desktop app public ») · after: [007, 011, 012]
worktree: /Users/lsh/projects/palicanon/wt/app-013/app (branch `feat/app-desktop-platforms`); yuga worktree /Users/lsh/projects/palicanon/wt/app-013/yuga (branch `ci/sidecar-release` off `feat/sidecar`)

## Summary
Rank: difficulty 7 (three repositories: yuga's sidecar per platform, the app's xtask and keychain, two workflows; platforms that cannot be built on the orchestrator's Mac, so every Windows and Linux error is seen in CI only), impact 6 (the first public Windows and Linux downloads). No agents (standing order); one review round with @deep before the first release run.

macOS on Apple silicon is public since 2026-10-25 (app-v0.1.0, task 007). Intel macOS is out: ONNX Runtime (ort 2.0.0-rc.13, yuga's) ships no prebuilt binary for `x86_64-apple-darwin` (the local build failed on it); the site says Apple silicon only. Windows and Linux on x86_64:

- **The sidecar**: built from yuga commit 0a4d0e8 (`feat/sidecar`, pushed to the private anatta-rs/yuga on 2026-10-25) plus a workflow (`ci/sidecar-release`, no source change) on `ubuntu-22.04` and `windows-latest`; sigil-engine is fetched through a read-only deploy key (secret `SIGIL_DEPLOY_KEY` in yuga, key 167587560 on sigil-engine). The archives (`yuga-sidecar-0.5.2-<triple>.tar.gz`: binary, LICENSE.md, THIRD-PARTY-NOTICES of the same lockfile) are published by hand to yuga-releases, release `yuga-sidecar-v0.5.2`. **Blocked**: anatta-rs's Actions refuse private-repo runs ("recent account payments have failed"); the user fixes the billing (or rules otherwise).
- **xtask**: one `Target` per platform (`os`, `arch`, triple, `.exe`, the pinned archive); the build machine's target is used (bundles are built natively); a platform with no released sidecar is refused. The Linux and Windows entries are added with their digests once published.
- **The keychain**: keyring with the platform's store: Apple Keychain, Windows Credential Manager, the Secret Service on Linux (D-Bus). cargo-deny checks the Windows and Linux graphs too; four unmaintained crates of Tauri's GTK and zbus stack on Linux are ignored with their reasons.
- **The icon**: `icons/icon.ico` for Windows, made from `icon.png` by `cargo tauri icon`.
- **The bundles**: `release-desktop.yml` (by hand, `-f tag=app-v<version>`) builds the Linux `.deb` and `.AppImage` on ubuntu-22.04 and the Windows NSIS `setup.exe` on windows-latest, and `scripts/attach-bundles.sh` names them (versioned and stable: `PaliCanon-x86_64.deb`, `.AppImage`, `-setup.exe`), hashes them and attaches them to the release. Windows is unsigned (SmartScreen warns once). The build inputs (store, dictionary, forms, passages: already public inside every bundle) come from the pre-release `build-inputs-2026-10-25` of PaliCanon/app, checked against xtask's pins: no cloud credential in a public repository's CI. The Desktop OAuth secret is the repository secret `PALICANON_GOOGLE_DESKTOP_SECRET` (set from the Keychain, never printed).
- **CI**: `check-platforms` runs clippy on the desktop crate on Windows and Linux for every change; `scripts` runs `attach-bundles.test.sh` (30 checks).

The Windows and Linux bundles of 0.1.0 are built from a later commit than the macOS one (the platform plumbing only, Learn and Search unchanged); their release notes say which.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | ✓ | the user's go 2026-10-25 for the deploy key and the CI route; GCP identity for the public repo ruled out in favour of the build-inputs pre-release (no new exposure: the files ship in every bundle) |
| Build | doing | xtask targets (63 xtask tests), keyring per platform, icon.ico, deny targets, release-desktop.yml, attach-bundles.sh + test (30), check-platforms job; yuga sidecar workflow pushed, blocked by billing |

## Retro
