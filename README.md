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

Install from source using the commands below. There is no GitHub release yet; registry installation is not the supported setup path.

Prerequisites: Git, [rustup](https://rustup.rs/), and a C/C++ toolchain for the target platform (the tree-sitter parsers and scanners compile native sources). The checked-in `rust-toolchain.toml` pins Rust 1.99.1, which `cargo` selects automatically.

The release workflow builds binaries for Windows, macOS, and Linux on x86_64 and aarch64. Linux archives use the `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` target names and link glibc dynamically. The supported baseline is glibc 2.39 or newer, matching the [Ubuntu 24.04 runners](https://packages.ubuntu.com/noble/libc6) used by CI and release builds. A binary may require older glibc symbols, but compatibility below 2.39 is not tested or promised. These Linux binaries do not run natively on Alpine or other musl-based systems.

sloplint uses the platform default allocator on every target. It includes no custom allocator.

CI builds and tests these targets; see [.github/workflows/ci.yml](.github/workflows/ci.yml).

From source, using the checked-in toolchain pin (Rust 1.99.1):

```bash
git clone https://github.com/gosuda/sloplint
cd sloplint
cargo install --path crates/sloplint --locked
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

## Features

- **complexity/**: cognitive depth, nesting depth, mutable-statement spans, nested mutation, boolean params, import fan-out.
- **grep/**: generic names, homonym definitions across files, renamed and wildcard imports, runtime-built symbols, non-literal log messages, multiline arguments.
- **slop/**: pass-through wrappers, echo comments, swallowed or double-reported errors, single-implementation abstractions, barrel files, field-copy literals.
- **agent/**: file size budgets, stringly-typed APIs, missing boundary annotations, untyped error returns, catch-all matches, ambient global state, message-less assertions, ambiguous patch anchors, missing colocated tests.
- **custom/**: tree-sitter queries in `sloplint.toml` or separate files, with captures interpolated into messages, per-rule severity, and path globs.

## Usage

### Commands

| Command | What it does |
|---|---|
| `sloplint check [PATHS]` | Lint files (bare `sloplint` is the same) |
| `sloplint baseline` | Freeze current findings into the baseline file |
| `sloplint rules [ID]` | Show effective rule settings for the active profile |
| `sloplint init` | Write a `sloplint.toml` with every default spelled out |

`check` flags: `--format agent|human|json`, `--strictness`, `--mode threshold|ratchet`, `--fail-on warn|deny|never`, `--rule ID` (repeatable, `family/*` allowed), `--config FILE`, `--no-cache`, `--jobs N`.

Every run analyzes the whole project root; `PATHS` only filters which findings are reported.

### Configuration

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

Baselines key findings by a fingerprint of rule, path, and normalized line; blank-line shifts do not re-report them.

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
