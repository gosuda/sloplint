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
  for it, so there is no published crate to diff against. `grammar.js` diverges from the pinned revision by
  `sloplint-grammar.patch` (regenerated parser artifacts checked in
  alongside it); the changes below are all corpus-driven, verified against
  the upstream pin and the corpus tests.
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

The committed `src/parser.c`, `src/grammar.json` and `src/node-types.json`
are regenerated from the patched `grammar.js` below: ABI 15
(`#define LANGUAGE_VERSION 15`), `STATE_COUNT 1422`, no external scanner
(`src/` holds no `scanner.c`). Regenerating the *pinned* grammar.js with
0.27.1 reproduces the pre-patch parser byte-identically (STATE_COUNT 624),
which is what `sloplint-grammar.patch` diffs against.

## Upstream corpus status

The upstream corpus passes under the patched grammar: `tree-sitter test`
reports 83 parses, 83 successful, 0 failed — including the
"primed identifier with dot" case that fails against the untouched pin
(82/83 upstream). The grammar README states coverage of the full Alloy 6
syntax (temporal operators, `var` fields, trace-style `run` commands)
validated against 331 real-world `.als` files at a 100% parse rate.

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

- `test/corpus`: 88 cases, 88 pass (83 upstream + 5 sloplint round-2).

## Divergences from the pin (sloplint-grammar.patch)

All of these come from real corpora (the Practical Alloy book models and
Electrum/Alloy-6 code in the dogfood corpus), each hit repeatedly:

- `sig_decl`: allow `sig A = B + C` subset declarations with no brace body.
- `field_decl`/`decl`: accept `disjoint` as a synonym of `disj`, and
  both `disj`/`disjoint` remain usable as declared names
  (`sig S { disjoint: S }`).
- `compare_expression`: word-prefixed comparisons — `x not = y`, `x ! in e`,
  `x not < y`.
- Negative literals parse via `neg_expression`, bound above `->` so
  `-1 -> x` is `(-1) -> x` rather than `-(1 -> x)` (`Node0->-8`,
  `>= -1`).
- Arrow operators (`some`/`one`/`lone`/`set` `->`) may omit the space
  before `->`; `set->`/`one->` are common in the wild.
- `dot_expression` member position accepts operator/keyword words
  (`e.before`, `x.after`) via `member_keyword`; `let` is left out so an
  `e.let x = a | b` member expression keeps parsing.
- `box_join_expression` callee accepts `member_keyword` (`sum[i + j]` calls
  a field named `sum`).
- `set_comprehension` without a `|` bar: `{x: A, y: B}`.
- `name` accepts non-ASCII letters (`État`) and a `"` tail (upstream
  parity); primed decl names (`t'`) parse via `_decl_name` = name +
  `'` literals — the same token `prime_expression` uses, so
  `no trashed'` (multiplicity + prime) and `no t': T` (decl)
  disambiguate on what follows instead of failing lexically.
- `decl_list` accepts a trailing comma (`pred p[t: Time, ]`) — real
  models carry it.
- `_semicolon_decoration`: runs of 2+ `;` at paragraph level (`;;;;;;`
  separators in textbook models).

Round 2 (2026-11-20 dogfood corpus):

- `_decl_name` (primed declarator names) is used for pred/fun/assert
  declaration names too: `assert add_1`, `pred step'`, `fun lookup'`.
- Declaration names may be sig-scoped (`optional(sig_ref (`.` | `::`))`
  prefix on pred/fun decls), e.g. `pred MySig.helper`.
- `assert` may carry a string name (`assert "every book has an address"`).
- Qualified-path segments may be declaration keywords (`_qname_seg`):
  `pred/totalOrder`, `fun/add`, `module/x` — real util-module file names.
  Only keywords that are not standalone expressions are admitted, so
  `univ`/`none`/`Int` stay expression-only.
- `qual_path` (at least one `/`) is the operator of
  `infix_qual_call_expression` (`n1 fun/add n2`) and a `box_join`
  callee (`pred/totalOrder[]`).
- `scope_expression`: `Sig::member` in expression position
  (`GrandParent::parents`), binding like the dot join.
- Infix calls admit only keyword-led operators (`a fun/add b`); a
  name-led path (`x a/b y`) is ambiguous with adjacent block
  expressions and stays a known gap.
