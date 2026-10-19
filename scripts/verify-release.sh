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
  printf '%s\n' "usage: verify-release.sh tag TAG" >&2
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

[ "$#" -ge 2 ] || usage
mode="$1"
shift
case "$mode" in
  tag) [ "$#" -eq 1 ] || usage; verify_tag "$@" ;;
  *) usage ;;
esac
