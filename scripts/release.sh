#!/usr/bin/env bash
# The public macOS release (task 007): build, sign, notarize, staple, verify,
# then a GitHub Release on PaliCanon/app with a stable download link.
#
#   scripts/release.sh build     everything up to a DRAFT release holding the
#                                two DMG assets (nothing public yet)
#   scripts/release.sh publish   checks the release's assets, publishes it as
#                                the latest (the last write) and verifies the
#                                stable link answers with the asset's size
#
# Runs on the user's Mac: the Developer ID Application identity, the notary
# profile (`xcrun notarytool store-credentials palicanon-notary`) and the
# Desktop OAuth secret live in the user's Keychain. It reads them inline and
# prints none of them (the build reads the secret itself: crates/desktop/
# build.rs; the gate only checks it landed in the binary). Nothing is written
# outside the repository's `target/` and the GitHub Release.
#
# Order the design fixes (deep, review of 2026-10-22): the .app is stapled
# BEFORE the DMG is made from it; the SHA-256 is taken AFTER the DMG is
# stapled (stapling rewrites the file); the release is a draft until `publish`;
# a published release for the version aborts (a version is released once); a
# draft left by an aborted run is deleted and redone, since nobody saw it.
#
# Exit 1 with a message on stderr when a preflight or a gate refuses; 64 for
# a usage error. Every external tool is called by name so a test can stand in
# for it on PATH (scripts/release.test.sh).
set -euo pipefail

mode="${1:-}"
if [ "$#" -ne 1 ] || { [ "$mode" != build ] && [ "$mode" != publish ]; }; then
  echo "usage: scripts/release.sh build|publish" >&2
  exit 64
fi

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="${RELEASE_ROOT:-$(cd "$here/.." && pwd)}"
repo="${RELEASE_REPO:-PaliCanon/app}"
profile="${RELEASE_NOTARY_PROFILE:-palicanon-notary}"
scratch="${RELEASE_SCRATCH:-$root/target/release-scratch}"
bundle_dir="$root/target/release/bundle"
app="$bundle_dir/macos/PaliCanon.app"
app_name="PaliCanon"
arch="aarch64"
mounted=""
scratch_err="$(mktemp)"

refuse() {
  echo "release.sh: $1" >&2
  exit 1
}
step() { echo "== $1"; }
# Searches a command's whole output. Never pipe a command into grep -q: under
# pipefail, grep -q exits at the first match, the command dies of SIGPIPE
# and the pipeline fails (the first real run, 2026-10-25: a signed,
# timestamped app refused as "no timestamp").
has() { # pattern text
  grep -Eq -- "$1" <<<"$2"
}
# A refusal while the dmg is attached leaves nothing mounted behind.
cleanup() {
  if [ -n "$mounted" ]; then hdiutil detach "$mounted" >/dev/null 2>&1 || true; fi
  rm -f "$scratch_err"
}
trap cleanup EXIT

# --- the version: tauri.conf.json is the source, the workspace must agree ----
version="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$root/crates/desktop/tauri.conf.json")"
workspace_version="$(python3 - "$root/Cargo.toml" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
section = re.search(r"^\[workspace\.package\]\n(.*?)(?=^\[|\Z)", text, re.S | re.M)
m = re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M) if section else None
print(m.group(1) if m else "")
PY
)"
[ -n "$version" ] || refuse "no version in crates/desktop/tauri.conf.json"
[ -n "$workspace_version" ] || refuse "no [workspace.package] version in Cargo.toml"
[ "$version" = "$workspace_version" ] || refuse "version drift: tauri.conf.json says $version, Cargo.toml's workspace says $workspace_version"
grep -Eq '^version\.workspace = true$' "$root/crates/desktop/Cargo.toml" || refuse "crates/desktop/Cargo.toml does not take the workspace version"
tag="app-v$version"
versioned="$app_name-$version-$arch.dmg"
stable="$app_name-$arch.dmg"
stable_link="https://github.com/$repo/releases/latest/download/$stable"

