# 006 The progress synced through Google Drive in the desktop app

status: building · rank: 7 · level: hard · type: feature · request: learn-pali 050 (user, 2026-10-16: «lance la desktop que je puisse check le google sync»; «sur le desktop il faudrait afficher le lien pas juste ouvrir le navigateur») · design: learn-pali 050 v7 (its `app` node), session learn-pali-050-progress-sync · after: [005 (branched from feat/learn-search-yuga), learn-pali 050 (its SyncTransport seam)]

## Summary
Rank: difficulty 7 (OAuth installed-app flow with PKCE and a loopback in Rust, the Keychain, Drive calls, the shown link), impact 7 (the user's progress across devices) → 7.
The desktop transport of 050's sync, all in Rust: the installed-app flow (PKCE, 127.0.0.1 loopback on a random port, Desktop client 873594463453-v23gmvb9ksptrunuti0j4871p46i7q84.apps.googleusercontent.com, its non-confidential secret read from the build machine's keychain at build time — never in the repository), the sign-in link SHOWN in the app (selectable, Copy, Open in browser, Cancel) while the loopback waits, the refresh token in the macOS Keychain, Drive appDataFolder calls (the same `drive_location` as 050), revoke on sign out; commands sync_sign_in, sync_cancel, sync_now, sync_status, sync_sign_out; the front implements 050's SyncTransport over them; sync on start, at session end, Sync now, and every 5 minutes while open. The file id is logged (the user compares it with the site's).
learn-pali's fragment is consumed from the 050 branch commit until 050 is merged and tagged (then the tag).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | design learn-pali 050 v7 (app node), validated with the user's choices 2026-10-16; worktree /Users/lsh/projects/palicanon/wt/app006/app (feat/desktop-sync from feat/learn-search-yuga) |
