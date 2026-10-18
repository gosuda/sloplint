# Contributing to sloplint

This guide covers building, testing, and extending `sloplint`.

## Prerequisites

Install `rustup`. The workspace pins Rust `1.99.1` with `clippy` and `rustfmt` in `rust-toolchain.toml`, which `cargo` selects automatically.

## Workspace layout

- `crates/sloplint-core`: facts, configuration, suppressions, baselines, and the rule catalog (`FileRule` and `WorldRule`). No tree-sitter or I/O dependencies.
- `crates/sloplint-syntax`: one iterative `TreeCursor` walk and 23 language adapters (`crates/sloplint-syntax/src/lang/*.rs`) that lower tree-sitter parse trees into `FileFacts`, plus `[[custom_rules]]` query execution.
- `crates/sloplint`: CLI entry point, project discovery, `.sloplint/cache-v1.bin` cache, and `agent`, `human`, and `json` renderers.

Rules read only `FileFacts` and `WorldFacts`. Keeping `sloplint-core` free of `tree-sitter` enforces that boundary in the dependency graph.

## Build, test, and lint

Build the workspace:

```bash
cargo build --locked
```

Run the unit, property, fixture, and invariant test suites:

```bash
cargo test --workspace --locked
```

Run the formatter and linter checks required by CI:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Run the self-lint check against this repository (using the root `sloplint.toml`, which excludes test fixtures):

```bash
sloplint check crates
# or, without installing the binary in PATH:
cargo run --locked --release -p sloplint -- check crates --format agent
```

Every file in `crates/` must pass `sloplint` under the default `standard` profile, including the 500-line file budget (`agent/file-token-budget`). Do not add implementation comments; place doc comments only on `pub` items in `sloplint-core` and `sloplint-syntax`.

## Adding a rule

1. Create `crates/sloplint-core/src/rules/<family>/<rule_name>.rs`, where `<family>` is `complexity`, `grep`, `slop`, or `agent`. Keep one rule per file.
2. Define an `Options` struct with `#[derive(Serialize, Deserialize)]` and `#[serde(deny_unknown_fields)]`, a `preset(Strictness) -> Preset` function covering all four profiles (`Minimal`, `Basic`, `Standard`, `Strict`), and a `FileRule` or `WorldRule` implementation.
3. Keep presets monotone from `minimal` to `strict`: rule levels must not decrease and metric thresholds (`Thresholds::new(warn_at, deny_at)`) must not loosen as strictness increases.
4. Export `pub(crate) const INFO: RuleInfo` from the rule module, declare the module in `crates/sloplint-core/src/rules/<family>/mod.rs`, and insert `INFO` into `CATALOG` in `crates/sloplint-core/src/rules/mod.rs` in sorted order by rule id.

## Adding fixtures

Every rule has a fixture directory at `crates/sloplint/tests/fixtures/<family>/<rule>/`:

- Source files in each language where the rule can fire, covering both positive findings and near-miss cases that must stay clean.
- An optional `sloplint.toml` when a fixture needs to override a single threshold or option on top of the `strict` preset.
- `expected.txt` listing every expected finding, one per line, sorted by `<rel-path>:<line> <severity> <rule-id>` (`#` starts a comment; an empty file expects zero findings).

Expectations live in `expected.txt` rather than inline comments so test markers never alter line counts, comments, or syntax trees in the analyzed source.

## Commit and pull request conventions

- Fork the repository and create a topic branch from `main`.
- Keep one logical concern per commit. Write a capitalized, imperative subject line of roughly 50 characters (for example, `Add builtin rule catalog`).
- Update or add fixtures in `crates/sloplint/tests/fixtures/<family>/<rule>/` for any rule or language adapter you change.
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, and `sloplint check crates` before opening a pull request. All CI jobs in `.github/workflows/ci.yml` must pass.
