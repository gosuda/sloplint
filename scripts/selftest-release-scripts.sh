#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"
passed=0
failed=0

cleanup() {
  rm -rf -- "${work:?}"
}
on_exit() {
  local status=$?
  cleanup
  if [ "$failed" -ne 0 ]; then
    printf '%s\n' "selftest-release-scripts: $failed failed, $passed passed" >&2
    exit 1
  fi
  if [ "$status" -eq 0 ]; then
    printf '%s\n' "selftest-release-scripts: $passed passed"
  fi
}
trap on_exit EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

record() {
  if [ "$1" = ok ]; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
    printf '%s\n' "FAIL: $2" >&2
  fi
}

expect_ok() {
  local name="$1"
  shift
  if "$@" >"$work/out.log" 2>&1; then
    record ok "$name"
  else
    record bad "$name (expected success)"
    sed 's/^/    /' "$work/out.log" >&2
  fi
}

expect_fail() {
  local name="$1"
  shift
  if "$@" >"$work/out.log" 2>&1; then
    record bad "$name (expected failure)"
  else
    record ok "$name"
  fi
}

bin="$work/bin"
state="$work/state"
mkdir -p "$bin" "$work/pub"
export STUB_STATE="$state"
export PATH="$bin:$PATH"

cat >"$bin/cargo" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
[ ! -e "$STUB_STATE/cargo-fail" ] || exit 101
cat "$STUB_STATE/metadata.json"
STUB

cat >"$bin/gh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
state="$STUB_STATE"
printf '%s\n' "$*" >>"$state/calls.log"
edit() {
  jq -c "$@" "$state/releases.json" >"$state/releases.tmp"
  mv "$state/releases.tmp" "$state/releases.json"
}
case "$1" in
  api)
    shift
    path=""
    expr="."
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --paginate | --slurp) shift ;;
        --jq) expr="$2"; shift 2 ;;
        -H) shift 2 ;;
        *) path="$1"; shift ;;
      esac
    done
    fail_if() {
      if [ -e "$state/$1" ]; then
        printf '%s\n' "gh: HTTP 401: Bad credentials" >&2
        exit 1
      fi
    }
    case "$path" in
      repos/*/git/ref/tags/*) fail_if ref-fail; jq -r "$expr" "$state/ref.json" ;;
      repos/*/git/tags/*) jq -r "$expr" "$state/tagobj.json" ;;
      repos/*/releases\?*) fail_if list-fail; jq -c '[.]' "$state/releases.json" ;;
      repos/*/releases/assets/*) cat "$state/blobs/${path##*/}" ;;
      *) exit 1 ;;
    esac
    ;;
  release)
    sub="$2"
    tag="$3"
    case "$sub" in
      create) edit --arg t "$tag" '. + [{id: 1, tag_name: $t, draft: true, assets: []}]' ;;
      upload)
        file="$4"
        name="${file##*/}"
        if jq -e --arg t "$tag" --arg n "$name" '.[] | select(.tag_name == $t) | .assets[] | select(.name == $n)' "$state/releases.json" >/dev/null; then
          printf '%s\n' "HTTP 422: already_exists" >&2
          exit 1
        fi
        id="$(wc -l <"$state/calls.log" | tr -d ' ')"
        digest="${STUB_UPLOAD_DIGEST:-sha256:$(sha256sum "$file" | cut -d' ' -f1)}"
        cp "$file" "$state/blobs/$id"
        edit --arg t "$tag" --arg n "$name" --arg d "$digest" --argjson id "$id" \
          'map(if .tag_name == $t then .assets += [{id: $id, name: $n, state: "uploaded", digest: $d}] else . end)'
        ;;
      delete-asset)
        edit --arg t "$tag" --arg n "$4" \
          'map(if .tag_name == $t then .assets |= map(select(.name != $n)) else . end)'
        ;;
      edit) edit --arg t "$tag" 'map(if .tag_name == $t then .draft = false else . end)' ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB
chmod +x "$bin/cargo" "$bin/gh"

