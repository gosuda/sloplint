<div align="center">
  <h1>sloplint</h1>
  <p><strong>An agent-first code quality linter for twenty-three languages.</strong></p>

  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg?style=flat" alt="License"></a>
</div>

---

## What is this?

sloplint finds code patterns that make changes harder to review and apply: deep nesting, names hidden from search, redundant wrappers, and repeated blocks that make edits ambiguous. It gives developers and coding agents a source location, an explanation, and a suggested fix for each finding, using one CLI across 23 languages.

- **23 languages**: Rust, TypeScript/TSX, JavaScript/JSX, Python, Go, Java, Kotlin, C#, C, C++, Objective-C, PHP, Ruby, Bash, PowerShell, Swift, R, Scala, Dart, Zig, OCaml, Odin, Starlark.
- **28 builtin rules** in four families (`complexity/`, `grep/`, `slop/`, `agent/`), plus custom tree-sitter queries under `custom/`.
- **4 strictness profiles** (`minimal`, `basic`, `standard`, `strict`), each monotone in coverage and thresholds.
- **Ratchet mode** freezes today's findings into a baseline, so only new debt fails the build.
- **Fix instructions** on every finding: not just what is wrong, but the shape of the change that resolves it.

sloplint works from syntax alone: tree-sitter parses each file and per-language heuristics lower the tree into facts, so results are approximate by design. It is not a typechecker or a security analyzer, and it applies no patches itself; every finding carries a `fix:` line describing the change a human or agent should make.

## Install

Install from source with the command below. Registry installation is not supported.

Release archives cover x86_64 and aarch64 on Linux, macOS, and Windows; no release has been published yet. Linux binaries link glibc dynamically and need glibc 2.39 or newer. They do not run on musl systems such as Alpine. Each release holds the archives, one `.sha256` file per archive that detects a corrupted download, and one CycloneDX SBOM per target named `sloplint-<tag>-<target>.cdx.json`. To verify provenance, run `gh attestation verify <archive> --repo gosuda/sloplint --signer-workflow gosuda/sloplint/.github/workflows/release.yml --source-ref refs/tags/<tag> --source-digest <commit>`, where `<commit>` is the commit that `git rev-parse <tag>^{commit}` prints in a clone you trust. Without `--source-ref` and `--source-digest`, an archive built by the release workflow from any tag or commit passes the check.

