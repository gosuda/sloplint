# tree-sitter-kotlin-ng, vendored for sloplint

Status: PARTIAL. Of the 17 named Kotlin clusters in the sweep, 10 are fixed and 7 are open. The open clusters are listed below with their causes.

## Origin

- Upstream: https://github.com/tree-sitter-grammars/tree-sitter-kotlin, tag v1.1.0, commit 77dd60ea0a9003ce062c9728a513ffe1aaff8c82.
- Published crate: tree-sitter-kotlin-ng 1.1.0 (crates.io). Its `grammar.js` and `src/scanner.c` are byte-identical to the upstream tag.
- Copied from the published crate unchanged: `Cargo.toml`, `Cargo.toml.orig`, `Cargo.lock`, `README.md`, `bindings/rust/`, `tree-sitter.json`.
- Copied from the upstream tag, because the published crate does not ship it: `LICENSE` (MIT).
- Copied from the upstream tag: `test/corpus/declarations.txt`, `expressions.txt`, `semi.txt`.
- Added: `test/corpus/sloplint_kotlin.txt` (14 cases, one per fixed repro).
- Changed by hand: `grammar.js`, `src/scanner.c`.
- Regenerated: `src/parser.c`, `src/grammar.json`, `src/node-types.json`, `src/tree_sitter/*`.
- `sloplint-grammar.patch` is the diff of `grammar.js` and `src/scanner.c` against the upstream tag. It applies with `git apply --check` in a checkout of that tag.

## Regeneration

- Tool: tree-sitter 0.27.1 (`.outline/tools/tree-sitter-cli/bin/tree-sitter`).
- In this directory: `tree-sitter generate`, then `tree-sitter build` for a shared library, then `tree-sitter test`.
- `src/scanner.c` is edited by hand and is not generated.
- `tree-sitter test --update` wrote the expected trees for `sloplint_kotlin.txt`. Each expected tree was checked for ERROR and MISSING nodes: there are none.

## Fixed shapes (14 repros, each validated with kotlinc 2.4.21)

| Cluster | Repro | Shape |
|---|---|---|
| KCTX | K01 | `context(x: String)` before a declaration |
| KCTX | K02 | `context(String, Int)` in a function type |
| KCTX | K03 | `(context(S) () -> R)` in a parenthesized function type |
| KSUSP-L | K04 | `val l = suspend { 1 }` |
| KNULLREF | K06 | `String?::plus` callable reference |
| KDCOLON | K07 | `::h.toString()` statement after a local function |
| KSTR5 | K09 | `"""ab"""""` five-quote closer |
| KSTR5 | K10 | `"""a"\"""""` escaped quote before a five-quote closer |
| KDOLLAR | K11 | `$$"hello"` |
| KDOLLAR | K12 | `$$"""..."""` |
| KENUMENTRY | K15 | enum entry named `enum` |
| KGUARD | K16 | `is X if cond ->` (one condition) |
| KGETVAL | K17 | `get("/x") { }` call after a value |
| KSETNEST | K18 | `set(s, g(s))` call after a value |

Limits of these fixes:
- KGUARD accepts a single condition before `if`. A guard after a comma-separated condition list is not handled.
- KGETVAL and KSETNEST are fixed for the repro shapes. The sweep below is the corpus-level check.
- KCTX is fixed for declarations, function types, and parenthesized receivers. Other context positions in the corpus are covered only by the sweep.

## Still failing (open, 7 clusters)

1. KSUSP-N (K05): `suspend` as an expression identifier. `fun g(suspend: Int, x: Int): Int = suspend + x` is valid Kotlin and is rejected. Named labels (`g(suspend = 1)`) are accepted. Making `suspend` a reserved identifier caused grammar conflicts, so it was not done.
2. KACCSET (K13): annotated setter after an `=` getter (`@Suppress("X") set(value) { ... }`). A scanner change that skipped the annotation was tried and reverted. It made `ChannelProducerConsumerBenchmark.kt` line 43 fail with `MISSING "get"`, a file the published grammar parses cleanly.
3. KSPLITCTOR (K14): a secondary or actual constructor split from its class header by a newline or annotation.
4. KSUSPPAREN (K19): `suspend ((Int) -> String?)`. A modifier prefix on `parenthesized_type` is ambiguous with `function_type`.
5. KPARENVAL (K20): `(fn)(call, it)` callee inside a lambda. Needs a declared conflict; not completed.
6. KENUMBODY (K22b): two non-empty enum entry bodies. A single entry body parses.
7. KVARARGARR (K23): `vararg val f: G = []`. `[]` must be limited to annotation-class parameter defaults. A grammar rule cannot express that without a dedicated rule, and a broader rule would accept invalid code.

Not isolated: KFILEANN (inference-only, skipped as assigned) and 10 residue files.

Pre-existing over-acceptance, present in both the published and the vendored grammar (kotlinc rejects both inputs):
- `set(s, 1)` as an accessor on a property (`m_settwo`).
- A newline-separated `::class` with no receiver (`m_dcolon_nl`). A grammar rule requiring a receiver for `class` broke an upstream case, so it was reverted.

## Validation

- Upstream corpus plus sloplint corpus (`tree-sitter test`): 36 of 36 on the vendored grammar. On the published grammar: the 22 upstream cases pass and the 14 sloplint cases fail.
- Sweep of 3688 `.kt` and `.kts` files under kotlinx.coroutines and ktor, classified by the presence of ERROR or MISSING nodes:
  - Published: 3591 clean, 97 with error nodes.
  - Vendored: 3661 clean, 27 with error nodes.
  - Transitions: 3591 clean to clean, 70 error to clean, 27 error to error, 0 clean to error.
- Repros (27): 14 fixed (table above). The 7 open clusters are K05, K13, K14, K19, K20, K22b, K23. Valid controls K08, K08b, K08c, K21b, K21c and K22 are clean on both grammars.
- Rust gap tests, `crates/sloplint-syntax/tests/grammar_gaps/kotlin.rs` (18 tests):
  - Vendored path dependency: 18 pass.
  - Published `tree-sitter-kotlin-ng` 1.1.0: the 14 fix tests fail, and the 4 control tests pass.
- Invalid controls that still report an error on the vendored grammar: `val x = []` (kotlinc: array literals outside annotations are unsupported), and a `get()` accessor with no body (kotlinc: expecting function body).

## Code notes

- `get_call_follows` advances the lexer before it returns false. Tree-sitter rewinds on a false return, so behavior is correct, but the helper should restore the position explicitly.
- `context` was added to `_reserved_identifier`. Identifiers named `context` are lexed as a keyword in some states. No test covers this.