fresh_state() {
  if [ -f "$state/calls.log" ]; then
    cat "$state/calls.log" >>"$work/all-calls.log"
  fi
  rm -rf -- "${state:?}"
  mkdir -p "$state/blobs"
  : >"$state/calls.log"
}

metadata() {
  fresh_state
  printf '%s\n' "$1" >"$state/metadata.json"
}

verify_tag() {
  bash "$here/verify-release.sh" tag "$1"
}

one='{"packages":[{"name":"sloplint","version":"0.1.0"},{"name":"sloplint-core","version":"9.9.9"}]}'
metadata "$one"
expect_ok "tag matches the sloplint package version while another member differs" verify_tag v0.1.0
expect_fail "tag with another version is rejected" verify_tag v0.2.0
expect_fail "tag without v prefix is rejected" verify_tag 0.1.0
expect_fail "empty version after v is rejected" verify_tag v
metadata '{"packages":[{"name":"sloplint-core","version":"0.1.0"}]}'
expect_fail "missing sloplint package is rejected" verify_tag v0.1.0
metadata '{"packages":[{"name":"sloplint","version":"0.1.0"},{"name":"sloplint","version":"0.1.0"}]}'
expect_fail "two sloplint packages are rejected" verify_tag v0.1.0
metadata "$one"
touch "$state/cargo-fail"
expect_fail "cargo metadata failure is not swallowed" verify_tag v0.1.0

targets=(x86_64-unknown-linux-gnu x86_64-pc-windows-msvc)
dist="$work/dist"

sbom_json() {
  jq -n --arg version "$1" '{
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    metadata: {component: {name: "sloplint", version: $version}},
    components: [{name: "clap", version: "4.6.7"}]}'
}

build_dist() {
  local tag="$1" stage="$work/stage" target
  rm -rf -- "${dist:?}" "${stage:?}"
  mkdir -p "$dist" "$stage"
  printf '%s\n' binary >"$stage/sloplint"
  printf '%s\n' binary >"$stage/sloplint.exe"
  printf '%s\n' license >"$stage/LICENSE"
  local unix="sloplint-$tag-x86_64-unknown-linux-gnu.tar.gz" win="sloplint-$tag-x86_64-pc-windows-msvc.zip"
  tar -czf "$dist/$unix" -C "$stage" sloplint LICENSE
  (cd "$dist" && shasum -a 256 "$unix" >"$unix.sha256")
  python3 - "$dist/$win" "$stage" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1], "w") as z:
    z.write(sys.argv[2] + "/sloplint.exe", "sloplint.exe")
    z.write(sys.argv[2] + "/LICENSE", "LICENSE")
PY
  printf '%s  %s' "$(sha256sum "$dist/$win" | cut -d' ' -f1)" "$win" >"$dist/$win.sha256"
  for target in "${targets[@]}"; do
    sbom_json "${tag#v}" >"$dist/sloplint-$tag-$target.cdx.json"
  done
}

verify_dist() {
  bash "$here/verify-release.sh" dist v1.0.0 "$dist" "${targets[@]}"
}

build_dist v1.0.0
expect_ok "complete dist passes, including a checksum file without trailing newline" verify_dist
build_dist v1.0.0
rm "$dist/sloplint-v1.0.0-x86_64-pc-windows-msvc.zip.sha256"
expect_fail "missing checksum file is rejected" verify_dist
build_dist v1.0.0
printf '%s\n' extra >"$dist/stray.txt"
expect_fail "unexpected file is rejected" verify_dist
build_dist v1.0.0
printf '%s' tampered >>"$dist/sloplint-v1.0.0-x86_64-unknown-linux-gnu.tar.gz"
expect_fail "archive that no longer matches its checksum is rejected" verify_dist
build_dist v1.0.0
(cd "$dist" && printf '%s  %s\n' "$(sha256sum sloplint-v1.0.0-x86_64-unknown-linux-gnu.tar.gz | cut -d' ' -f1)" other.tar.gz >sloplint-v1.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256)
expect_fail "checksum file naming another archive is rejected" verify_dist
build_dist v1.0.0
mkdir "$work/stage2"
printf '%s\n' binary >"$work/stage2/sloplint"
tar -czf "$dist/sloplint-v1.0.0-x86_64-unknown-linux-gnu.tar.gz" -C "$work/stage2" sloplint
(cd "$dist" && shasum -a 256 sloplint-v1.0.0-x86_64-unknown-linux-gnu.tar.gz >sloplint-v1.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256)
expect_fail "archive without LICENSE is rejected" verify_dist
build_dist v0.9.0
expect_fail "archives named for another tag are rejected" verify_dist

