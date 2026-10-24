#!/usr/bin/env bash
# Tests for scripts/release.sh (task 007), against stubs of every tool it
# calls: git, gh, security, xcrun, codesign, cargo, file, ditto, hdiutil,
# spctl, shasum. Each stub logs its call to $STUB_LOG and answers as the
# test's STUB_* variables say, so the suite proves the refusals, the gates
# and the ORDER of the chain (the .app stapled before hdiutil, the hash after
# the dmg's staple, the versioned asset before the stable one, no publish in
# `build`), and that the OAuth secret never reaches stdout or stderr.
#
# What it does NOT prove: that the real tools accept the arguments (a wrong
# codesign flag is seen on the user's Mac at the first real run), and the
# wiring of the real Keychain.
# Run: bash scripts/release.test.sh — needs bash and python3. With
# RELEASE_TEST_REAL_TOOLS=1 (the macOS CI job) hdiutil, ditto, shasum and cp
# are the real ones on a fake 2 KiB .app, so a wrong flag of theirs is caught;
# codesign, xcrun, spctl, gh, git, security, cargo, file and curl stay stubs.
# Exit 0 when every assertion held.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
script="$here/release.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

checks=0
failed=0
pass() { checks=$((checks + 1)); echo "ok   - $1"; }
flunk() { checks=$((checks + 1)); failed=$((failed + 1)); echo "FAIL - $1"; }
assert_eq() { # name want got
  if [ "$2" = "$3" ]; then pass "$1"; else flunk "$1 (want '$2', got '$3')"; fi
}
assert_match() { # name pattern text
  if grep -Eq -- "$2" <<<"$3"; then pass "$1"; else flunk "$1 (no match for /$2/ in: $(head -c 400 <<<"$3"))"; fi
}
assert_absent() { # name needle text
  if grep -Fq -- "$2" <<<"$3"; then flunk "$1 ('$2' found)"; else pass "$1"; fi
}
log_line() { grep -n -E -- "$1" "$log" | head -1 | cut -d: -f1; }
assert_before() { # name pattern_a pattern_b (a logged before b)
  local a b
  a="$(log_line "$2")"
  b="$(log_line "$3")"
  if [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ]; then pass "$1"; else flunk "$1 (/$2/ at line '${a:-none}', /$3/ at line '${b:-none}')"; fi
}
assert_logged() { # name pattern
  if grep -Eq -- "$2" "$log"; then pass "$1"; else flunk "$1 (/$2/ not logged)"; fi
}
assert_not_logged() { # name pattern
  if grep -Eq -- "$2" "$log"; then flunk "$1 (/$2/ logged)"; else pass "$1"; fi
}

SECRET="oauth-secret-of-the-test-7f3a"

# --- the stubs ---------------------------------------------------------------
stubs="$work/stubs"
mkdir -p "$stubs"
stub() { # name body
  printf '#!/usr/bin/env bash\necho "%s $*" >>"$STUB_LOG"\n%s\n' "$1" "$2" >"$stubs/$1"
  chmod +x "$stubs/$1"
}
stub git '
case "$*" in
  *"status --porcelain"*) printf "%s" "${STUB_DIRTY:-}" ;;
  *"rev-parse --abbrev-ref HEAD"*) echo "${STUB_BRANCH:-main}" ;;
  *"rev-parse HEAD"*) echo "aaaa1111" ;;
  *"rev-parse origin/main"*) echo "${STUB_ORIGIN:-aaaa1111}" ;;
  *"tag --list"*) printf "%s" "${STUB_LOCAL_TAG:-}" ;;
  *"ls-remote"*) printf "%s" "${STUB_REMOTE_TAG:-}" ;;
  *fetch*) ;;
