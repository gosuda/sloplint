# tree-sitter-tlaplus, vendored for sloplint

Date: 2026-11-19.

## Origin

- Upstream repository: https://github.com/tlaplus-community/tree-sitter-tlaplus
  (community-owned, mirrored on SourceHut).
- Upstream revision: `add40814fda369f6efd989977b2c498aaddde984` (HEAD of `main`
  on 2026-11-19; commit date 2026-02-08, `scanner: remove erroneous
  array_delete on const parameter`).
- Latest release at pin time: `v1.5.0` (2024-10-20). HEAD is pinned instead of
  the release because two scanner correctness fixes postdate it
  (`8d749f9a fix(scanner): correct UB in nested_scan`, 2026-01-22, and the
  `add40814` fix above). The external scanner handles nested proofs and
  conjunction/disjunction lists, which is exactly the nesting the linter
  measures, so the fixes are linter-relevant.
- Licence: MIT (`Copyright 2020 Andrew Helwer`).
- Copied verbatim from upstream: `grammar.js`, `src/` (including the
  committed `parser.c` and `src/scanner.c`), `bindings/rust/`, `Cargo.toml`
  (plus one empty `[workspace]` table, which keeps `cargo fmt --all` from
  claiming the package for the parent workspace), `LICENSE`, `queries/`,
  `tree-sitter.json`, `test/corpus/`, `test/examples/Highlight.tla`.
- Dropped: `bindings/{c,go,node,python,swift}/`, editor and packaging files
  (`.github/`, `Makefile`, `Package.swift`, `binding.gyp`, `pyproject.toml`,
  `setup.py`, `shell.nix`), `integrations/`, `test/{consumers,
  crash_regressions,dependencies,examples/external,fuzzing,sanitize,
  run-corpus.*}`. The `external` examples submodule (github.com/tlaplus/examples)
  is not fetched.
- `sloplint-grammar.patch` holds the one local change below (S1) plus the
  corpus expectation it flips. `grammar.js` is otherwise byte-identical to
  upstream HEAD.

## Regenerating src/

- Tool: tree-sitter-cli 0.24.7 (`/home/alpha/repos-devtool/tasks/tools-ts/bin`).
  Command, run in this directory: `tree-sitter generate` (warning-free),
  then `tree-sitter test`. ABI stays 14, so the runtime keeps loading the
  parser as-is.
- `src/parser.c` is 37028761 bytes (`STATE_COUNT 5552`). It stays committed
  plain: it is under GitHub's 100MB blob limit.
