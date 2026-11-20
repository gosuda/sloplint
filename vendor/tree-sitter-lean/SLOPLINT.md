# tree-sitter-lean, vendored for sloplint

Date: 2026-11-19.

## Origin

- Upstream: `https://github.com/Julian/tree-sitter-lean`, default branch
  `main`, commit `0d28e5d6e8c4755f5df7ef9bde3b6ec6f59fae94` dated
  2026-11-01 (`Merge pull request #30`, HEAD on 2026-11-19; the repository
  has no tags and no releases).
- Licence: MIT (`LICENSE`: `Copyright (c) 2024 Julian Berman`).
- Rust crate name and version are unchanged: `tree-sitter-lean 0.2.0`
  (`Cargo.toml`). The crate is unpublished on crates.io and npm; the
  `Cargo.toml` `repository` already names the upstream repository.
- `sloplint-grammar.patch` carries one grammar edit: `open <ns> renaming
  a → b, c → d` (also `->`) parses as `renaming_pair` fields instead of
  erroring on the arrow; `hiding`/`renaming`/`only`/`as` tails still lex as
  identifier namespaces for the adapter to strip.
- `Cargo.toml` has one addition: an empty `[workspace]` table after the
  `[dev-dependencies]` section. Without it `cargo fmt --all` reports the
  vendored package as belonging to the parent workspace and fails. No other
  manifest change.
- Files kept from the revision: `Cargo.toml`, `LICENSE`, `README.md`,
  `grammar.js`, `queries/` (`folds`, `highlights`, `indents`, `injections`,
  `locals`), `src/` (`grammar.json`, `node-types.json`, `parser.c`,
  `scanner.c`, `tree_sitter/`), `bindings/rust/` (`build.rs`, `lib.rs`),
  `test/corpus/` (7 files), `tree-sitter.json`.
- Files dropped: `binding.gyp`, `bindings/c/`, `bindings/node/`,
  `bindings/python/`, `CMakeLists.txt`, `Makefile`, `pyproject.toml`,
  `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`, `.github/`.

## How src/ is produced

- `src/` is the committed upstream parser at the pinned revision, used
  without regeneration: `src/parser.c` carries `#define LANGUAGE_VERSION
  15`, which the workspace runtime (`tree-sitter 0.27.2`) loads.
- `src/scanner.c` is upstream: an external scanner for dotted and
  quoted identifiers, nested block/doc/module-doc comments, raw string
  literals with matched `#` counts, and the layout-sensitive `_indent`,
  `_dedent`, `_newline` tokens that delimit tactic and do-block bodies.
- No `tree-sitter generate` run: the grammar's `package.json` pins
  `tree-sitter-cli ^0.26.0` and the committed parser already speaks the
  ABI the runtime loads, so regenerating would only churn 49 MB of C.
- Verification (not regeneration) used tree-sitter-cli 0.27.1
  (`/home/alpha/repos-devtool/tasks/tools-ts-0.27.1/tree-sitter`):
  `tree-sitter test` in this directory, 301/301 corpus cases pass (see
  "Corpus results"). Purge `$XDG_CACHE_HOME/tree-sitter` before rerunning
  after any future edit, per `vendor/README.md`.

## Size

`src/parser.c`: 49,111,890 bytes, 1,298,005 lines. States: 10243
(`STATE_COUNT`), 6502 large. Symbols: 591. Token count: 403. External
tokens: 11 (`EXTERNAL_TOKEN_COUNT`). Fields: 51. ABI 15
(`LANGUAGE_VERSION`). Under GitHub's 100 MB blob limit, so `src/parser.c`
is committed plain; no `.gitignore`, no packed copy.

## Corpus results

`tree-sitter test` (cli 0.27.1, this directory, committed parser):

- Total parses: 301; successful parses: 301; failed parses: 0.

## Node compatibility

The sloplint Lean adapter (`crates/sloplint-syntax/src/lang/lean.rs`)
maps the grammar's own node names onto the linter roles. Definitions
(`def`, `theorem` which also covers `lemma`, `abbrev`, `opaque`, `axiom`,
`instance`, `example`, `constant`, `where_aux_def`) are functions with
parameters taken from the `binders` children; `fun` is the closure;
`if_then_else` is the branch; `match` and `induction` are matches with
`match_alt` arms; `app` (including nested `app` chains and tactic-head
applications, which the grammar leaves as generic terms per its README)
is the call; `import` and `open` are imports; `inductive` and `structure`
are type definitions; `let` and `have` are declarations; `block_assign`
(`:=`) is the write; the four comment nodes are comments.

Unmapped, with reasons:

- Loops: the grammar emits no dedicated loop node. `for`/`in` and `while`
  inside `do` blocks parse as generic `app` chains, and recursion is a
  `def` whose body contains an `app` self-reference. There is no honest
  Loop counterpart, so complexity depth from loops is not counted.
- `sorry`, `hole`, tactic nodes other than `match_alt` carriers
  (`by`, `by_cases`, `tactic_case`, `induction` targets), specification
  nodes (`forall`, `exists`, `have` bodies aside, `show`, `suffices`),
  proof-state commands (`set_option`, `attribute_cmd`, `syntax_cmd`,
  `macro_cmd`, `notation_decl_cmd`), module-structure commands
  (`namespace`, `section`, `end`, `mutual`, `variable`, `universe`,
  `export` aside from imports, `open ... hiding` details): no linter
  concept covers them; they contribute ordinary expression nesting
  (`app`, `binary_op`, `unary_op`, `paren`, `fun`, `forall`) where they
  appear inside function bodies.

## Known limits (upstream's own statement)

`README.md` ("What this is not"): the grammar cannot model Lean's dynamic
extensible parser. Concretely unsupported: user-defined notation, syntax
and macro extensions (commands parse; uses only when already in the static
operator table); custom precedence numbers via `:N` suffixes; the full set
of Mathlib tactics (uniformly parsed as generic term-shaped invocations);
multi-name structure fields (`x y : T`); per-field doc-comments and
attributes (one name per field); `end` is treated as a top-level command
rather than a nested closer. `src/parser.c` header comment in
`src/scanner.c` lists the externalized tokens above.

Observed here: `"hello {1 + 2} world"` parses as a single `str_lit` with no
`interpolated_str`/`interpolation` child, so ordinary string content stays
invisible to interpolation-aware rules; `import`/`open` carry only the
dotted name. Both are grammar limits, not adapter gaps.
