#!/usr/bin/env bash
set -euo pipefail

on_exit() {
  local status=$?
  [ "$status" -eq 0 ] || printf '%s\n' "verify-release: exiting with status $status" >&2
}
trap on_exit EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

usage() {
  printf '%s\n' "usage: verify-release.sh tag TAG" \
    "       verify-release.sh dist TAG DIR TARGET..." >&2
  exit 2
}

fail() {
  printf '%s\n' "verify-release: $*" >&2
  exit 1
}

verify_tag() {
  local tag="$1" metadata versions package_version
  [[ "$tag" == v?* ]] || fail "tag '$tag' must be v followed by the sloplint package version"
  metadata="$(cargo metadata --no-deps --format-version 1 --locked)"
  versions="$(jq -r '.packages[] | select(.name == "sloplint") | .version' <<<"$metadata")"
  [ "$(grep -c . <<<"$versions")" = 1 ] || fail "expected exactly one sloplint package in cargo metadata"
  package_version="$versions"
  [ "${tag#v}" = "$package_version" ] \
    || fail "tag $tag does not match sloplint package version $package_version"
  printf '%s\n' "verify-release: tag $tag matches sloplint package version $package_version"
}

check_checksum_file() {
  local dir="$1" archive="$2" hash="" name="" rest=""
  { read -r hash name rest || true; } <"$dir/$archive.sha256"
  [[ "$hash" =~ ^[0-9a-f]{64}$ ]] || fail "$archive.sha256 does not start with a sha256 digest"
  [ "$name" = "$archive" ] || fail "$archive.sha256 names '$name', not $archive"
  [ -z "$rest" ] || fail "$archive.sha256 has trailing fields"
  [ "$(grep -c . "$dir/$archive.sha256")" = 1 ] || fail "$archive.sha256 must hold exactly one line"
  (cd "$dir" && sha256sum --check --strict --status "$archive.sha256") \
    || fail "$archive does not match $archive.sha256"
}

check_members() {
  local dir="$1" archive="$2" got want
  case "$archive" in
    *.tar.gz)
      got="$(tar -tzf "$dir/$archive" | LC_ALL=C sort)"
      want=$'LICENSE\nsloplint'
      ;;
    *.zip)
      got="$(python3 -c 'import sys, zipfile; print("\n".join(sorted(zipfile.ZipFile(sys.argv[1]).namelist())))' "$dir/$archive")"
      want=$'LICENSE\nsloplint.exe'
      ;;
    *) fail "unsupported archive $archive" ;;
  esac
  [ "$got" = "$want" ] || fail "$archive holds [$(tr '\n' ' ' <<<"$got")] instead of [$(tr '\n' ' ' <<<"$want")]"
}

check_sbom() {
  local dir="$1" sbom="$2" version="$3"
  jq -e --arg version "$version" '
    .bomFormat == "CycloneDX"
    and .specVersion == "1.5"
    and .metadata.component.name == "sloplint"
    and .metadata.component.version == $version
    and ((.components | type) == "array")
    and ((.components | length) > 0)' "$dir/$sbom" >/dev/null 2>&1 \
    || fail "$sbom is not a CycloneDX 1.5 SBOM for sloplint $version"
}

verify_dist() {
  local tag="$1" dir="$2" target ext archive want have
  shift 2
  [ -d "$dir" ] || fail "$dir is not a directory"
  local expected=()
  for target in "$@"; do
    case "$target" in
      *-windows-*) ext=zip ;;
      *) ext=tar.gz ;;
    esac
    expected+=("sloplint-$tag-$target.$ext" "sloplint-$tag-$target.$ext.sha256" "sloplint-$tag-$target.cdx.json")
  done
  want="$(printf '%s\n' "${expected[@]}" | LC_ALL=C sort)"
  have="$(find "$dir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort)"
  if [ "$want" != "$have" ]; then
    diff <(printf '%s\n' "$want") <(printf '%s\n' "$have") >&2 || true
    fail "$dir does not hold exactly the expected archives, checksums, and SBOMs"
  fi
  for archive in "${expected[@]}"; do
    case "$archive" in
      *.sha256) continue ;;
      *.cdx.json) check_sbom "$dir" "$archive" "${tag#v}" ;;
      *)
        check_checksum_file "$dir" "$archive"
        check_members "$dir" "$archive"
        ;;
    esac
  done
  printf '%s\n' "verify-release: $dir holds ${#expected[@]} verified files for $tag"
}

[ "$#" -ge 2 ] || usage
mode="$1"
shift
case "$mode" in
  tag) [ "$#" -eq 1 ] || usage; verify_tag "$@" ;;
  dist) [ "$#" -ge 3 ] || usage; verify_dist "$@" ;;
  *) usage ;;
esac
