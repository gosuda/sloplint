# tree-sitter-dafny, vendored for sloplint

Date: 2026-11-19.

## Origin

- Upstream repository: https://github.com/hath995/tree-sitter-dafny
- Upstream revision: `8085572ab23ba703a1f5c1cf753136a3508c68a6` (HEAD `main`,
  commit date 2026-06-30; no tags or releases exist).
- Published crate: tree-sitter-dafny 0.1.0 (crates.io, updated 2026-06-30).
  The crate's `.cargo_vcs_info.json` records sha1
  `8085572ab23ba703a1f5c1cf753136a3508c68a6`, so the published crate is the
  pinned revision. Name and version are unchanged.
- This directory is the published crate plus the files the crate omits:
  `LICENSE` (MIT, fetched from the upstream repository at the pinned
  revision; the crate ships no license file), `grammar.js`, `src/`
  (`parser.c`, `grammar.json`, `node-types.json`, `scanner.c`,
  `src/tree_sitter/`), `bindings/rust/`, `queries/`, `tree-sitter.json`,
  `README.md`. Dropped from the crate: `Cargo.toml.orig`, `Cargo.lock`,
  `.cargo_vcs_info.json`. `Cargo.toml` is the crate's original manifest.
- `sloplint-grammar.patch`: none. No local grammar change was needed; there
  is no patch file in this directory.

## Why

Dafny is a verification-aware language with no tree-sitter grammar in the
existing set. This grammar follows Dafny's EBNF reference grammar and parses
the language's declaration, statement, and expression forms, including the
specification clauses (`requires`, `ensures`, `invariant`, `decreases`) that
the sloplint adapter deliberately leaves unmapped.

## Regenerating src/

- No regeneration was run. The committed `src/parser.c` carries
  `#define LANGUAGE_VERSION 14` and `EXTERNAL_TOKEN_COUNT 15`; the
  tree-sitter 0.27 runtime used by sloplint loads ABI 14 (as the C and C++
  grammars in this tree prove), and the grammar's own dev-dependency pins
  tree-sitter 0.24.7, whose CLI emits ABI 14. Regenerating with the
  installed 0.24.7 CLI (`/home/alpha/repos-devtool/tasks/tools-ts/bin`)
  would only reproduce the same ABI.
- `src/parser.c` is 55,653,939 bytes, below GitHub's 100 MB blob limit, so
  it is committed plain (no `parser.c.gz` packing, no `.gitignore`).
- `src/scanner.c` is upstream's external scanner for context-sensitive
  tokens (generic brackets, `|` cardinality/comprehension families, `>>`
  split-close); it is kept verbatim.
- Upstream `test/` holds only `test/parse-suite.js`, a node runner script,
  not a tree-sitter corpus, so there is no `test/corpus/` to vendor and
  `tree-sitter test` runs zero tests here. Grammar coverage is instead
  pinned by `crates/sloplint-syntax/tests/grammar_gaps/dafny.rs`.

## Node kinds the sloplint adapter reads

From `src/node-types.json` (326 entries, 178 named) at the pinned revision.
Only `call_expression` (`function`, `type_arguments`) and the declaration
nodes (`method_decl`, `function_decl`, `constructor_decl`, `iterator_decl`,
`class_decl`, `trait_decl`, `datatype_decl`, `newtype_decl`,
`synonym_type_decl`, `datatype_formal`, `formal`, `local_var_decl`,
`module_definition`, `module_import`, `module_export`) define fields; every
other kind the adapter reads is field-less, so the adapter uses positional
named-child access throughout.

## Known gaps (upstream, documented in the grammar README)

- Float literals `.5` / `42.` (leading/trailing-dot forms).
- Empty `match` expressions/statements.
- Backtick frame expressions in `reads`/`modifies` clauses.
- Digit-leading member names and a few other `Remaining Known Issues`
  entries (~1.5% of the grammar's 1,400-file corpus).

## Size

- `src/parser.c`: 55,653,939 bytes (`STATE_COUNT 24699`).
- `src/scanner.c`: 15,013 bytes.