- `src/scanner.c` is 76905 bytes and unchanged: the external scanner for
  nested proofs and conjunction/disjunction lists (README: "the latter
  contains logic for parsing the context-sensitive parts of TLA+ like
  nested proofs and conjunction/disjunction lists").

## Changes to grammar.js

S1, empty files parse clean: the second `source_file` alternative uses
`repeat($._unit)` instead of `repeat1($._unit)`, so a file with no content
matches with zero units instead of erroring. The first alternative still
needs a module and the third a PlusCal algorithm, so the empty parse is
unambiguous, and every non-empty file parses exactly as before.
`test/corpus/modules.txt` flips the `Blank File Parse Error` `:error` case
to `Blank file parses clean` with `(source_file)`.

## Test

Run in this directory (`XDG_CACHE_HOME` pointed outside the checkout):

- `tree-sitter test` with CLI 0.24.7: 371 cases, 371 pass, 0 fail
  (3 committed `:skip` markers in `jlist.txt`, `labels.txt`, `proofs.txt`,
  applied upstream; `:error` cases pass as expected failures).
- `test/examples/Highlight.tla` parses with no `ERROR` or `MISSING` node
  (also pinned by `crates/sloplint-syntax/tests/grammar_gaps/tlaplus.rs`).
## Node compatibility

Node kinds and fields read by `crates/sloplint-syntax/src/lang/tlaplus.rs`:

- `operator_definition` (fields `name`, `parameter`, `definition`),
  `function_definition` (fields `name`, `definition`; `quantifier_bound`
  children are parameters), `recursive_declaration`,
  `pcal_macro_decl` (fields `name`, `parameter`), `pcal_procedure`,
  `pcal_process` (field `name`), `theorem` (field `name`).
- `operator_args` (function-like parameters), `quantifier_bound`
  (fields `intro`, `set`: bound-quantifier parameters).

## Mapping

See the `lang(tlaplus)` commit. Function-like units: `operator_definition`,
`function_definition` (parameters from `quantifier_bound` intros), `theorem`,
PlusCal `pcal_macro_decl` (parameters from `parameter` identifiers),
`pcal_procedure` (parameters from `pcal_proc_var_decl`) and `pcal_process`.
Parameters: `operator_args` and `quantifier_bound` intros. Branches:
`if_then_else`, PlusCal `pcal_if`/`pcal_either`, `case` with
`case_arm`/`other_arm`. Loops: bounded and unbounded quantification
(TLA+ has no loop statements) and PlusCal `pcal_while`; PlusCal `pcal_with`
is a scoped binding. Imports: `extends` and `instance`. Comments: `comment`
and `block_comment`. Calls: `bound_*_op`, `bound_op`, `prefixed_op`,
`function_evaluation`, `pcal_macro_call` and `pcal_proc_call` (callee names
only; argument shapes are unmapped, so message-content and dynamic-symbol
rules stay silent). Declarations: `variable_declaration`,
`constant_declaration`, `recursive_declaration`, PlusCal `pcal_var_decl`;
writes: PlusCal `pcal_assign` targets.
Unmapped (no honest counterpart, never force-fired): proofs (`_proof`
family: `proof_step`, `assume_prove`, `have_proof_step`,
`witness_proof_step`, `take_proof_step`, `pick_proof_step`,
`suffices_proof_step`, `qed_step`, `terminal_proof`, `non_terminal_proof`,
`use_body*`, `case_proof_step`, `definition_proof_step`), conjunction and
disjunction lists (`conj_list`/`disj_list` and items: idiomatic structuring,
not branches), strings (the linter has no string role), `match_arm`
scrutinees (TLA+ `CASE` has guards, not a scrutinee expression), and theorem
proofs bodies.

## Known limits

- Correctness is approximate by the grammar's own statement (README `Aims &
  Capabilities`): valid TLA+ parses correctly; invalid TLA+ parses
  approximately correctly. It is not a checker backbone.
- PlusCal parses only inside block comments (`block_comment` children include
  `pcal_algorithm`); line-comment PlusCal is comment text.
- The corpus `nonstandard/` cases pin comment-embedded and fragment shapes;
  three corpus cases stay `:skip`ped upstream.

S3, PlusCal declarations (2026-11-20 dogfood corpus):

- `pcal_p_definitions` / `pcal_c_definitions` are `repeat1(choice(
  definition, recursive_declaration))`: `RECURSIVE Op(...)` is legal in
  a `define` block (jb567-style specs) and previously errored at `RECURSIVE`.
- New `pcal_fifo_decls`: `fifos`/`fifo`/`channels`/`channel` declarations
  alongside `variables` in P- and C-syntax algorithms
  (`fifos q1, q2;`), which the base grammar never admitted; fugue-style
  specs carry them.

## Changes to src/scanner.c

S2, bounded `nested_scanner_serialize`: the serializer used to write the
full context stack unconditionally, overflowing
`TREE_SITTER_SERIALIZATION_BUFFER_SIZE` (1024, `src/tree_sitter/parser.h`)
when a deeply nested proof/PlusCal state ran up the serialized size, and
`nested_scanner_deserialize` then read stale bytes as the context count.
Now every context keeps its 4-byte size slot (reserved up front, so the
accounting cannot overflow) and each context's contents are written only
when they fit — a context that does not fit gets a zero size slot and
deserializes as an empty context at the right nesting depth, preserving
the block structure instead of truncating the stack. The wire format is
unchanged: `nested_scanner_deserialize` reads the depth, N sizes, N-1
contents, and the current context exactly as before. Only a stack deeper
than the size slots alone can hold drops the outermost contexts.