esac'
stub gh '
state="$STUB_UPLOADS/.state"
case "$*" in
  *"release view"*"--json isDraft")
    # the preflight: an existing release, draft or published; else gh says
    # not found (exit 1), or fails for another reason (STUB_GH_DOWN)
    [ -n "${STUB_GH_DOWN:-}" ] && { echo "error connecting to api.github.com" >&2; exit 1; }
    [ -n "${STUB_RELEASE_EXISTS:-}" ] || { echo "release not found" >&2; exit 1; }
    echo "{\"isDraft\": ${STUB_RELEASE_EXISTS}}" ;;
  *"release view"*"--json"*)
    python3 - "$STUB_UPLOADS" "${STUB_DRAFT:-true}" "${STUB_LATEST:-false}" "${STUB_SIZE_OVERRIDE:-}" <<'"'"'PY'"'"'
import json, os, sys
d, draft, latest, override = sys.argv[1], sys.argv[2] == "true", sys.argv[3] == "true", sys.argv[4]
state = os.path.join(d, ".state")
if os.path.exists(state):
    s = open(state).read()
    if "published" in s: draft = False
    if "latest" in s: latest = True
assets = []
for n in sorted(os.listdir(d)) if os.path.isdir(d) else []:
    if n.startswith("."): continue
    size = int(override) if override and n.endswith("aarch64.dmg") and "-0." not in n else os.path.getsize(os.path.join(d, n))
    assets.append({"name": n, "size": size})
print(json.dumps({"isDraft": draft, "isLatest": latest, "assets": assets}))
PY
    ;;
  *"release delete"*) ;;
  *"release create"*) ;;
  *"release upload"*)
    mkdir -p "$STUB_UPLOADS"
    for a in "$@"; do
      if [ -f "$a" ]; then
        case "$a" in *PaliCanon-aarch64.dmg) [ -n "${STUB_UPLOAD_SECOND_FAILS:-}" ] && exit 1 ;; esac
        cp "$a" "$STUB_UPLOADS/"
      fi
    done ;;
  *"release edit"*"--draft=false --latest")
    mkdir -p "$STUB_UPLOADS"; echo "published" >>"$state"
    [ -n "${STUB_LATEST_FAILS:-}" ] && exit 1
    echo "latest" >>"$state" ;;
  *"release edit"*"--latest") mkdir -p "$STUB_UPLOADS"; echo "latest" >>"$state" ;;
esac'
stub curl '
if [ -n "${STUB_LINK_FAIL:-}" ]; then printf "HTTP/2 404\r\n\r\n"; exit 0; fi
size=$(wc -c <"$STUB_UPLOADS/PaliCanon-aarch64.dmg" 2>/dev/null | tr -d " ")
printf "HTTP/2 302\r\nlocation: https://objects.example/x\r\n\r\nHTTP/2 200\r\ncontent-length: ${STUB_LINK_SIZE:-$size}\r\n\r\n"'
stub security '
case "$*" in
  *find-identity*)
    n="${STUB_IDENTITIES:-1}"
    i=1; while [ "$i" -le "$n" ]; do
      echo "  $i) 0123456789ABCDEF0123456789ABCDEF0123456$i \"Developer ID Application: Test Person (TEAM123456)\""; i=$((i+1)); done
    echo "  9) FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF \"Apple Development: Someone (TEAM123456)\""
    echo "     $((n+1)) valid identities found" ;;
  *find-generic-password*) [ -n "${STUB_NO_OAUTH:-}" ] && exit 44; echo "$STUB_SECRET" ;;
esac'
stub xcrun '
case "$*" in
  *"notarytool history"*) [ -n "${STUB_NOTARY_FAIL:-}" ] && exit 1 || exit 0 ;;
  *"notarytool submit"*) echo "{\"status\":\"${STUB_NOTARY_STATUS:-Accepted}\",\"id\":\"sub-1\"}" ;;
  *"notarytool log"*) ;;
  *stapler*) ;;
esac'
stub codesign '
case "$*" in
  *-dvv*) echo "Timestamp=24 Oct 2026" >&2 ;;
  *) ;;