# --- preflight ---------------------------------------------------------------
preflight_tree() {
  step "preflight: the tree"
  [ -z "$(git -C "$root" status --porcelain)" ] || refuse "the working tree is not clean"
  [ "$(git -C "$root" rev-parse --abbrev-ref HEAD)" = main ] || refuse "not on main"
  git -C "$root" fetch --quiet origin main
  [ "$(git -C "$root" rev-parse HEAD)" = "$(git -C "$root" rev-parse origin/main)" ] || refuse "HEAD is not origin/main (push first)"
}

# A published release for the version is final. A draft is an aborted run's
# leftover (a draft is never public): it is deleted and the release redone.
preflight_tag() {
  step "preflight: $tag is not released"
  [ -z "$(git -C "$root" tag --list "$tag")" ] || refuse "the tag $tag exists locally"
  [ -z "$(git -C "$root" ls-remote --tags origin "refs/tags/$tag")" ] || refuse "the tag $tag exists on origin"
  # A draft creates no tag on GitHub (the tag is made when it is published),
  # so a leftover draft passes the two tag checks above and is found here.
  # Only "not found" means no release: any other failure of gh is a refusal.
  local json err
  if json="$(gh release view "$tag" --repo "$repo" --json isDraft 2>"$scratch_err")"; then
    if printf '%s' "$json" | python3 -c 'import json,sys; sys.exit(0 if json.load(sys.stdin).get("isDraft") else 1)'; then
      echo "a draft $tag is left from an aborted run: deleted, redone"
      gh release delete "$tag" --repo "$repo" --yes >/dev/null
    else
      refuse "a release $tag is published: bump the version, never republish"
    fi
  else
    err="$(cat "$scratch_err")"
    grep -qi "not found" <<<"$err" || refuse "gh could not read the releases of $repo: ${err:-no message}"
  fi
}

identity=""
preflight_identity() {
  step "preflight: the signing identity"
  local lines
  lines="$(security find-identity -v -p codesigning | grep "Developer ID Application" || true)"
  [ "$(printf '%s\n' "$lines" | grep -c .)" = 1 ] || refuse "exactly one Developer ID Application identity is needed in the Keychain"
  identity="$(printf '%s\n' "$lines" | sed -nE 's/^ *[0-9]+\) ([0-9A-F]{40}) .*/\1/p')"
  [ -n "$identity" ] || refuse "the identity line has no hash"
}

preflight_notary() {
  step "preflight: the notary profile $profile"
  xcrun notarytool history --keychain-profile "$profile" >/dev/null 2>&1 || refuse "the notary profile $profile does not answer (xcrun notarytool store-credentials $profile)"
}

oauth_secret() {
  security find-generic-password -a palicanon -s google-oauth-desktop-secret -w 2>/dev/null || true
}
preflight_oauth() {
  step "preflight: the Desktop OAuth secret"
  [ -n "$(oauth_secret)" ] || refuse "the Keychain item google-oauth-desktop-secret is missing: the app would ship with no sync"
}

# --- build -------------------------------------------------------------------
build_app() {
  step "build: a fresh bundle folder, the app unsigned, the home path remapped"
  rm -rf "${bundle_dir:?}" "${scratch:?}"
  mkdir -p "$scratch"
  (
    cd "$root/crates/desktop"
    RUSTFLAGS="--remap-path-prefix=$HOME=/build ${RUSTFLAGS:-}" \
      cargo tauri build --bundles app --config tauri.bundle.conf.json --no-sign
  )
  [ -d "$app" ] || refuse "no app at $app after the build"
}

# The binary built here carries the front (Tauri embeds the dist) and the
# secret build.rs read from the Keychain. The sidecar is the author's build:
# its paths are theirs (a yuga-releases follow-up), only its architecture is
# checked.
gate_binary() {
  step "gate: the built binary"
  local bin="$app/Contents/MacOS/palicanon-app-desktop" sidecar="$app/Contents/MacOS/yuga-sidecar"
  [ -f "$bin" ] || refuse "no binary at $bin"
  [ -f "$sidecar" ] || refuse "no sidecar at $sidecar"
  # The secret is compared through a file descriptor, never an argument.
  grep -a -q -F -f <(oauth_secret) "$bin" || refuse "the OAuth secret is not in the binary: the build read no Keychain item"
  has "arm64" "$(file "$bin")" || refuse "the binary is not arm64"
  has "arm64" "$(file "$sidecar")" || refuse "the sidecar is not arm64"
  if grep -a -q -F -- "/Users/" "$bin"; then refuse "a /Users/ path is in the binary (the remap did not apply)"; fi
  if grep -q -F -- "/Users/" "$app/Contents/Info.plist"; then refuse "a /Users/ path is in Info.plist"; fi
}

