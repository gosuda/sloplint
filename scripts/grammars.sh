#!/usr/bin/env bash
# Grammar pin manifest tooling for sloplint's vendored tree-sitter grammars.
#
# check  (offline): validate vendor/grammars.toml against vendor/.
# verify (network): reproduce a vendored tree from its pinned upstream commit.
# bump   (network, read-only by default): show the steps to follow upstream.
#
# TOML parsing uses python3 (tomllib, stdlib since 3.11), already present on
# the host. No tomlq is used. The offline gate is this shell check (not a
# Rust test) because it needs no workspace build, runs on every CI runner
# with bash plus python3, stays offline, and matches scripts/ convention.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/vendor/grammars.toml"
# UNRESOLVED is allowed only for these names (documented in grammars.toml).
ALLOWED_UNRESOLVED="tree-sitter-logos"

die() {
  printf 'grammars.sh: %s\n' "$*" >&2
  exit 1
}

need_file() {
  if [ ! -f "$1" ]; then
    die "missing file: $1"
  fi
}

# Print VAR=value lines for one manifest entry.
entry_field() {
  local name="$1"
  local field="$2"
  MANIFEST_PATH="$MANIFEST" ENTRY_NAME="$name" ENTRY_FIELD="$field" python3 - <<'EOF'
import os, tomllib
with open(os.environ["MANIFEST_PATH"], "rb") as f:
    data = tomllib.load(f)
for g in data["grammar"]:
    if g["name"] == os.environ["ENTRY_NAME"]:
        print(g.get(os.environ["ENTRY_FIELD"], ""))
        break
EOF
}

cmd_check() {
  need_file "$MANIFEST"
  MANIFEST_PATH="$MANIFEST" REPO_ROOT="$ROOT" ALLOWED="$ALLOWED_UNRESOLVED" python3 - <<'EOF'
import os, re, sys, tomllib

root = os.environ["REPO_ROOT"]
with open(os.environ["MANIFEST_PATH"], "rb") as f:
    data = tomllib.load(f)
entries = data["grammar"]
allowed = set(os.environ["ALLOWED"].split())

errors = []
names = [g.get("name", "") for g in entries]
if len(set(names)) != len(names):
    errors.append("manifest has duplicate grammar names")

vendored = {}
for g in entries:
    if g.get("kind") in ("vendored-patched", "vendored-pristine", "own"):
        vendored[g["name"]] = g

import glob
dirs = sorted(os.path.basename(d) for d in glob.glob(os.path.join(root, "vendor", "tree-sitter-*")) if os.path.isdir(d))
for d in dirs:
    if d not in vendored:
        errors.append(f"vendor/{d} has no manifest entry")
for n in sorted(vendored):
    if n not in dirs:
        errors.append(f"manifest entry {n} has no vendor/{n} directory")

hex40 = re.compile(r"^[0-9a-f]{40}$")
for g in entries:
    n = g.get("name", "?")
    kind = g.get("kind", "")
    if kind not in ("crate-exact", "vendored-pristine", "vendored-patched", "own", "crate"):
        errors.append(f"{n}: unknown kind {kind!r}")
        continue
    commit = g.get("commit", "")
    if hex40.match(commit):
        pass
    elif commit == "UNRESOLVED" and n in allowed:
        pass
    else:
        errors.append(f"{n}: commit {commit!r} is not 40-hex lowercase (UNRESOLVED allowed only for {sorted(allowed)})")
    status = g.get("patch_status", "")
    if status not in ("none", "local", "upstream-pending"):
        errors.append(f"{n}: bad patch_status {status!r}")
        continue
    patch = g.get("patch", "")
    if status == "none":
        if patch != "":
            errors.append(f"{n}: patch_status none but patch is {patch!r}")
    else:
        if patch == "":
            errors.append(f"{n}: patch_status {status} but patch is empty")
        else:
            p = os.path.join(root, patch)
            if not os.path.isfile(p):
                errors.append(f"{n}: patch file missing: {patch}")
            elif os.path.getsize(p) == 0:
                errors.append(f"{n}: patch file is empty: {patch}")
        if status == "upstream-pending" and not g.get("patch_upstream", ""):
            errors.append(f"{n}: patch_status upstream-pending but patch_upstream is empty")
        if status == "local" and not g.get("notes", ""):
            errors.append(f"{n}: patch_status local but notes are empty")
    if not g.get("language", ""):
        errors.append(f"{n}: language is empty")
    if kind == "crate":
        continue
    cargo = os.path.join(root, "vendor", n, "Cargo.toml")
    if not os.path.isfile(cargo):
        errors.append(f"{n}: missing vendor/{n}/Cargo.toml")
        continue
    if g.get("crate", ""):
        name = version = ""
        with open(cargo, encoding="utf-8") as f:
            for line in f:
                s = line.strip()
                if s.startswith("name =") and not name:
                    name = s.split('"')[1]
                elif s.startswith("version =") and not version:
                    version = s.split('"')[1]
                if name and version:
                    break
        if g["crate"] != name or g.get("crate_version", "") != version:
            errors.append(f"{n}: manifest crate {g.get('crate')}/{g.get('crate_version')} != Cargo.toml {name}/{version}")
    if kind == "crate":
        pass

# The crates.io dependency entry (kind crate) has no vendor dir; check the lock.
for g in entries:
    if g.get("kind") != "crate":
        continue
    lock = os.path.join(root, "Cargo.lock")
    if not os.path.isfile(lock):
        errors.append(f"{g['name']}: missing Cargo.lock for crate check")
        continue
    want = g.get("crate_version", "")
    with open(lock, "rb") as f:
        lockdata = tomllib.load(f)
    pkgs = [p for p in lockdata.get("package", []) if p.get("name") == g.get("crate", "")]
    if not pkgs:
        errors.append(f"{g['name']}: crate {g.get('crate')} not in Cargo.lock")
    elif want and pkgs[0].get("version") != want:
        errors.append(f"{g['name']}: Cargo.lock pins {g.get('crate')} {pkgs[0].get('version')}, manifest wants {want}")

for e in errors:
    print(f"grammars.sh check: {e}")
sys.exit(1 if errors else 0)
EOF
}