sbom_path="$dist/sloplint-v1.0.0-x86_64-unknown-linux-gnu.cdx.json"
break_sbom() {
  build_dist v1.0.0
  jq "$1" "$sbom_path" >"$work/sbom.tmp"
  mv -- "$work/sbom.tmp" "$sbom_path"
}

build_dist v1.0.0
rm "$sbom_path"
expect_fail "missing SBOM is rejected" verify_dist
build_dist v1.0.0
printf '%s\n' 'not json' >"$sbom_path"
expect_fail "SBOM that is not JSON is rejected" verify_dist
break_sbom '.bomFormat = "SPDX"'
expect_fail "SBOM with another format is rejected" verify_dist
break_sbom '.specVersion = "1.3"'
expect_fail "SBOM with another spec version is rejected" verify_dist
break_sbom '.metadata.component.name = "other"'
expect_fail "SBOM describing another package is rejected" verify_dist
break_sbom '.metadata.component.version = "9.9.9"'
expect_fail "SBOM describing another version is rejected" verify_dist
break_sbom '.components = []'
expect_fail "SBOM without components is rejected" verify_dist
break_sbom 'del(.components)'
expect_fail "SBOM without a components array is rejected" verify_dist

sha="1111111111111111111111111111111111111111"
export GITHUB_REPOSITORY=owner/repo GITHUB_SHA="$sha"
printf '%s\n' alpha >"$work/pub/a.txt"
printf '%s\n' beta >"$work/pub/b.txt"
sum_a="sha256:$(sha256sum "$work/pub/a.txt" | cut -d' ' -f1)"
sum_b="sha256:$(sha256sum "$work/pub/b.txt" | cut -d' ' -f1)"

workflow="$here/../.github/workflows/release.yml"
yq '.jobs.publish.steps[] | select(.name == "Publish release") | .run' "$workflow" >"$work/publish.sh"
[ -s "$work/publish.sh" ] || { printf '%s\n' "selftest-release-scripts: Publish release step not found in $workflow" >&2; exit 1; }

publish() {
  GITHUB_REF_NAME=v1.0.0 RELEASE_DIST_DIR="$work/pub" bash "$work/publish.sh"
}

scenario() {
  fresh_state
  printf '{"object":{"type":"commit","sha":"%s"}}\n' "$sha" >"$state/ref.json"
  printf '%s\n' "$1" >"$state/releases.json"
}

called() {
  grep -qF -- "$1" "$state/calls.log"
}

asset() {
  printf '{"id":%s,"name":"%s","state":"%s","digest":%s}' "$1" "$2" "$3" "$4"
}

scenario '[]'
expect_ok "absent release is created as draft, filled, and published" publish
if called "release create v1.0.0 --draft --verify-tag" && called "release edit v1.0.0 --draft=false"; then
  record ok "creation uses a verified-tag draft and ends with publication"
else
  record bad "creation flow calls"
fi

scenario "[{\"id\":7,\"tag_name\":\"v1.0.0\",\"draft\":true,\"assets\":[$(asset 11 a.txt uploaded "\"$sum_a\"")]}]"
printf 'alpha\n' >"$state/blobs/11"
expect_ok "draft with one identical asset completes the rest" publish
if ! called "release upload v1.0.0 $work/pub/a.txt" && called "release upload v1.0.0 $work/pub/b.txt"; then
  record ok "identical asset is skipped, missing asset uploaded"
else
  record bad "skip-or-upload decision"
fi

scenario "[{\"id\":7,\"tag_name\":\"v1.0.0\",\"draft\":true,\"assets\":[$(asset 11 a.txt uploaded '"sha256:deadbeef"')]}]"
expect_fail "draft asset with different content fails" publish
if ! called "release upload" && ! called "release edit" && ! called "delete-asset"; then
  record ok "mismatch changes nothing"