esac'
stub cargo '
echo "cargo-env RUSTFLAGS=${RUSTFLAGS:-}" >>"$STUB_LOG"
app="$RELEASE_ROOT/target/release/bundle/macos/PaliCanon.app/Contents/MacOS"
mkdir -p "$app"
{ echo "binary"; [ -z "${STUB_BINARY_NO_SECRET:-}" ] && echo "$STUB_SECRET"; [ -n "${STUB_LEAK:-}" ] && echo "/Users/someone/src"; } >"$app/palicanon-app-desktop"
echo "sidecar" >"$app/yuga-sidecar"
{ echo "<plist>"; [ -n "${STUB_PLIST_LEAK:-}" ] && echo "/Users/someone"; echo "</plist>"; } >"$app/../Info.plist"'
stub file '
case "$1" in
  *yuga-sidecar) [ -n "${STUB_SIDECAR_X86:-}" ] && a=x86_64 || a=arm64 ;;
  *) [ -n "${STUB_BINARY_X86:-}" ] && a=x86_64 || a=arm64 ;;
esac
echo "$1: Mach-O 64-bit executable $a"'
stub spctl '
case "$*" in *"-t exec"*) [ -n "${STUB_SPCTL_REJECT_EXEC:-}" ] && { echo rejected >&2; exit 3; } ;; esac
if [ -n "${STUB_SPCTL_REJECT:-}" ]; then echo "rejected" >&2; exit 3; fi; echo "accepted"; echo "source=Notarized Developer ID"'
if [ -n "${RELEASE_TEST_REAL_TOOLS:-}" ]; then
  # The real tool runs; the wrapper only logs the call.
  for tool in ditto hdiutil shasum cp; do
    real="$(PATH=/usr/bin:/bin command -v "$tool")"
    stub "$tool" "exec $real \"\$@\""
  done
else
stub ditto '
if [ "$1" = -c ]; then echo zip >"${@: -1}"; else cp -R "$1" "$2"; fi'
stub hdiutil '
case "$1" in
  create) echo "dmg-image-bytes" >"${@: -1}" ;;
  attach) mp=""; while [ $# -gt 0 ]; do [ "$1" = -mountpoint ] && mp="$2"; shift; done; mkdir -p "$mp" ;;
  detach) ;;
esac'
stub shasum 'echo "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef  $2"'
fi
sha_pattern="deadbeef"
[ -n "${RELEASE_TEST_REAL_TOOLS:-}" ] && sha_pattern="sha256 [0-9a-f]{64}"

# --- the fake repository root -------------------------------------------------
fake="$work/root"
mkdir -p "$fake/crates/desktop"
printf '{"version": "0.1.0"}\n' >"$fake/crates/desktop/tauri.conf.json"
printf '[workspace]\nmembers = ["crates/desktop"]\n\n[workspace.package]\nversion = "0.1.0"\n\n[workspace.dependencies]\nserde = { version = "1" }\n' >"$fake/Cargo.toml"
printf '[package]\nname = "palicanon-app-desktop"\nversion.workspace = true\n' >"$fake/crates/desktop/Cargo.toml"

# run MODE [VAR=value ...] -> $out $err $code $log
run() {
  local mode=$1
  shift
  log="$work/log.$RANDOM"
  : >"$log"
  rm -rf "$fake/target"
  out="$(env -i PATH="$stubs:/usr/bin:/bin" HOME=/Users/test STUB_LOG="$log" STUB_SECRET="$SECRET" STUB_UPLOADS="$work/uploads.$RANDOM" \
    RELEASE_ROOT="$fake" RELEASE_SCRATCH="$fake/target/scratch" "$@" bash "$script" "$mode" 2>"$work/err" </dev/null)"
  code=$?
  err="$(cat "$work/err")"
}
refused() { # name
  assert_eq "$1: exit 1" "1" "$code"
  assert_match "$1: a reason on stderr" "release.sh: " "$err"
}

echo "== usage"
run ""
assert_eq "no mode: exit 64" "64" "$code"
run deploy
assert_eq "unknown mode: exit 64" "64" "$code"

