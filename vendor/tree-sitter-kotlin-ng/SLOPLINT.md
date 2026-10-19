# tree-sitter-kotlin-ng, vendored for sloplint

Status: PARTIAL. 11 of the 17 named Kotlin clusters are fixed. 6 are open, listed below. K21b and K21c belong to the skipped KFILEANN cluster.

## Origin

- Upstream: https://github.com/tree-sitter-grammars/tree-sitter-kotlin, tag v1.1.0, commit 77dd60ea0a9003ce062c9728a513ffe1aaff8c82.
- Published crate: tree-sitter-kotlin-ng 1.1.0 (crates.io). Its `grammar.js` and `src/scanner.c` are byte-identical to the upstream tag.
- Copied unchanged from the published crate: `Cargo.toml`, `Cargo.toml.orig`, `Cargo.lock`, `README.md`, `bindings/rust/`, `tree-sitter.json`.
- Copied from the upstream tag, because the published crate does not ship it: `LICENSE` (MIT).
- Copied from the upstream tag: `test/corpus/declarations.txt`, `expressions.txt`, `semi.txt`.
- Added: `test/corpus/sloplint_kotlin.txt` (14 cases, one per fixed repro) and `test/corpus/sloplint_kotlin_same_line.txt` (4 cases for same-line bodies).
- Changed by hand: `grammar.js`, `src/scanner.c`.
- Regenerated: `src/parser.c`, `src/grammar.json`, `src/node-types.json`, `src/tree_sitter/*`.
- `sloplint-grammar.patch` is the diff of `grammar.js` and `src/scanner.c` against the upstream tag. It applies with `git apply --check` in a checkout of that tag.

## How results are measured

Every clean or error claim here comes from the raw tree: `tree.root_node().has_error()`, with `to_sexp()` scanned for `ERROR` and `MISSING`.

- `analyze` is not used for these claims. Tree-sitter inserts zero-width `MISSING` nodes during error recovery, and those are hidden from the cursor and from `children()`. The base `first_error_pos` walks the cursor, so on a tree whose only error is a hidden `MISSING` it returns no position, and `analyze` reports no parse error. Earlier probes through `analyze` reported `parse_error=false` on inputs whose raw tree had `has_error=true`.
- The `tree-sitter` CLI text output hides these nodes too. Its exit code does not match the raw `has_error()` either, so it is not used.
- `tree-sitter test` caches compiled grammars under `$HOME`. Each corpus run here used a new `HOME`, and each Rust run used a new cargo target directory.

## Root cause fixed

A class member followed by `}` on the same line. The scanner emitted `_class_member_semi` only after a newline. The parser recovered with a zero-width `MISSING _class_member_semi` inside `class_body`, which is hidden from the cursor. The fix in `src/scanner.c`: when `_class_member_semi` is valid, `_semi` is not, and the next token is `}`, emit a zero-width `_class_member_semi`.

This one change also fixes enum entries whose bodies close on the same line (KENUMBODY, K22b), one-line local classes (K08, K08b, K08c), and `class A { fun f(): Int = 1 }`.

## Fixed (11 named clusters)

KCTX, KSUSP-L, KNULLREF, KDCOLON, KSTR5, KDOLLAR, KENUMENTRY, KGUARD (one condition before `if`), KGETVAL, KSETNEST, KENUMBODY.

Repros that are clean on the vendored raw tree (19 of 27): K01, K02, K03, K04, K06, K07, K08, K08b, K08c, K09, K10, K11, K12, K15, K16, K17, K18, K22, K22b.

Limits:
- KGUARD accepts one condition before `if`. A guard after a comma-separated condition list is not handled.
- KGETVAL and KSETNEST are fixed for the repro shapes. The corpus sweep below is the broader check.
- KCTX is fixed for declarations, function types, and parenthesized receivers. Other context positions are covered only by the sweep.

## Open (6 named clusters)