cmd_verify() {
  local name="${1:-}"
  [ -n "$name" ] || die "usage: grammars.sh verify <name>"
  need_file "$MANIFEST"
  local kind upstream commit generator patch
  kind="$(entry_field "$name" "kind")"
  [ -n "$kind" ] || die "no manifest entry for $name"
  if [ "$kind" = "crate" ]; then
    die "$name is kind crate (no vendor directory to reproduce)"
  fi
  upstream="$(entry_field "$name" "upstream")"
  commit="$(entry_field "$name" "commit")"
  generator="$(entry_field "$name" "generator")"
  patch="$(entry_field "$name" "patch")"
  [ -n "$upstream" ] || die "$name has no upstream (own dialect; verify its base instead)"
  case "$commit" in
    UNRESOLVED|"") die "$name has no pinned commit to verify" ;;
  esac
  command -v git >/dev/null || die "git not found"
  local tsbin="${GRAMMARS_TS_BIN:-tree-sitter}"
  command -v "$tsbin" >/dev/null || die "tree-sitter CLI not found: $tsbin (set GRAMMARS_TS_BIN)"
  # Generation runs in scratch subdirectories, where a caller-relative
  # override no longer resolves. Pin a bare command on PATH as is and
  # resolve any path override to absolute before changing directories.
  case "$tsbin" in
    */*) tsbin="$(cd "$(dirname "$tsbin")" && pwd)/$(basename "$tsbin")" ;;
  esac
  if [ -n "$generator" ]; then
    tsver="$("$tsbin" --version 2>&1)"
    # Release builds may append a parenthesized build SHA; match the release
    # number literally and allow only that suffix form.
    case "$tsver" in
      "tree-sitter $generator"|"tree-sitter $generator ("*")") ;;
      *) die "CLI version mismatch: entry wants tree-sitter $generator, got $tsver" ;;
    esac
  fi
  local scratch="${GRAMMARS_SCRATCH:-/dev/shm/grammars-verify}"
  mkdir -p "$scratch"
  # Unique per-run directory (mktemp mode 0700): a predictable shared path
  # lets another local user alter the checkout between fetch and generate.
  local work
  work="$(mktemp -d "$scratch/$name-XXXXXX")"
  git init -q "$work/upstream"
  git -C "$work/upstream" remote add origin "$upstream"
  git -C "$work/upstream" fetch --depth 1 origin "$commit" 2>"$work/fetch.err" || {
    cat "$work/fetch.err" >&2
    die "$name: cannot fetch commit $commit from $upstream"
  }
  git -C "$work/upstream" checkout -q FETCH_HEAD
  if [ -n "$patch" ]; then
    # Patches are relative to the vendor directory root, which matches the
    # upstream checkout root. Most patches use git a/b prefixes (-p1); the
    # ocaml patch is p0-style with timestamps and the dafny patch carries
    # sloplint-relative a/vendor/... paths (-p3). Try each level in turn.
    local patch_level=""
    local try_level=""
    for try_level in -p1 -p0 -p3; do
      if git -C "$work/upstream" apply --check "$try_level" < "$ROOT/$patch" 2>"$work/apply-err"; then
        patch_level="$try_level"
        break
      fi
    done
    if [ -z "$patch_level" ]; then
      # Show the -p1 errors without a pipe: under pipefail a failing check
      # would kill the shell before die names the pin.
      git -C "$work/upstream" apply --check -p1 < "$ROOT/$patch" >"$work/apply-err" 2>&1 || true
      head -3 "$work/apply-err" >&2
      die "$name: patch $patch does not apply to $commit"
    fi
    if [ "$patch_level" != "-p1" ]; then
      printf 'grammars.sh verify: %s: patch uses %s strip level\n' "$name" "$patch_level" >&2
    fi
    git -C "$work/upstream" apply "$patch_level" < "$ROOT/$patch"
  fi
  # A patched grammar may extend a sibling vendored grammar (typescript
  # extends ../../tree-sitter-javascript/grammar.js). Link each referenced
  # sibling from vendor/ read-only so generate resolves it exactly as the
  # vendored tree did.
  while IFS= read -r sib; do
    if [ -d "$ROOT/vendor/$sib" ] && [ ! -e "$work/$sib" ]; then
      ln -s "$ROOT/vendor/$sib" "$work/$sib"
      printf 'grammars.sh verify: %s: linked sibling %s from vendor/\n' "$name" "$sib" >&2
    fi
  done < <(cd "$work/upstream" && grep -rhoE '\.\./\.\./tree-sitter-[a-z0-9-]+/grammar\.js' --include='*.js' . 2>/dev/null | sed -E 's|^\.\./\.\./||; s|/grammar\.js$||' | sort -u)
  # A grammar may resolve its base through node_modules (cpp and starlark
  # commit theirs) or through a package-style require of a vendored base
  # (objc uses the patched vendored C grammar per its SLOPLINT.md). Link a
  # committed node_modules read-only; otherwise fall back to NODE_PATH so
  # the documented base resolves exactly as the vendored tree did.
  if [ -d "$ROOT/vendor/$name/node_modules" ] && [ ! -e "$work/upstream/node_modules" ]; then
    ln -s "$ROOT/vendor/$name/node_modules" "$work/upstream/node_modules"
    printf 'grammars.sh verify: %s: linked node_modules from vendor/\n' "$name" >&2
  elif (cd "$work/upstream" && grep -rqE "require\(['\"]tree-sitter-|from ['\"]tree-sitter-" --include='*.js' . 2>/dev/null); then
    export NODE_PATH="$ROOT/vendor${NODE_PATH:+:$NODE_PATH}"
    printf 'grammars.sh verify: %s: resolving package imports from vendor/\n' "$name" >&2
  fi
  export XDG_CACHE_HOME="$work/xdg"
  export TREE_SITTER_LIBDIR="$work/lib"
  mkdir -p "$XDG_CACHE_HOME" "$TREE_SITTER_LIBDIR"
  # Regenerate every grammar.js in the checkout (covers nested php/,
  # typescript/tsx, and ocaml grammars/*/ layouts): each nested grammar
  # generates from its own subdirectory, where its relative requires and
  # imports resolve.
  local failed=0
  while IFS= read -r gj; do
    gdir="$(dirname "$gj")"
    (cd "$work/upstream/$gdir" && "$tsbin" generate) || { printf 'grammars.sh verify: %s: generate failed in %s\n' "$name" "$gdir" >&2; failed=1; continue; }
  done < <(cd "$work/upstream" && find . -name grammar.js -not -path "./node_modules/*" | sort)
  [ "$failed" -eq 0 ] || exit 1
  local diffs=0
  while IFS= read -r gj; do
    rel="${gj#./}"
    if [ ! -f "$ROOT/vendor/$name/$rel" ]; then
      printf 'grammars.sh verify: %s: vendor file missing: %s\n' "$name" "$rel" >&2
      diffs=1
      continue
    fi
    if ! cmp -s "$work/upstream/$rel" "$ROOT/vendor/$name/$rel"; then
      printf 'grammars.sh verify: %s: differs: %s\n' "$name" "$rel" >&2
      diffs=1
    fi
  done < <(cd "$work/upstream" && find . -name grammar.js -not -path "./node_modules/*" | sort)
  # Compare regenerated parser artifacts. The cpp and lean vendors commit
  # src/parser.c.gz instead of src/parser.c (decompressed by build.rs), so
  # compare against the decompressed artifact; a regenerated artifact with
  # no vendor counterpart fails instead of passing silently.
  while IFS= read -r rf; do
    rel="${rf#./}"
    if [ "$rel" = "src/parser.c" ] && [ ! -f "$ROOT/vendor/$name/$rel" ] && [ -f "$ROOT/vendor/$name/$rel.gz" ]; then
      if ! gzip -dc "$ROOT/vendor/$name/$rel.gz" 2>"$work/gunzip.err" | cmp -s - "$work/upstream/$rel"; then
        printf 'grammars.sh verify: %s: differs: %s (against committed %s)\n' "$name" "$rel" "$rel.gz" >&2
        diffs=1
      fi
    elif [ -f "$ROOT/vendor/$name/$rel" ]; then
      if ! cmp -s "$work/upstream/$rel" "$ROOT/vendor/$name/$rel"; then
        printf 'grammars.sh verify: %s: differs: %s\n' "$name" "$rel" >&2
        diffs=1
      fi
    else
      printf 'grammars.sh verify: %s: missing vendor artifact: %s\n' "$name" "$rel" >&2
      diffs=1
    fi
  done < <(cd "$work/upstream" && find . -path "*/src/grammar.json" -o -path "*/src/node-types.json" -o -path "*/src/parser.c" | sort)
  # Compare external scanner sources too: scanner drift changes parsing
  # behavior without changing any generated artifact (e.g. powershell).
  # Headers compiled into the scanner ride along (e.g. quint); the
  # CLI-owned src/tree_sitter/ support headers stay out because the
  # pinned generator rewrites them, not the upstream checkout.
  while IFS= read -r sf; do
    rel="${sf#./}"
    if [ -f "$ROOT/vendor/$name/$rel" ]; then
      if ! cmp -s "$work/upstream/$rel" "$ROOT/vendor/$name/$rel"; then
        printf 'grammars.sh verify: %s: differs: %s\n' "$name" "$rel" >&2
        diffs=1
      fi
    else
      printf 'grammars.sh verify: %s: missing vendor artifact: %s\n' "$name" "$rel" >&2
      diffs=1
    fi
  done < <(cd "$work/upstream" && find . \( -path "*/src/scanner.c" -o -path "*/src/scanner.cc" -o -path "*/common/scanner.h" -o -path "*/src/*.h" \) -not -path "*/src/tree_sitter/*" | sort)
  if [ "$diffs" -ne 0 ]; then
    die "$name: regenerated tree differs from vendor/$name"
  fi
  printf 'grammars.sh verify: %s OK\n' "$name"
}

