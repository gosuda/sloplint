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
mkdir -p "$bin" "$state"
export STUB_STATE="$state"
export PATH="$bin:$PATH"

cat >"$bin/cargo" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
[ ! -e "$STUB_STATE/cargo-fail" ] || exit 101
cat "$STUB_STATE/metadata.json"
STUB
chmod +x "$bin/cargo"

metadata() {
  rm -rf -- "${state:?}"
  mkdir -p "$state"
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
