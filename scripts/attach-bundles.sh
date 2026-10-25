#!/usr/bin/env bash
# Names, hashes and attaches the Windows or Linux bundles of a release
# (task 013, called by .github/workflows/release-desktop.yml).
#
#   scripts/attach-bundles.sh <tag> <bundle-dir>
#
# <tag> is the release (app-v<version>); the version is read from it and
# must equal crates/desktop/tauri.conf.json's. Under <bundle-dir> it finds
# what Tauri made:
#
#   deb/*.deb            -> PaliCanon-<version>-x86_64.deb       and PaliCanon-x86_64.deb
#   appimage/*.AppImage  -> PaliCanon-<version>-x86_64.AppImage  and PaliCanon-x86_64.AppImage
#   nsis/*-setup.exe     -> PaliCanon-<version>-x86_64-setup.exe and PaliCanon-x86_64-setup.exe
#
# Each versioned file gets a `.sha256` beside it, and the run a
# `BUILD-INFO-<kind>.txt` (the tag, the commit, the date): the Windows and
# Linux bundles may come from a later commit than the macOS one. The stable
# names are replaced freely; a versioned name already on the release is
# refused unless ATTACH_FORCE=1 (a published hash never changes silently).
# The .sha256 files go up last. Exit 1 when the tag and the version differ,
# when the release does not exist, when a versioned asset exists without
# ATTACH_FORCE, or when the directory holds none of the three bundles; 64
# for a usage error.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: scripts/attach-bundles.sh <tag> <bundle-dir>" >&2
  exit 64
fi
tag=$1
dir=$2
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="${ATTACH_ROOT:-$(cd "$here/.." && pwd)}"
repo="${ATTACH_REPO:-PaliCanon/app}"
out="${ATTACH_OUT:-$dir/attach}"
commit="${ATTACH_COMMIT:-${GITHUB_SHA:-$(git -C "$root" rev-parse HEAD 2>/dev/null || echo unknown)}}"

refuse() {
  echo "attach-bundles.sh: $1" >&2
  exit 1
}

version="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$root/crates/desktop/tauri.conf.json")"
[ "$tag" = "app-v$version" ] || refuse "the tag $tag is not app-v$version (tauri.conf.json)"
existing="$(gh release view "$tag" --repo "$repo" --json assets --jq '.assets[].name' 2>/dev/null)" || refuse "no release $tag on $repo: run scripts/release.sh first"

sha_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

mkdir -p "$out"
found=0
# one PATTERN SUFFIX: the single bundle matching PATTERN, copied to its two names.
one() {
  local pattern=$1 suffix=$2 files=()
  shopt -s nullglob
  files=($pattern)
  shopt -u nullglob
  [ "${#files[@]}" -le 1 ] || refuse "more than one file matches $pattern"
  [ "${#files[@]}" -eq 1 ] || return 0
  local versioned="PaliCanon-$version-x86_64$suffix" stable="PaliCanon-x86_64$suffix"
  cp "${files[0]}" "$out/$versioned"
  cp "${files[0]}" "$out/$stable"
  echo "$(sha_of "$out/$versioned")  $versioned" >"$out/$versioned.sha256"
  found=$((found + 1))
}
one "$dir/deb/*.deb" ".deb"
one "$dir/appimage/*.AppImage" ".AppImage"
one "$dir/nsis/*-setup.exe" "-setup.exe"
[ "$found" -gt 0 ] || refuse "no deb, AppImage or setup.exe under $dir"
kinds="$(ls "$out" | sed -nE 's/^PaliCanon-x86_64(.*)$/\1/p' | tr -d '.' | tr '\n' '-' | sed 's/-$//')"
printf 'tag %s\ncommit %s\nbuilt %s\n' "$tag" "$commit" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$out/BUILD-INFO-$kinds.txt"

# A versioned name already published is refused unless forced.
for f in "$out"/PaliCanon-"$version"-*; do
  name="$(basename "$f")"
  if grep -qxF -- "$name" <<<"$existing" && [ "${ATTACH_FORCE:-}" != 1 ]; then
    refuse "$name is already on $tag: set ATTACH_FORCE=1 to replace a published asset"
  fi
done
upload() { gh release upload "$tag" --repo "$repo" --clobber "$1" >/dev/null && echo "attached $(basename "$1")"; }
for f in "$out"/*; do
  case "$f" in *.sha256) ;; *) upload "$f" ;; esac
done
for f in "$out"/*.sha256; do upload "$f"; done
cat "$out"/*.sha256