cmd_bump() {
  local name="${1:-}"
  local new_commit="${2:-}"
  local write=0
  [ "${3:-}" = "--write" ] && write=1
  [ -n "$name" ] && [ -n "$new_commit" ] || die "usage: grammars.sh bump <name> <commit> [--write]"
  need_file "$MANIFEST"
  local upstream old_commit tag generator
  upstream="$(entry_field "$name" "upstream")"
  old_commit="$(entry_field "$name" "commit")"
  tag="$(entry_field "$name" "tag")"
  generator="$(entry_field "$name" "generator")"
  [ -n "$upstream" ] || die "$name has no upstream to bump"
  case "$new_commit" in
    *[!0-9a-f]*) die "new commit is not lowercase hex: $new_commit" ;;
  esac
  if [ "${#new_commit}" -ne 40 ]; then
    die "new commit is not 40 hex: $new_commit"
  fi
  cat <<EOF
To follow upstream for $name:
1. Fetch the new commit: git ls-remote $upstream
2. Show what changed since the pinned commit:
   git clone --shared $upstream <scratch> && git -C <scratch> diff $old_commit $new_commit -- grammar.js src/
   (restrict to grammar inputs; regenerated src/ always differs)
3. Re-apply the patch series: vendor/$name/sloplint-grammar.patch
   (keep upstream authorship; record the upstream commit hash in the patch header)
