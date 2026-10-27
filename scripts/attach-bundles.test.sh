#!/usr/bin/env bash
# Tests for scripts/attach-bundles.sh (task 013) against a stub gh: the names
# (versioned and stable, per bundle kind), the .sha256 beside each versioned
# file, the refusals (a tag that is not the version, no release, no bundle,
# two candidates), and the upload of every file with --clobber.
# Run: bash scripts/attach-bundles.test.sh — needs bash and python3.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
script="$here/attach-bundles.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

checks=0
failed=0
pass() { checks=$((checks + 1)); echo "ok   - $1"; }
flunk() { checks=$((checks + 1)); failed=$((failed + 1)); echo "FAIL - $1"; }
assert_eq() { if [ "$2" = "$3" ]; then pass "$1"; else flunk "$1 (want '$2', got '$3')"; fi; }
assert_match() { if grep -Eq -- "$2" <<<"$3"; then pass "$1"; else flunk "$1 (no /$2/ in: $(head -c 300 <<<"$3"))"; fi; }

stubs="$work/stubs"
mkdir -p "$stubs"
cat >"$stubs/gh" <<'EOF'
#!/usr/bin/env bash
echo "gh $*" >>"$STUB_LOG"
case "$*" in
  *"release view"*) [ -n "${STUB_NO_RELEASE:-}" ] && exit 1; printf '%s\n' ${STUB_EXISTING:-} ;;
  *"release upload"*) ;;
esac
EOF
chmod +x "$stubs/gh"

fake="$work/root"
mkdir -p "$fake/crates/desktop"
printf '{"version": "0.1.0"}\n' >"$fake/crates/desktop/tauri.conf.json"

bundles() { # kinds...
  local b="$work/bundle"
  rm -rf "$b"
  for k in "$@"; do
    case "$k" in
      deb) mkdir -p "$b/deb" && echo deb >"$b/deb/PaliCanon_0.1.0_amd64.deb" ;;
      appimage) mkdir -p "$b/appimage" && echo img >"$b/appimage/PaliCanon_0.1.0_amd64.AppImage" ;;
      nsis) mkdir -p "$b/nsis" && echo exe >"$b/nsis/PaliCanon_0.1.0_x64-setup.exe" ;;
      twodebs) mkdir -p "$b/deb" && echo a >"$b/deb/a.deb" && echo b >"$b/deb/b.deb" ;;
    esac
  done
}
run() { # tag [VAR=value...]
  local tag=$1
  shift
  log="$work/log.$RANDOM"
  : >"$log"
  rm -rf "$work/out"
  out="$(env -i PATH="$stubs:/usr/bin:/bin" STUB_LOG="$log" ATTACH_ROOT="$fake" ATTACH_OUT="$work/out" ATTACH_COMMIT=abc1234 "$@" bash "$script" "$tag" "$work/bundle" 2>"$work/err")"
  code=$?
  err="$(cat "$work/err")"
}

echo "== Linux: deb and AppImage"
bundles deb appimage
run app-v0.1.0
assert_eq "exit 0 ($err)" "0" "$code"
for f in PaliCanon-0.1.0-x86_64.deb PaliCanon-x86_64.deb PaliCanon-0.1.0-x86_64.AppImage PaliCanon-x86_64.AppImage PaliCanon-0.1.0-x86_64.deb.sha256 PaliCanon-0.1.0-x86_64.AppImage.sha256; do
  if [ -f "$work/out/$f" ]; then pass "$f made"; else flunk "$f made"; fi
  assert_match "$f uploaded with --clobber" "gh release upload app-v0.1.0 --repo PaliCanon/app --clobber .*/$f$" "$(cat "$log")"
done
assert_match "the sha256 file names the versioned file" "^[0-9a-f]{64}  PaliCanon-0.1.0-x86_64.deb$" "$(cat "$work/out/PaliCanon-0.1.0-x86_64.deb.sha256")"
assert_eq "the stable copy is the same file" "$(cat "$work/out/PaliCanon-0.1.0-x86_64.deb")" "$(cat "$work/out/PaliCanon-x86_64.deb")"
assert_eq "no Windows file on Linux" "0" "$(ls "$work/out" | grep -c setup || true)"
info="$(cat "$work/out"/BUILD-INFO-*.txt 2>/dev/null)"
assert_match "the build info names the tag" "^tag app-v0.1.0$" "$info"
assert_match "the build info names the commit" "^commit abc1234$" "$info"
last_bundle="$(grep -n 'release upload' "$log" | grep -v sha256 | tail -1 | cut -d: -f1)"
first_sha="$(grep -n 'release upload.*sha256' "$log" | head -1 | cut -d: -f1)"
if [ -n "$first_sha" ] && [ "$first_sha" -gt "$last_bundle" ]; then pass "the .sha256 files go up last"; else flunk "the .sha256 files go up last ($first_sha after $last_bundle)"; fi

echo "== Windows: setup.exe"
bundles nsis
run app-v0.1.0
assert_eq "exit 0 ($err)" "0" "$code"
for f in PaliCanon-0.1.0-x86_64-setup.exe PaliCanon-x86_64-setup.exe PaliCanon-0.1.0-x86_64-setup.exe.sha256; do
  if [ -f "$work/out/$f" ]; then pass "$f made"; else flunk "$f made"; fi
done

echo "== a versioned asset already published"
bundles deb
run app-v0.1.0 STUB_EXISTING="PaliCanon-0.1.0-x86_64.deb PaliCanon-x86_64.deb"
assert_eq "a published versioned asset: exit 1" "1" "$code"
assert_match "it names ATTACH_FORCE" "ATTACH_FORCE=1" "$err"
assert_eq "nothing uploaded" "0" "$(grep -c 'release upload' "$log" || true)"
run app-v0.1.0 STUB_EXISTING="PaliCanon-0.1.0-x86_64.deb" ATTACH_FORCE=1
assert_eq "forced: exit 0 ($err)" "0" "$code"
run app-v0.1.0 STUB_EXISTING="PaliCanon-x86_64.deb"
assert_eq "only the stable name exists: replaced freely, exit 0 ($err)" "0" "$code"

echo "== refusals"
bundles deb
run app-v0.2.0
assert_eq "a tag that is not the version: exit 1" "1" "$code"
assert_match "the tag refusal names both" "app-v0.2.0 is not app-v0.1.0" "$err"
run app-v0.1.0 STUB_NO_RELEASE=1
assert_eq "no release: exit 1" "1" "$code"
assert_match "no release: says so" "no release app-v0.1.0" "$err"
bundles
run app-v0.1.0
assert_eq "no bundle: exit 1" "1" "$code"
bundles twodebs
run app-v0.1.0
assert_eq "two candidates: exit 1" "1" "$code"
assert_eq "nothing uploaded on a refusal" "0" "$(grep -c 'release upload' "$log" || true)"
out="$(bash "$script" 2>&1)"
assert_eq "usage: exit 64" "64" "$?"

echo "== wiring"
if grep -q "bash scripts/attach-bundles.test.sh" "$root/.github/workflows/ci.yml"; then pass "ci.yml runs this suite"; else flunk "ci.yml runs this suite"; fi
if grep -q "bash scripts/attach-bundles.sh" "$root/.github/workflows/release-desktop.yml"; then pass "the release workflow calls the script"; else flunk "the release workflow calls the script"; fi

echo
echo "$checks checks, $failed failed"
[ "$failed" -eq 0 ]