echo "== build: the happy path and its order"
run build
assert_eq "build: exit 0 ($err)" "0" "$code"
assert_before "the sidecar is signed before the app" "codesign --force --timestamp --options runtime -s [0-9A-F]+ .*yuga-sidecar" "codesign --force --timestamp --options runtime -s [0-9A-F]+ .*PaliCanon.app$"
assert_logged "the identity is the 40-hex hash of the Developer ID line" "codesign .* -s 0123456789ABCDEF0123456789ABCDEF01234561 "
assert_logged "tauri builds unsigned" "cargo tauri build --bundles app --config tauri.bundle.conf.json --no-sign"
assert_logged "the home path is remapped" "cargo-env RUSTFLAGS=--remap-path-prefix=/Users/test=/build"
assert_before "the app is notarized before it is stapled" "notarytool submit .*PaliCanon.zip" "stapler staple .*PaliCanon.app"
assert_before "the app is stapled BEFORE the dmg is made" "stapler staple .*PaliCanon.app" "hdiutil create"
assert_before "the dmg is signed before its notarization" "codesign .*PaliCanon-0.1.0-aarch64.dmg" "notarytool submit .*PaliCanon-0.1.0-aarch64.dmg"
assert_before "the dmg is stapled before spctl" "stapler staple .*PaliCanon-0.1.0-aarch64.dmg" "spctl -a -t open"
assert_before "the hash is taken AFTER the dmg's staple" "stapler staple .*PaliCanon-0.1.0-aarch64.dmg" "shasum -a 256"
assert_before "the app inside the dmg is assessed" "hdiutil attach" "spctl -a -t exec"
assert_before "the dmg is detached after" "spctl -a -t exec" "hdiutil detach"
assert_logged "the release is a draft on the pushed commit" "gh release create app-v0.1.0 --repo PaliCanon/app --draft --target aaaa1111"
assert_before "the versioned asset is uploaded before the stable one" "release upload .*PaliCanon-0.1.0-aarch64.dmg" "release upload .*PaliCanon-aarch64.dmg"
assert_not_logged "build never publishes" "release edit"
assert_match "build says what to do next" "scripts/release.sh publish" "$out"
assert_match "build reports the sha" "$sha_pattern" "$out"
assert_eq "the stable copy is hashed too (two shasum calls)" "2" "$(grep -c '^shasum ' "$log")"
assert_before "the stable copy is hashed after the versioned dmg" "^shasum .*PaliCanon-0.1.0-aarch64.dmg" "^shasum .*PaliCanon-aarch64.dmg"
assert_absent "the secret is not on stdout" "$SECRET" "$out"
assert_absent "the secret is not on stderr" "$SECRET" "$err"
assert_absent "the secret is not in a logged argument" "$SECRET" "$(cat "$log")"
assert_match "the notes carry the version, the size and the sha" "PaliCanon 0.1.0 for macOS" "$(cat "$fake/target/scratch/notes.md")"
assert_match "the notes carry the sha" "${sha_pattern#sha256 }" "$(cat "$fake/target/scratch/notes.md")"
assert_absent "no placeholder left in the notes" "@" "$(cat "$fake/target/scratch/notes.md")"

echo "== build: a dirty shell (T5)"
run build APPLE_SIGNING_IDENTITY="Developer ID Application: Someone Else (TEAM000000)"
assert_eq "build with APPLE_SIGNING_IDENTITY exported: exit 0 ($err)" "0" "$code"
assert_eq "codesign is called three times: sidecar, app, dmg; Tauri never" "3" "$(grep -c '^codesign --force' "$log")"
assert_logged "tauri still builds unsigned" "cargo tauri build .* --no-sign"