# --- sign, notarize, staple ---------------------------------------------------
sign_path() { # path
  codesign --force --timestamp --options runtime -s "$identity" "$1"
}
sign_app() {
  step "sign: the sidecar, then the app (inside out)"
  sign_path "$app/Contents/MacOS/yuga-sidecar"
  sign_path "$app"
  codesign --verify --deep --strict "$app"
  has '^Timestamp=' "$(codesign -dvv "$app" 2>&1)" || refuse "the app's signature has no timestamp"
}

notarize() { # path label
  step "notarize: $2"
  local out status id
  out="$(xcrun notarytool submit "$1" --keychain-profile "$profile" --wait --output-format json)"
  status="$(printf '%s' "$out" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status",""))')"
  id="$(printf '%s' "$out" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("id",""))')"
  if [ "$status" != Accepted ]; then
    [ -n "$id" ] && xcrun notarytool log "$id" --keychain-profile "$profile" "$scratch/notary-$2.json" >/dev/null 2>&1 || true
    refuse "notarization of $2 ended '$status' (the log, if any: $scratch/notary-$2.json)"
  fi
}
staple() { # path
  xcrun stapler staple "$1" >/dev/null
  xcrun stapler validate "$1" >/dev/null
}

notarize_app() {
  local zip="$scratch/$app_name.zip"
  ditto -c -k --keepParent "$app" "$zip"
  notarize "$zip" app
  step "staple: the app"
  staple "$app"
}

make_dmg() {
  step "dmg: from the stapled app"
  local staging="$scratch/dmg"
  mkdir -p "$staging"
  ditto "$app" "$staging/$app_name.app"
  ln -s /Applications "$staging/Applications"
  hdiutil create -volname "$app_name" -srcfolder "$staging" -ov -format UDZO "$scratch/$versioned" >/dev/null
  sign_path "$scratch/$versioned"
  notarize "$scratch/$versioned" dmg
  step "staple: the dmg"
  staple "$scratch/$versioned"
}

verify_dmg() {
  step "verify: Gatekeeper on the dmg and on the app it holds"
  has "source=Notarized Developer ID" "$(spctl -a -t open --context context:primary-signature -vv "$scratch/$versioned" 2>&1 || true)" || refuse "spctl does not accept the dmg as Notarized Developer ID"
  local mount="$scratch/mnt"
  mkdir -p "$mount"
  hdiutil attach -nobrowse -readonly -mountpoint "$mount" "$scratch/$versioned" >/dev/null
  mounted="$mount"
  local ok=0
  has "source=Notarized Developer ID" "$(spctl -a -t exec -vv "$mount/$app_name.app" 2>&1 || true)" && ok=1
  hdiutil detach "$mount" >/dev/null
  mounted=""
  [ "$ok" = 1 ] || refuse "spctl does not accept the app inside the dmg"
}

sha256=""
size_bytes=""
sha_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
measure() {
  step "measure: the SHA-256 after the staple, the stable copy checked against it"
  sha256="$(sha_of "$scratch/$versioned")"
  size_bytes="$(wc -c <"$scratch/$versioned" | tr -d ' ')"
  cp "$scratch/$versioned" "$scratch/$stable"
  [ "$(sha_of "$scratch/$stable")" = "$sha256" ] || refuse "the stable copy differs from the versioned dmg"
}

write_notes() {
  local mib=$(( (size_bytes + 524288) / 1048576 ))
  sed -e "s/@VERSION@/$version/g" -e "s/@SIZE_MIB@/$mib/g" -e "s/@SHA256@/$sha256/g" \
    "$here/release-notes.md" >"$scratch/notes.md"
}