Prerequisites: Git, [rustup](https://rustup.rs/), and a C/C++ toolchain for the target platform (the tree-sitter parsers and scanners compile native sources). The checked-in `rust-toolchain.toml` pins Rust 1.99.1, which `cargo` selects automatically.

From source, using the checked-in toolchain pin (Rust 1.99.1):

```bash
cargo install --git https://github.com/gosuda/sloplint --locked
```

This places the `sloplint` binary in `~/.cargo/bin`; make sure that directory is on your `PATH`.

## Quick start

```bash
mkdir sloplint-demo && cd sloplint-demo
sloplint init
mkdir src
printf 'pub fn handle(flag: bool) {}\n' > src/lib.rs
sloplint check --strictness standard
# src/lib.rs:1:8: warn: `handle` is a generic name shared by unrelated code [grep/generic-name]
```

In an existing project, run `sloplint init` once to write the default configuration, then run `sloplint check`.

Exit code `0` means the gate passed: the run can still list `warn` findings below `fail_on` (default `deny`). `1` means at least one reported finding is at or above `fail_on`; `2` means usage or config error; `3` means an I/O error stopped the run.

An unreadable source file stops the run with exit `3` and reports its path and cause. Oversized, generated, and non-UTF-8 source files remain counted skips.

## Features

- **complexity/**: nesting-weighted cognitive depth, nesting depth, mutable-statement spans, nested mutation, boolean params, import fan-out.
- **grep/**: generic names, homonym definitions across files, renamed and wildcard imports, runtime-built symbols, non-literal log messages, multiline arguments.
- **slop/**: pass-through wrappers, echo comments, swallowed or double-reported errors, single-implementation abstractions, barrel files, field-copy literals.
- **agent/**: file size budgets, stringly-typed APIs, missing boundary annotations, untyped error returns, catch-all matches, ambient global state, message-less assertions, ambiguous patch anchors, missing colocated tests.
- **custom/**: tree-sitter queries in `sloplint.toml` or separate files, with captures interpolated into messages, per-rule severity, and path globs. Each rule is bounded per file at 16,384 in-progress matches and 1,024 reported hits: a query that exceeds either cap reports `meta/custom-cap` instead of returning partial results silently.

`meta/parse-error` is Warn in every profile, so a default `deny` gate does not fail on it. Set the rule to Deny or pass `--fail-on warn`. `meta/custom-cap` is Deny in every profile: a capped query's results are partial, so a default `deny` gate fails until the query is anchored. An empty `project.languages` list analyzes nothing.

## Usage

### Commands

| Command | What it does |
|---|---|
| `sloplint check [PATHS]` | Lint files (bare `sloplint` is the same) |
| `sloplint baseline` | Freeze current findings into the baseline file |
| `sloplint rules [ID]` | Show effective rule settings for the active profile |
| `sloplint init` | Write a starter `sloplint.toml`; use `sloplint rules` for effective rule settings |

`check` flags: `--format agent|human|json`, `--strictness`, `--mode threshold|ratchet`, `--fail-on warn|deny|never`, `--rule ID` (repeatable, `family/*` allowed), `--config FILE`, `--no-cache`, `--jobs N`.

Unknown `--rule` selectors fail with exit 2 listing the valid rule ids.

Every run analyzes the project root. `PATHS` only filters reported findings. Hidden files are skipped. `.gitignore` applies up to the enclosing repository; `.ignore` applies from every ancestor directory.

### Configuration

`--config FILE` names a file that must exist; without `--config`, a missing `sloplint.toml` falls back to defaults. Configuration and custom rule files are capped at 1 MiB per file (16 MiB total for custom rules); baselines are capped at 512 MiB and 1,000,000 entries.

Rule option names and values are validated even when a rule is disabled or filtered out. An invalid threshold or unknown option fails with exit `2`; changing strictness cannot hide the error.

```toml
[project]
strictness = "standard"

[rules."complexity/cognitive-depth"]
deny_at = 18                          # warn_at still comes from the preset

[rules."agent/colocated-tests"]
level = "warn"                        # enables a rule the standard profile leaves off
```

### Ratchet mode

```bash
sloplint baseline               # writes sloplint-baseline.json
sloplint check --mode ratchet   # reports only findings absent from the baseline
```

`project.baseline` is relative to the project root. Paths that resolve outside the root are rejected.

`sloplint baseline` refuses to replace an existing regular file that is not a sloplint baseline, such as `sloplint.toml` or a source file: it exits 2 and leaves the file untouched; delete the file to record a baseline there.

The cache is per-user and never stored in the checkout. It lives in `sloplint/<hash of the project root>/cache-v2/` under `$XDG_CACHE_HOME` when that is absolute, else under `~/.cache`, `~/Library/Caches`, or `%LOCALAPPDATA%`. `--no-cache` skips it. A damaged or mismatched cache falls back to a fresh analysis.

sloplint warns and runs without the cache when the cache directory is a symlink, is not a directory, is writable by group or others, or sits inside the project root, when no home directory is set, or when the running executable cannot be read.

Older releases left a `cache-v1.bin` file in the per-user cache directory, and the oldest kept a `.sloplint` directory in the checkout. sloplint never reads or removes either; delete them by hand to reclaim the space.

### Custom rules

```toml
[[custom_rules]]
id = "custom/no-unwrap-in-lib"
language = "rust"
severity = "deny"
message = "`{method}()` in library code"
why = "unwrap panics on the error path and hides the failure from callers"
fix = "propagate with ? and return a typed error"
query = '''
(call_expression
  function: (field_expression field: (field_identifier) @method)
  (#eq? @method "unwrap")) @match
'''
```

Files named by `project.custom_rule_files` load even when hidden or gitignored. An absolute pattern, a pattern containing `..`, or a symlinked rule file is a configuration error.

### Suppressions

Suppress inline only where you cannot fix the code yet. A reason is required. A directive that suppresses nothing is itself a `meta/unused-suppression` finding, reported for each named active rule that matched nothing.

```rust
// sloplint-allow(complexity/cognitive-depth): state machine is one place by design
```

A directive covers its own comment and the next line. `sloplint-allow-file(rule): reason` covers the whole file.

### Output formats

`--format agent` (default, stable machine-parseable text), `--format human` (annotated snippets with color), `--format json` (one document: findings, rules fired, summary).

## Development

Build, test, and self-lint commands are in [CONTRIBUTING.md](CONTRIBUTING.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0; see [LICENSE](LICENSE).