echo "== build: the preflight refusals, nothing built"
printf '{"version": "0.1.1"}\n' >"$fake/crates/desktop/tauri.conf.json"
run build
refused "version drift"
assert_match "version drift names both" "0.1.1.*0.1.0" "$err"
printf '{"version": "0.1.0"}\n' >"$fake/crates/desktop/tauri.conf.json"
printf '[package]\nname = "palicanon-app-desktop"\nversion = "0.1.0"\n' >"$fake/crates/desktop/Cargo.toml"
run build
refused "the desktop crate with its own version line"
printf '[package]\nname = "palicanon-app-desktop"\nversion.workspace = true\n' >"$fake/crates/desktop/Cargo.toml"
run build STUB_DIRTY=" M x"
refused "a dirty tree"
run build STUB_BRANCH=feat/x
refused "not on main"
run build STUB_ORIGIN=bbbb2222
refused "HEAD not pushed"
run build STUB_LOCAL_TAG=app-v0.1.0
refused "the tag exists locally"
run build STUB_REMOTE_TAG="abc refs/tags/app-v0.1.0"
refused "the tag exists on origin"
run build STUB_RELEASE_EXISTS=false
refused "a published release exists"
assert_match "a published release: never republish" "never republish" "$err"
assert_not_logged "a published release: nothing deleted" "release delete"
run build STUB_GH_DOWN=1
refused "gh failing for another reason than not found"
assert_match "gh down: the reason is gh, not the version" "could not read the releases" "$err"
assert_not_logged "gh down: nothing created" "release create"
run build STUB_RELEASE_EXISTS=true
assert_eq "a draft left by an aborted run: exit 0 ($err)" "0" "$code"
assert_before "the stale draft is deleted before the build" "^gh release delete app-v0.1.0 --repo PaliCanon/app --yes" "cargo tauri build"
assert_match "the stale draft is said" "aborted run" "$out"
run build STUB_UPLOAD_SECOND_FAILS=1
assert_eq "a failed second upload: exit 1" "1" "$code"
assert_not_logged "a failed second upload publishes nothing" "release edit"
run build STUB_IDENTITIES=0
refused "no Developer ID identity"
run build STUB_IDENTITIES=2
refused "two Developer ID identities"
run build STUB_NOTARY_FAIL=1
refused "the notary profile does not answer"
run build STUB_NO_OAUTH=1
refused "no OAuth secret in the Keychain"
assert_not_logged "a refused preflight builds nothing" "cargo tauri build"

echo "== build: the gates after the build"
run build STUB_BINARY_NO_SECRET=1
refused "the secret is not in the binary"
assert_not_logged "no signing after a failed gate" "^codesign"
run build STUB_SIDECAR_X86=1
refused "a sidecar that is not arm64"
run build STUB_BINARY_X86=1
refused "a main binary that is not arm64"
run build STUB_PLIST_LEAK=1
refused "a /Users/ path in Info.plist"
run build STUB_LEAK=1
refused "a /Users/ path in the binary"
run build STUB_NOTARY_STATUS=Invalid
refused "a notarization that is not Accepted"
assert_logged "the notary log is fetched" "notarytool log sub-1"
assert_not_logged "nothing stapled after Invalid" "stapler"
run build STUB_SPCTL_REJECT=1
refused "Gatekeeper refuses the dmg"
assert_not_logged "a refused dmg is never attached" "hdiutil attach"
run build STUB_SPCTL_REJECT_EXEC=1
refused "Gatekeeper refuses the app inside the dmg"
assert_logged "the dmg is attached under the scratch" "^hdiutil attach -nobrowse -readonly -mountpoint .*/target/scratch/mnt "
assert_before "the dmg is detached before the refusal exits" "^hdiutil attach" "^hdiutil detach"
assert_not_logged "no release after a refusal" "release create"

if [ -z "${RELEASE_TEST_REAL_TOOLS:-}" ]; then
echo "== build: a short copy of the stable asset (T6)"
stub cp 'case "$1" in *.dmg) [ -n "${STUB_SHORT_COPY:-}" ] && { head -c 3 "$1" >"$2"; exit 0; } ;; esac; exec /bin/cp "$@"'
stub shasum 'if [ -n "${STUB_SHORT_COPY:-}" ]; then shasum_real=/usr/bin/shasum; [ -x "$shasum_real" ] || shasum_real="$(command -v sha256sum)"; "$shasum_real" "$@" 2>/dev/null | sed "s/^\([0-9a-f]*\) .*/\1  $2/" || sha256sum "$2"; else echo "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef  $2"; fi'
run build STUB_SHORT_COPY=1
refused "a stable copy shorter than the dmg"
assert_not_logged "a short copy releases nothing" "release create"
rm -f "$stubs/cp"
stub shasum 'echo "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef  $2"'
fi