draft_release() {
  step "release: a draft $tag with the two assets"
  write_notes
  gh release create "$tag" --repo "$repo" --draft --target "$(git -C "$root" rev-parse HEAD)" \
    --title "$app_name $version (macOS, Apple silicon)" --notes-file "$scratch/notes.md" >/dev/null
  gh release upload "$tag" --repo "$repo" "$scratch/$versioned" >/dev/null
  gh release upload "$tag" --repo "$repo" "$scratch/$stable" >/dev/null
  release_json draft >/dev/null
  echo "draft $tag ready: $versioned and $stable, $size_bytes bytes, sha256 $sha256"
  echo "check the dmg at $scratch/$versioned, then: scripts/release.sh publish"
}

# The release's two assets, read back from the API: present, the same
# non-zero size (the built size when known). `draft` also requires a draft.
# Prints the JSON for the caller.
release_json() { # draft|any
  local json
  json="$(gh release view "$tag" --repo "$repo" --json isDraft,assets)"
  RELEASE_JSON="$json" python3 - "$versioned" "$stable" "${size_bytes:-}" "$1" <<'PY' || refuse "the release $tag does not hold the two assets as built"
import json, os, sys
d = json.loads(os.environ["RELEASE_JSON"])
if sys.argv[4] == "draft" and not d.get("isDraft"):
    sys.exit("the release is not a draft")
sizes = {a["name"]: a["size"] for a in d.get("assets", [])}
want = sys.argv[3]
for name in sys.argv[1:3]:
    if name not in sizes or sizes[name] <= 0:
        sys.exit(f"asset {name} missing or empty")
    if want and str(sizes[name]) != want:
        sys.exit(f"asset {name}: size {sizes[name]} is not {want}")
if sizes[sys.argv[1]] != sizes[sys.argv[2]]:
    sys.exit("the two assets differ in size")
PY
  printf '%s' "$json"
}
# The latest release's tag, empty when the repository has none (gh release
# view has no isLatest field: the first real run, 2026-10-25).
latest_tag() {
  gh api "repos/$repo/releases/latest" --jq .tag_name 2>/dev/null || true
}
json_field() { # json field
  printf '%s' "$1" | python3 -c 'import json,sys; v=json.load(sys.stdin).get(sys.argv[1]); print("true" if v is True else "false" if v is False else v)' "$2"
}

# Publishing is idempotent: a release already public but not the latest (an
# edit that half failed) is finished, not refused. The stable link is the
# promise: it is read with its size before the script says it is published.
publish_release() {
  step "publish: $tag, the last write"
  local json size
  json="$(release_json any)"
  # A failed edit is not fatal here: the release is read back and judged.
  if [ "$(json_field "$json" isDraft)" = true ]; then
    gh release edit "$tag" --repo "$repo" --draft=false --latest >/dev/null || echo "the edit failed: reading the release back"
  elif [ "$(latest_tag)" != "$tag" ]; then
    gh release edit "$tag" --repo "$repo" --latest >/dev/null || echo "the edit failed: reading the release back"
  fi
  json="$(release_json any)"
  [ "$(json_field "$json" isDraft)" = false ] || refuse "the release is still a draft after the edit"
  [ "$(latest_tag)" = "$tag" ] || refuse "the release is not the latest after the edit"
  size="$(printf '%s' "$json" | python3 -c 'import json,sys; d=json.load(sys.stdin); print([a["size"] for a in d["assets"] if a["name"]==sys.argv[1]][0])' "$stable")"
  step "verify: the stable link"
  local headers
  headers="$(curl -sIL "$stable_link")"
  headers="$(tr -d '\r' <<<"$headers")"
  has '^HTTP/[0-9.]+ 200' "$headers" || refuse "the stable link does not answer 200: $stable_link"
  has "^[Cc]ontent-[Ll]ength: $size\$" "$headers" || refuse "the stable link's content-length is not $size"
  echo "published: $stable_link"
}

case "$mode" in
  build)
    preflight_tree
    preflight_tag
    preflight_identity
    preflight_notary
    preflight_oauth
    build_app
    gate_binary
    sign_app
    notarize_app
    make_dmg
    verify_dmg
    measure
    draft_release
    ;;
  publish)
    publish_release
    ;;
esac