1. KSUSP-N (K05): `suspend` as an expression identifier, `fun g(suspend: Int, x: Int): Int = suspend + x`. The named label `g(suspend = 1)` is accepted. Making `suspend` reserved caused grammar conflicts.
2. KACCSET (K13): annotated setter after an `=` getter, `@Suppress("X") set(value) { ... }`. A scanner change that skipped the annotation was tried and reverted: it made `ChannelProducerConsumerBenchmark.kt` line 43 fail with `MISSING "get"`, a file the published grammar parses cleanly.
3. KSPLITCTOR (K14): a constructor split from its class header by a newline or an annotation.
4. KSUSPPAREN (K19): `suspend ((Int) -> String?)`. A modifier prefix on `parenthesized_type` is ambiguous with `function_type`.
5. KPARENVAL (K20): `(fn)(call, it)` callee inside a lambda. Needs a declared conflict.
6. KVARARGARR (K23): `vararg val f: G = []`. `[]` must be limited to annotation-class parameter defaults. A grammar rule cannot express that without a dedicated rule, and a broader rule would accept invalid code.

Not isolated: KFILEANN (K21b, K21c), inference-only, skipped as assigned. Both grammars report `MISSING` on them. They are not counted as clean controls.

## Not fixed, found while measuring

- `class C { fun f() = 1 fun g() = 2 }` is valid Kotlin: kotlinc 2.4.21 exits 0 with no diagnostics. Both grammars report an error on it. It is not a named cluster and was not fixed in this lane.

## Pre-existing acceptances (both grammars clean; kotlinc rejects)

- `set(s, 1)` as an accessor on a property (`var s: String = "d"` then `set(s, 1)`). kotlinc: syntax error, expecting `)` and a function body. Raw tree: clean on both grammars. Not introduced here.
- `::class` on its own line inside a function body. kotlinc: "class literals with empty left hand side are unsupported", a diagnostic that is not a syntax error. Raw tree: clean on both grammars. Not introduced here.

## Validation (raw trees)

Corpus: 3688 `.kt` and `.kts` files under kotlinx.coroutines and ktor, plus the 27 repros.

| | clean | ERROR | MISSING only |
|---|---|---|---|
| Published | 3587 | 96 | 5 |
| Vendored | 3661 | 26 | 1 |

Transitions, published to vendored, over the 3688 corpus files: clean to clean 3587; ERROR to clean 70; MISSING to clean 4; ERROR to ERROR 26; MISSING to MISSING 1; clean to error 0. The 74 files that become clean were all error or missing on the published raw tree and clean on the vendored raw tree. None was replaced with a hidden `MISSING`.

Repros: all 27 are raw errors on the published grammar. On the vendored grammar, 19 are clean, 5 are ERROR (K05, K13, K14, K19, K20), and 3 are MISSING only (K21b, K21c, K23).

Rust gap tests, `crates/sloplint-syntax/tests/grammar_gaps/kotlin.rs` (24 tests, each clean check and invalid control on the raw tree):
- Vendored path dependency: 24 pass.
- Published `tree-sitter-kotlin-ng` 1.1.0: 20 fail and 4 pass. The 4 that pass are the invalid controls (`local class without a separator`, `enum entry body without a separator`, `empty array literal`, `getter without a body`).

Corpus cases (`tree-sitter test`, fresh `HOME`):
- Vendored: 40 of 40. These are the 22 upstream cases, the 14 `sloplint_kotlin.txt` cases, and the 4 `sloplint_kotlin_same_line.txt` cases. The expected trees have no `ERROR` or `MISSING`.
- Published copy (verified to match the registry crate's `grammar.js`, `src/parser.c`, and `src/scanner.c`): the 14 sloplint cases and 4 same-line cases all fail. The 22 upstream cases pass on the published grammar; that was measured before any change.

Invalid controls, kotlinc 2.4.21:
- `enum class E { A { fun f() = 1 } B }`: syntax error. Vendored raw tree: ERROR. Published: ERROR.
- `fun f() { class C { fun g(): Int = 1 } val m = C::class.java }` (no separator): syntax error. Vendored raw tree: ERROR. Published: ERROR.
- `var x: Int = 1` followed by `get()` with no body: syntax error. Vendored raw tree: ERROR.
- `val x = []` outside annotations: kotlinc error. Vendored raw tree: MISSING.

## Code notes

- `get_call_follows` advances the lexer before it returns false. Tree-sitter rewinds on a false return, so behavior is correct, but the helper should restore the position explicitly.
- `context` was added to `_reserved_identifier`. Identifiers named `context` are lexed as a keyword in some states. No test covers this.
