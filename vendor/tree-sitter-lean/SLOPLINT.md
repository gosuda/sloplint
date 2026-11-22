# tree-sitter-lean, vendored for sloplint

Date: 2026-11-21.

## Origin

- Upstream: `https://github.com/Julian/tree-sitter-lean`, default branch
  `main`, commit `0d28e5d6e8c4755f5df7ef9bde3b6ec6f59fae94` dated
  2026-11-01 (`Merge pull request #30`, HEAD on 2026-11-19; the repository
  has no tags and no releases).
- Licence: MIT (`LICENSE`: `Copyright (c) 2024 Julian Berman`).
- Rust crate name and version are unchanged: `tree-sitter-lean 0.2.0`
  (`Cargo.toml`). The crate is unpublished on crates.io and npm; the
  `Cargo.toml` `repository` already names the upstream repository.
- `sloplint-grammar.patch` carries the grammar delta vs the pinned
  upstream. Round 2 (2026-11-21) covers, on top of the original
  `open <ns> renaming a → b` fix:
  - `do`-block layout: `try … catch … finally` without separator tokens
    (the scanner now peeks clause keywords at same-indent and marks the
    line end first, and `pending_newline` is a one-shot in-memory flag —
    tree-sitter only serializes scanner state on a successful `scan()`,
    so restoring it in `deserialize` resurrected suppressed newlines).
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
  - Multi-level dedents: when a newline lands several indent steps up
    (a `for … do` or `finally` body followed by a column-0 `def`), the
    scanner records a `dedent_target` column after the first `_dedent`
    and keeps emitting `_dedent` — without consuming input — on the next
    scans until the indent stack reaches it. Previously only one dedent
    per newline could ever be produced, so the following declaration
    parsed inside the still-open inner block and errored.
  - Known upstream-parity gaps (identical ERROR on the pinned upstream,
    unfixed by convention): `simpa using h`, `rcases h with ⟨…⟩`,
    `first | …` alternation, `∀ᶠ`, a comma after a tactic in `by` blocks
    (`by t, u` parses as `app(t, ERROR, u)` identically upstream), and
    `open … hiding`/`export` modifier words landing in
    `namespace`/`custom_command` nodes. (`open only Nat` now parses
    `only` as a real first namespace — identical to upstream.)
  - Generated-artifact note: `src/parser.c` (~97MB) is not committed;
    `src/parser.c.gz` is, and `bindings/rust/build.rs` inflates it into
    `OUT_DIR` when the plain file is absent.
- `Cargo.toml` has one addition: an empty `[workspace]` table after the
  `[dev-dependencies]` section. Without it `cargo fmt --all` reports the
  vendored package as belonging to the parent workspace and fails. No other
  manifest change.
- Files kept from the revision: `Cargo.toml`, `LICENSE`, `README.md`,
  `grammar.js`, `queries/` (`folds`, `highlights`, `indents`, `injections`,
  `locals`), `src/` (`grammar.json`, `node-types.json`, `parser.c.gz`,
  `scanner.c`, `tree_sitter/`), `bindings/rust/` (`build.rs`, `lib.rs`),
  `test/corpus/` (7 files), `tree-sitter.json`.
- Files dropped: `binding.gyp`, `bindings/c/`, `bindings/node/`,
  `bindings/python/`, `CMakeLists.txt`, `Makefile`, `pyproject.toml`,
  `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`, `.github/`.

## How src/ is produced

- `src/` is regenerated from the patched `grammar.js`/`src/scanner.c`
  with tree-sitter-cli 0.27.1 (`tree-sitter generate --abi 15`): the
  output carries `#define LANGUAGE_VERSION 15`, which the workspace
  runtime (`tree-sitter 0.27.2`) loads. Generation peaks at ~26 GB RSS
  and ~40 minutes — keep grammar deltas small vs a known-good generate;
  the kernel OOM-kills past ~26-27 GB and swap does not help.
- `src/scanner.c` is upstream plus the sloplint delta: an external
  scanner for dotted and quoted identifiers, nested block/doc/module-doc
  comments, raw string literals with matched `#` counts, and the
  layout-sensitive `_indent`, `_dedent`, `_newline` tokens that delimit
  tactic and do-block bodies — extended with `mark_end`-guarded word
  peeks, a non-serialized `pending_newline`, a `dedent_target` column
  for multi-level dedents, and widened identifier characters (see the
  grammar patch).
- Verification: `tree-sitter test` in this directory, 313/313 corpus
  cases pass (see "Corpus results"). `tree-sitter test` builds the
  plain `src/parser.c`, so after a fresh clone first inflate the
  committed copy: `gzip -dc src/parser.c.gz > src/parser.c`. Purge
  `$XDG_CACHE_HOME/tree-sitter` before rerunning after any future edit,
  per `vendor/README.md`.

## Size

`src/parser.c`: 97,113,146 bytes, 2,571,806 lines. States: 18895
(`STATE_COUNT`), 12150 large. Symbols: 647. Token count: 434. External
tokens: 11 (`EXTERNAL_TOKEN_COUNT`). Fields: 57. ABI 15
(`LANGUAGE_VERSION`). Over GitHub's 100 MB blob limit when combined with
the rest of the diff, so `src/parser.c` is gitignored and `src/parser.c.gz`
(~5.5 MB) is committed; `bindings/rust/build.rs` inflates it into
`OUT_DIR` when the plain file is absent.

## Corpus results

`tree-sitter test` (cli 0.27.1, this directory, committed parser):

- Total parses: 313; successful parses: 313; failed parses: 0.

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