else
  record bad "mismatch mutated the release"
fi

scenario "[{\"id\":7,\"tag_name\":\"v1.0.0\",\"draft\":false,\"assets\":[$(asset 11 a.txt uploaded "\"$sum_a\""),$(asset 12 b.txt uploaded "\"$sum_b\"")]}]"
expect_ok "published release with identical assets is a no-op" publish
if ! called "release create" && ! called "release upload" && ! called "release edit"; then
  record ok "published release is untouched"
else
  record bad "published release was modified"
fi

scenario "[{\"id\":7,\"tag_name\":\"v1.0.0\",\"draft\":false,\"assets\":[$(asset 11 a.txt uploaded "\"$sum_a\"")]}]"
expect_fail "published release missing an asset fails" publish
if ! called "release upload"; then
  record ok "published release gets no upload"
else
  record bad "uploaded into a published release"
fi

scenario '[]'
touch "$state/list-fail"
expect_fail "release listing failure is not treated as a missing release" publish
if ! called "release create"; then
  record ok "no draft is created after a listing failure"
else
  record bad "created a release after listing failure"
fi

scenario '[]'
touch "$state/ref-fail"
expect_fail "tag lookup failure stops before any release call" publish
if ! called "release create"; then
  record ok "no draft is created after a tag lookup failure"
else
  record bad "created a release after tag lookup failure"
fi

scenario '[]'
printf '{"object":{"type":"commit","sha":"%s"}}\n' 2222222222222222222222222222222222222222 >"$state/ref.json"
expect_fail "tag that moved off the built commit fails" publish
if ! called "release create"; then
  record ok "moved tag blocks creation"
else
  record bad "created a release for a moved tag"
fi

scenario '[]'
printf '{"object":{"type":"tag","sha":"3333333333333333333333333333333333333333"}}\n' >"$state/ref.json"
printf '{"object":{"type":"commit","sha":"%s"}}\n' "$sha" >"$state/tagobj.json"
expect_ok "annotated tag is dereferenced to its commit" publish

scenario "[{\"id\":7,\"tag_name\":\"v1.0.0\",\"draft\":true,\"assets\":[$(asset 11 a.txt starter null)]}]"
expect_ok "incomplete draft asset is replaced" publish
if called "release delete-asset v1.0.0 a.txt --yes" && called "release upload v1.0.0 $work/pub/a.txt"; then
  record ok "incomplete asset is deleted then re-uploaded"
else
  record bad "incomplete asset handling"
fi

scenario "[{\"id\":7,\"tag_name\":\"v1.0.0\",\"draft\":true,\"assets\":[$(asset 11 old.txt uploaded '"sha256:00"')]}]"
expect_fail "unexpected asset on the draft fails" publish
if ! called "release edit"; then
  record ok "unexpected asset blocks publication"
else
  record bad "published with unexpected asset"
fi

scenario '[{"id":7,"tag_name":"v1.0.0","draft":true,"assets":[]},{"id":8,"tag_name":"v1.0.0","draft":true,"assets":[]}]'
expect_fail "duplicate releases for one tag fail" publish

scenario '[]'
export STUB_UPLOAD_DIGEST="sha256:ffff"
expect_fail "upload that lands with another digest fails" publish
unset STUB_UPLOAD_DIGEST
if ! called "release edit"; then
  record ok "bad upload digest blocks publication"
else
  record bad "published after bad digest"
fi

scenario "[{\"id\":7,\"tag_name\":\"v1.0.0\",\"draft\":true,\"assets\":[$(asset 11 a.txt uploaded null)]}]"
printf 'alpha\n' >"$state/blobs/11"
expect_ok "asset without API digest is compared by downloading it" publish
if ! called "release upload v1.0.0 $work/pub/a.txt"; then
  record ok "downloaded identical asset is skipped"
else
  record bad "re-uploaded identical asset"
fi

fresh_state
if grep -qF -- "--clobber" "$work/all-calls.log"; then
  record bad "a publish scenario passed --clobber"
else
  record ok "no publish scenario ever passes --clobber"
fi
