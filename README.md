<div align="center">
  <h1>sloplint</h1>
  <p><strong>An agent-first code quality linter for twenty-three languages.</strong></p>

  <a href="https://crates.io/crates/sloplint"><img src="https://img.shields.io/crates/v/sloplint.svg?style=flat" alt="crates.io"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg?style=flat" alt="License"></a>
</div>

---

## What is this?

sloplint lints code for the properties that decide whether a coding agent (or a tired human) can find, patch, and verify a change on its own. It parses with tree-sitter and reports measurable facts: how much state a function forces you to hold in memory, whether a name survives a text search, whether a comment says anything the code did not, and whether a patch anchor is unique enough to edit safely.

- **23 languages**: Rust, TypeScript/TSX, JavaScript/JSX, Python, Go, Java, Kotlin, C#, C, C++, Objective-C, PHP, Ruby, Bash, PowerShell, Swift, R, Scala, Dart, Zig, OCaml, Odin, Starlark.
- **28 builtin rules** in four families (`complexity/`, `grep/`, `slop/`, `agent/`), plus custom tree-sitter queries under `custom/`.
- **4 strictness profiles** (`minimal`, `basic`, `standard`, `strict`), each monotone in coverage and thresholds.
- **Ratchet mode** freezes today's findings into a baseline, so only new debt fails the build.
- **Fix instructions** on every finding: not just what is wrong, but the shape of the change that resolves it.

## Install

Registry install from the first tagged release (`0.0.0` today is a name-reservation placeholder with no binary):

```bash
cargo install sloplint      # crates.io; binary name: sloplint
npm install -g sloplint-cli # npm
```

From source, using the checked-in toolchain pin (Rust 1.99.1):

```bash
git clone https://github.com/gosuda/sloplint
cd sloplint
cargo install --path crates/sloplint --locked
```

## Quick start

```bash
cd your-project
sloplint init               # writes sloplint.toml with every default spelled out
sloplint                    # check with the standard profile
sloplint --strictness strict
# src/lib.rs:4:8: deny: `process_order` takes 3 consecutive primitive parameters [agent/stringly-api]
```

Exit codes: `0` clean, `1` findings at or above `fail_on`, `2` usage or config error, `3` I/O error.

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

### Configuration

```toml
[project]
strictness = "strict"

[rules."complexity/cognitive-depth"]
deny_at = 18                          # warn_at still comes from the preset

[rules."agent/colocated-tests"]
level = "warn"                        # enables a rule the profile leaves off
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

Fork, branch, add or update fixtures for the rule you touch, run the three commands above, and open a pull request. Fixture cases live under `crates/sloplint/tests/fixtures/<family>/<rule>/` with an `expected.txt` per project.

## License

Apache-2.0; see [LICENSE](LICENSE).