4. Regenerate with the manifest generator CLI ${generator:-"(see SLOPLINT.md)"}, then run tree-sitter test
5. Update vendor/grammars.toml (commit, tag, generator) and the SLOPLINT.md provenance
Current pin: commit $old_commit tag ${tag:-"(none)"}
Target:      commit $new_commit
EOF
  if [ "$write" -eq 1 ]; then
    MANIFEST_PATH="$MANIFEST" ENTRY_NAME="$name" NEW_COMMIT="$new_commit" python3 - <<'EOF'
import os
path = os.environ["MANIFEST_PATH"]
text = open(path, encoding="utf-8").read()
old = None
import re, tomllib
with open(path, "rb") as f:
    data = tomllib.load(f)
for g in data["grammar"]:
    if g["name"] == os.environ["ENTRY_NAME"]:
        old = g.get("commit", "")
        break
assert old, "no manifest entry"
assert old in text, "pinned commit not found in manifest"
open(path, "w", encoding="utf-8").write(text.replace(f'commit = "{old}"', f'commit = "{os.environ["NEW_COMMIT"]}"', 1))
print(f"updated {os.environ['ENTRY_NAME']} commit {old} -> {os.environ['NEW_COMMIT']}")
print("now re-vendor, regenerate, and fix tag/generator in the manifest")
EOF
  fi
}

cmd="${1:-}"
case "$cmd" in
  check) cmd_check ;;
  verify) shift; cmd_verify "$@" ;;
  bump) shift; cmd_bump "$@" ;;
  *) die "usage: grammars.sh {check|verify <name>|bump <name> <commit> [--write]}" ;;
esac