echo "== publish"
uploads="$work/pub-uploads"
mkdir -p "$uploads"
echo "dmg-image-bytes" >"$uploads/PaliCanon-0.1.0-aarch64.dmg"
echo "dmg-image-bytes" >"$uploads/PaliCanon-aarch64.dmg"
run publish STUB_UPLOADS="$uploads"
assert_eq "publish: exit 0 ($err)" "0" "$code"
assert_logged "publish flips the draft and marks it latest" "gh release edit app-v0.1.0 --repo PaliCanon/app --draft=false --latest"
assert_before "the link is read after the edit" "release edit" "^curl -sIL https://github.com/PaliCanon/app/releases/latest/download/PaliCanon-aarch64.dmg"
assert_match "publish prints the stable link" "published: https://github.com/PaliCanon/app/releases/latest/download/PaliCanon-aarch64.dmg" "$out"
rm -f "$uploads/.state"
run publish STUB_UPLOADS="$uploads" STUB_LATEST_FAILS=1
refused "an edit that published but did not mark latest"
assert_not_logged "no link printed on a half edit" "published:"
run publish STUB_UPLOADS="$uploads" STUB_DRAFT=false STUB_LATEST=false
assert_eq "re-run after a half edit: exit 0 ($err)" "0" "$code"
assert_logged "re-run after a half edit: only --latest is set" "^gh release edit app-v0.1.0 --repo PaliCanon/app --latest$"
assert_not_logged "re-run after a half edit: the draft flag is not touched" "draft=false"
rm -f "$uploads/.state"
run publish STUB_UPLOADS="$uploads" STUB_DRAFT=false STUB_LATEST=true
assert_eq "publish on an already published latest release: exit 0, nothing edited ($err)" "0" "$code"
assert_not_logged "already published: no edit" "release edit"
rm -f "$uploads/.state"
run publish STUB_UPLOADS="$uploads" STUB_LINK_FAIL=1
refused "the stable link does not answer 200"
rm -f "$uploads/.state"
run publish STUB_UPLOADS="$uploads" STUB_LINK_SIZE=7
refused "the stable link's size differs from the asset"
rm -f "$uploads/.state"
run publish STUB_UPLOADS="$uploads" STUB_SIZE_OVERRIDE=0
refused "publish with an empty stable asset"
rm "$uploads/PaliCanon-aarch64.dmg"
run publish STUB_UPLOADS="$uploads"
refused "publish with the stable asset missing"
run publish STUB_UPLOADS="$work/none"
refused "publish with no asset at all"

echo "== wiring"
if grep -q "bash scripts/release.test.sh" "$root/.github/workflows/ci.yml"; then pass "ci.yml runs this suite"; else flunk "ci.yml runs this suite"; fi
if grep -q "RELEASE_TEST_REAL_TOOLS=1 bash scripts/release.test.sh" "$root/.github/workflows/ci.yml"; then pass "ci.yml runs this suite on macOS with the real disk tools"; else flunk "ci.yml runs this suite on macOS with the real disk tools"; fi
if [ -x "$script" ]; then pass "release.sh is executable"; else flunk "release.sh is executable"; fi

# The count is pinned per mode: the stub mode owns the order and argv
# assertions that the real-tools mode cannot make (review 007, round 2).
if [ -n "${RELEASE_TEST_REAL_TOOLS:-}" ]; then want_checks=115; else want_checks=118; fi
assert_eq "the suite ran every check of this mode" "$want_checks" "$checks"

echo
echo "$checks checks, $failed failed"
[ "$failed" -eq 0 ]
