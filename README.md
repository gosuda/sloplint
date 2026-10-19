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

Install from source using the commands below. Registry installation is not the supported setup path.

Prerequisites: Git, [rustup](https://rustup.rs/), and a C/C++ toolchain for the target platform (the tree-sitter parsers and scanners compile native sources). The checked-in `rust-toolchain.toml` pins Rust 1.99.1, which `cargo` selects automatically.

The release workflow builds binaries for Windows, macOS, and Linux on x86_64 and aarch64. Linux archives use the `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` target names and link glibc dynamically. The supported baseline is glibc 2.39 or newer, matching the [Ubuntu 24.04 runners](https://packages.ubuntu.com/noble/libc6) used by CI and release builds. A binary may require older glibc symbols, but compatibility below 2.39 is not tested or promised. These Linux binaries do not run natively on Alpine or other musl-based systems.

sloplint uses the platform default allocator on every target. It includes no custom allocator.

CI builds and tests these targets and runs `sloplint check crates --strictness standard --mode threshold --fail-on deny` on each; see [.github/workflows/ci.yml](.github/workflows/ci.yml).

Releases are cut by pushing a `vMAJOR.MINOR.PATCH` tag equal to the `sloplint` package version; none has been published yet. The release workflow checks the tag against the package version, runs the full CI workflow on the tagged commit, smoke-tests each binary (`--version` equals the tag and `check crates --strictness standard --mode threshold --fail-on deny` passes), and uploads `sloplint-<tag>-<target>.tar.gz|zip` with its `.sha256` file.

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

This Bash example creates a new project. In an existing project, run `sloplint init` once to write the default configuration, then run `sloplint check`.

Exit code `0` means the gate passed: the run can still list `warn` findings below `fail_on` (default `deny`). `1` means at least one reported finding is at or above `fail_on`; `2` means usage or config error; `3` means an I/O error stopped the run.

An unreadable source file stops the run with exit `3` and reports its path and cause. Oversized, generated, and non-UTF-8 source files remain counted skips.

## Features

- **complexity/**: nesting-weighted cognitive depth, nesting depth, mutable-statement spans, nested mutation, boolean params, import fan-out.
- **grep/**: generic names, homonym definitions across files, renamed and wildcard imports, runtime-built symbols (Java reflection calls require a non-literal first argument), non-literal log messages, multiline arguments.
- **slop/**: pass-through wrappers, echo comments, swallowed or double-reported errors (error propagation such as Zig `try` and Odin `or_return` is not a handler), single-implementation abstractions, barrel files, field-copy literals.
- **agent/**: file size budgets, stringly-typed APIs, missing boundary annotations, untyped error returns, catch-all matches, ambient global state, message-less assertions, ambiguous patch anchors, missing colocated tests.
- **custom/**: tree-sitter queries in `sloplint.toml` or separate files, with captures interpolated into messages, per-rule severity, and path globs.

For Zig catches, Go `if err != nil` blocks, Ruby `rescue` clauses, TypeScript `catch` clauses, and Rust `Err` match arms and `if let Err` guards, `slop/zombie-catch` treats an empty or comment-only body as a handler with no statements, distinct from an unchanged rethrow. Rust arms with a match or `if` guard are not handlers. An `Err` arm that only rethrows is excluded when an earlier arm's pattern may match `Err`. Returning a recovery value does not count as swallowing the error.

Dart mutation rules track assignments, member and index writes, and prefix and postfix increments.

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

Every run analyzes the whole project root; `PATHS` only filters which findings are reported.

### Configuration

`--config FILE` names a file that must exist; without `--config`, a missing `sloplint.toml` falls back to defaults.

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

Baselines key findings by a fingerprint of rule, path, and normalized line; blank-line shifts do not re-report them. Line-local findings normalize the finding's source line, while whole-file findings carry a stable empty context, so a changed first line does not change their identity.
Unused-suppression findings retain separate identities, so baselining one does not hide another added later.

`project.baseline` is a path relative to the project root. Absolute paths, `..` escapes, and parent or leaf symlinks that resolve outside the root are rejected with an error and leave the outside target untouched; nested paths inside the root keep working. Cache writes refuse a symlinked `.sloplint` directory with a warning. `.gitignore` and the temporary cache file are created exclusively, so a pre-planted symlink at either path is preserved. Cache reads can follow a directory symlink; invalid or mismatched cache contents fall back to fresh analysis.

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

Files named by `project.custom_rule_files` load even when hidden or gitignored; source discovery keeps its usual filters.

### Suppressions

Inline where you cannot fix yet; a reason is required, and unused suppressions are lint findings too:

```rust
// sloplint-allow(complexity/cognitive-depth): state machine is one place by design
```

### Output formats

`--format agent` (default, stable machine-parseable text), `--format human` (annotated snippets with color), `--format json` (one document: findings, rules fired, summary).

## Development

```bash
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
sloplint check crates       # self-lint with the repo's own sloplint.toml
```

## Contributing

Fork, branch, add or update fixtures for the rule you touch, run the four commands above, and open a pull request. Fixture cases live under `crates/sloplint/tests/fixtures/<family>/<rule>/` with an `expected.txt` per project. See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide.

## License

Apache-2.0; see [LICENSE](LICENSE).
