# tree-sitter-alloy, vendored for sloplint

Date: 2026-11-19.

## Origin

- Upstream repository: https://github.com/vincentlu/tree-sitter-alloy
  (a tree-sitter grammar for the Alloy 6 formal specification language;
  not the Grafana Alloy configuration language grammar of the same name).
- Pinned revision: `7bcc865b0d12ff4f20876efd072fd1bc11faff48`
  (HEAD `main`, commit date 2026-03-14).
- Licence: MIT (`LICENSE` is the upstream file, fetched verbatim).
- This grammar is unpublished: no crates.io crate and no npm package exist
  for it, so there is no published crate to diff against. `grammar.js` is
  unmodified from the pinned revision and `sloplint-grammar.patch` is empty.
- Files kept from the revision: `grammar.js`, `src/` (`parser.c`,
  `grammar.json`, `node-types.json`, `tree_sitter/`), `queries/`,
  `test/corpus/`, `tree-sitter.json`, `LICENSE`, `README.md`,
  `bindings/rust/` (rewritten below).
- Files dropped: `bindings/node/`, `binding.gyp`, `package.json`,
  `package-lock.json`.
- `Cargo.toml` is new (the upstream `Cargo.toml` names a wrong repository
  URL, `https://github.com/tree-sitter/tree-sitter-alloy`, which does not
  hold this grammar; the vendored manifest corrects it to the upstream URL
  above). It keeps the upstream name `tree-sitter-alloy` and version
  `0.0.1`, adds the empty `[workspace]` table so `cargo fmt --all` does
  not claim the package for the parent workspace, depends on
  `tree-sitter-language 0.1` with `cc 1.2` as the build dependency, and
  builds with `bindings/rust/build.rs` (`cc`, C11, `-utf-8` on MSVC, no
  external scanner to compile).
- `bindings/rust/lib.rs` exposes the modern `LANGUAGE: LanguageFn`
  constant (plus `NODE_TYPES` and `HIGHLIGHTS_QUERY`); the upstream file
  exposes only the legacy `language()` function against
  `tree-sitter ~0.20`.

## Regenerating src/

Tool: tree-sitter-cli 0.27.1
(`tasks/tools-ts-0.27.1/tree-sitter`, release binary). JavaScript
runtime: node v26.13.1. Command, run in this directory:
`tree-sitter generate`, then `tree-sitter test`. The grammar is
self-contained (`grammar.js` has no `require` of another grammar), so no
vendored `node_modules` are needed.

Regenerating the pinned `grammar.js` with 0.27.1 is byte-identical to the
committed `src/parser.c`, `src/grammar.json` and `src/node-types.json`,
so the committed parser is the faithful 0.27.1 output: ABI 15
(`#define LANGUAGE_VERSION 15`), `STATE_COUNT 624`. No external scanner
(`src/` holds no `scanner.c`).

## Why this grammar needs no patch

The upstream corpus passes unmodified: `tree-sitter test` reports 83
parses, 83 successful, 0 failed. The grammar README states coverage of
the full Alloy 6 syntax (temporal operators, `var` fields, trace-style
`run` commands) validated against 331 real-world `.als` files at a 100%
parse rate.

## Known remaining gaps

- Nested comprehensions can misparse (upstream issue, open 2026-08-09).
  Affected files still parse without ERROR nodes in the cases tried here;
  the shape, not the parse, may be wrong.
- The grammar is expression-oriented: there is no `if` statement node and
  no `match` node. Conditionality is `implies_expression` (with an
  optional `else`), iteration is `quantified_expression`. The sloplint
  adapter maps those two shapes onto the conditional and loop concepts;
  plain boolean connectives (`and`/`or`/`iff`) and temporal operators
  (`always`/`eventually`/`until`/`since`) stay unmapped.

## Corpora (tree-sitter-cli 0.27.1, this directory)

- `test/corpus`: 83 cases, 83 pass.
