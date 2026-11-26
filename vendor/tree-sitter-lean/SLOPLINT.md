# tree-sitter-lean, vendored for sloplint

Date: 2026-11-26.

## Origin

- Upstream: `https://github.com/Julian/tree-sitter-lean`, default branch
  `main`, commit `fefc2eff9a83eeb72cda44646709e40e33e09175` dated
  2026-11-21 (`Merge pull request #36`, still HEAD on 2026-11-24; the
  repository has no tags and no releases). The previous pin was
  `0d28e5d6e8c4755f5df7ef9bde3b6ec6f59fae94` (2026-11-01, PR #30 merge);
  the only change between the two is PR #36.
- Licence: MIT (`LICENSE`: `Copyright (c) 2024 Julian Berman`).
- String gaps (a backslash, a line break, and the next line's
  indentation inside `"…"` and `s!"…"`) come from upstream PR #36
  (issue #8): commit `5c56950` is applied with its authorship
  (`grammar.js`, `test/corpus/literals.txt`). Its companion `d9736e6`
  only regenerated upstream's parser, so the regeneration of the patched
  grammar here replaces it.
- Rust crate name and version are unchanged: `tree-sitter-lean 0.2.0`
  (`Cargo.toml`). The crate is unpublished on crates.io and npm; the
  `Cargo.toml` `repository` already names the upstream repository.
- `sloplint-grammar.patch` carries the grammar delta vs the pinned
  upstream: 59 hunks, none of them from an upstream PR. Moving the pin to
  `fefc2ef` changed the `grammar.js` patch indexes and hunk offsets.
  The extra hunk records a prior vendored change: `queries/highlights.scm`
  drops `(public_section) @keyword.import`, because the patch folds
  `public_section` into `section`, and a query naming a missing node does
  not compile. Round 2 (2026-11-21) covers, on top of the original
  `open <ns> renaming a → b` fix:
  - `do`-block layout: `try … catch … finally` without separator tokens
    (the scanner now peeks clause keywords at same-indent and marks the
    line end first). The scanner progress fix below supersedes the
    original transient newline replay state.
  - `for x in xs do` loops (`for_loop` with `prec.right(PREC.app + 1)`;
    a `reserved:` keyword block was tried and removed — it stops `using`,
    `with`, `do` etc. falling back to identifiers in argument position,
    which upstream deliberately avoids).
  - `run_meta`/`include` commands, `variable n : T` (typed bare binder),
    `meta section`, `open … in` at term level, `#hash` commands and
    `World "…"`/`et.semantics.termination "…"` custom command heads
    (arg-less, upstream shape — the `custom_command_head` lexical-tie
    steal at completable NAME positions is fixed instead by
    `_name_or_head` on `end`/`section`/`universe`/`open`), `;`-leading
    block lines. Commas are not statement separators
    (`{ f := by t, g := 1 }`).
  - `calc` / `conv` / `match` with multi-`|` alternatives; `calc` steps
    win a dynamic-preference fight vs `binary_op` when `:=` follows
    (`by calc 0 = 0 := rfl`).
  - `for (a, b) in xs` and `for ⟨a, b⟩ in xs` destructuring binders.
  - Lake DSL heads (`package`, `lean_lib`, `lean_exe`, `require`, …) as
    `lake_cmd` instead of erroring on `where`/brace fields — the top
    corpus-error family; `@[default_target]` attributes and `#[…]`
    array-literal values are accepted.
  - `bindings/rust/build.rs` emits `cargo:rerun-if-changed=` for `src/`,
    `src/parser.c.gz` and `grammar.js`. It watches the `src/` directory
    rather than `src/parser.c` itself: the file is absent in normal
    checkouts, and a watched missing path reruns the build script on
    every build — each rerun recompiled the ~97 MB parser. The directory
    watch still notices a `tree-sitter generate` creating `parser.c`.
  - Known gap (no error, no facts): `custom_command_head` wins the
    lexical tie at TERM-TAIL positions (`#check 𝟙 X`, `def M :=
    StateT Nat IO`) and produces a harmless phantom `custom_command`
    node — the adapter emits no facts for it. Both lexical-precedence
    fixes are non-viable (`token(prec(-1))` explodes past the 65,535
    state ceiling; `token(prec(1))` on the `word` token steals
    keywords).
  - Identifier scanning: `is_id_rest` accepts `≥ ≤ > < ∞` so `ℝ≥0∞`
    and `x≥0` lex as single identifiers, and superscript/subscript
    alphanumeric chars join the token (`Aᵒᵖ`, `x⁻¹`, `a⁺` are now
    identifiers — postfix notation on those chars is fixture-drifted
    away, not error-freed).
  - Multi-level dedents leave the triggering physical newline unread
    until every enclosing block has closed. Each zero-width dedent pops
    one serialized indentation level; the final newline consumes input.
    This also preserves sibling statement separators without replay flags.
  - Known upstream-parity gaps: some tactic and modifier forms lack
    dedicated nodes or produce errors. Examples include `simpa using h`,
    `rcases h with ⟨…⟩`, `first | …` alternation, `∀ᶠ`, a comma after a tactic in `by` blocks
    (`by t, u` parses as `app(t, ERROR, u)` identically upstream), and
    `open … hiding`/`export` modifier words landing in
    `namespace`/`custom_command` nodes. (`open only Nat` now parses
    `only` as a real first namespace — identical to upstream.)
  - Generated-artifact note: `src/parser.c` (~97MB) is not committed;
    `src/parser.c.gz` is, and `bindings/rust/build.rs` inflates it into
    `OUT_DIR` when the plain file is absent.
- `Cargo.toml` has two additions: a `flate2 = "1"` build dependency
  (`bindings/rust/build.rs` inflates `src/parser.c.gz` with it) and an
  empty `[workspace]` table after the `[dev-dependencies]` section.
  Without the table `cargo fmt --all` reports the vendored package as
  belonging to the parent workspace and fails. `.gitignore` is replaced
  by one that ignores `src/parser.c` and `lean.so`. No other packaging
  change.
- Files kept from the revision: `Cargo.toml`, `LICENSE`, `README.md`,
  `grammar.js`, `queries/` (`folds`, `highlights`, `indents`, `injections`,
  `locals`), `src/` (`grammar.json`, `node-types.json`, `parser.c.gz`,
  `scanner.c`, `tree_sitter/`), `bindings/rust/` (`build.rs`, `lib.rs`),
  `test/corpus/` (upstream's 7 files plus `sloplint-commands.txt` and
  `sloplint-string-gaps.txt`),
  `tree-sitter.json`.
- Files dropped: `binding.gyp`, `bindings/c/`, `bindings/node/`,
  `bindings/python/`, `CMakeLists.txt`, `Makefile`, `pyproject.toml`,
  `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`, `.github/`.

## How src/ is produced

- `src/` is regenerated from the patched `grammar.js`/`src/scanner.c`
  with tree-sitter-cli 0.27.1 (`tree-sitter generate --abi 15`): the
  output carries `#define LANGUAGE_VERSION 15`, which the workspace
  runtime (`tree-sitter 0.27.2`) loads. Allow sufficient memory
  for generation. The 2026-11-26 reproduction used a 32 GiB Depot
  Ubuntu 24.04 runner, peaked at 25,860,624 KiB resident memory, and
  completed generation in 21 minutes 37.67 seconds without swapping.
  The earlier 10.5 GiB observation did not bound the complete recipe.
- `src/scanner.c` is upstream plus the sloplint delta: an external
  scanner for dotted and quoted identifiers, nested block/doc/module-doc
  comments, raw string literals with matched `#` counts, and the
  layout-sensitive `_indent`, `_dedent`, `_newline` tokens that delimit
  tactic and do-block bodies, extended with `mark_end`-guarded word
  peeks, replay-free multi-level dedents, widened identifier
  characters, string-only gaps with exactly one physical line break,
  and token-end marks on completed block comments and raw
  strings so a same-line token after `by` keeps its full span
  (see the grammar patch). The indentation stack is the
  complete serialized scanner state.
- `tree-sitter test` builds the plain `src/parser.c` (see "Corpus
  results"). After a fresh clone, first inflate the
  committed copy: `gzip -dc src/parser.c.gz > src/parser.c`. Purge
  `$XDG_CACHE_HOME/tree-sitter` before rerunning after any future edit,
  per `vendor/README.md`.

## Size

`src/parser.c`: 97,115,282 bytes, 2,571,893 lines. States: 18895
(`STATE_COUNT`), 12150 large. Symbols: 648. Token count: 435. External
tokens: 12 (`EXTERNAL_TOKEN_COUNT`). Fields: 57. ABI 15
(`LANGUAGE_VERSION`). To keep the large generated diff out of Git,
`src/parser.c` is ignored and `src/parser.c.gz`
(~5.5 MB) is committed; `bindings/rust/build.rs` inflates it into
`OUT_DIR` when the plain file is absent.

## Corpus results

`tree-sitter test --debug-build` (CLI 0.27.1): all 323 cases pass. The
string-gap regressions cover ordinary and interpolated strings, CRLF,
invalid character-literal gaps, forbidden blank lines after a gap, and
ordinary multiline string text.

The [2026-11-26 Depot reproduction](https://github.com/gosuda/sloplint/actions/runs/45761114903)
started from the exact upstream pin above, applied `sloplint-grammar.patch`,
and byte-compared all 11 patched source inputs before generating the parser.
Its temporary producer commit was
`e90a73f92ec04bf37d2f3465b884cf13918a5a62`. The generated files were downloaded
from artifact `15091111568` and verified against the run's source and output
hashes before being committed here. The generator and corpus steps passed;
the run's later Rust step stopped at an unrelated Alloy formatting failure
before executing Rust tests.

The [follow-up Rust run](https://github.com/gosuda/sloplint/actions/runs/45765398446)
used those exact committed parser bytes at
`bc2067d1ec888789e6584dc6a24e8c67d1b0b506`. Workspace formatting, complete
workspace Clippy with warnings denied, all 24 Lean adapter regressions, a
CLI build, and the contributor self-lint gate passed on Depot Ubuntu 24.04.
This follow-up tested the generated parser without regenerating it.

- Generator: tree-sitter CLI 0.27.1, ABI 15.
- Generator archive SHA-256:
  `d01042f94ba3d6827ddc9788e235f75d8366431be42bdf90ade534d960c98b7d`.
- Artifact ZIP SHA-256:
  `9bb2f7b1fd055bdcf49daf98b416afdb006f9e4c669010989e7508ac24c05288`.
- Committed `src/parser.c.gz` SHA-256:
  `c6a503fd51535258b83c0edff4acf5d4df6d523d79514fa08160c34cc8f54b68`.

## Scanner progress regression

The scanner must either consume input or change serialized state whenever
it emits a token. A previous `pending_newline` replay flag was deliberately
excluded from serialization; a GLR recovery branch could therefore reuse
the same zero-width newline indefinitely at one source position.

The scanner now marks the start of a layout scan. Dedents pop one stack
entry without consuming the physical newline. Indents and newlines mark
their consumed input, so there is no transient replay state to restore.

Before this change, the nested malformed tactic in
`malformed_nested_tactic_keeps_progress_and_later_declarations` exceeded
10,000 parser progress callbacks. The corrected scanner returns a tree
with the syntax error and the following definition intact. The callback
budget is a regression-test guard, not a production fallback.

Focused validation used the committed generated parser and tree-sitter
0.27.2 runtime: all 314 vendored corpus sources produced byte-identical
trees before and after the scanner change. A bounded scan of 402
elegant-mermaid Lean files completed with no resource failures, retained
all 72 previously clean files, and reported 330 syntax failures. Those
grammar coverage gaps remain separate work; terminating a parse does not
make it valid.

## Node compatibility

The sloplint Lean adapter (`crates/sloplint-syntax/src/lang/lean/`)
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
String gaps use a dedicated external token, aliased to `escape_sequence`
inside ordinary and interpolated strings. Character literals retain only
ordinary escapes. The scanner requires exactly one LF or CRLF and rejects
another newline before content, while ordinary multiline text stays valid.
The `escape_sequence` node shape is retained, so the adapter does not change.

The lexical contract follows Lean 4.19.0's
[`quotedCharCoreFn` and `stringGapFn`](https://github.com/leanprover/lean4/blob/v4.19.0/src/Lean/Parser/Basic.lean).
[`mkInputContext`](https://github.com/leanprover/lean4/blob/v4.19.0/src/Lean/Parser/Extension.lean)
normalizes CRLF to LF before Lean parses it; the scanner accepts either
physical line ending while preserving source offsets.

Unmapped, with reasons:

- Loops: `for x in xs do` now has a dedicated `for_loop` node (round 2);
  `while` inside `do` blocks still parses as generic `app` chains, and
  recursion is a `def` whose body contains an `app` self-reference.
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
