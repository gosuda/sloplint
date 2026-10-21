# tree-sitter-kotlin-ng, vendored for sloplint

Status: PARTIAL. 11 of the 17 named Kotlin clusters are fixed for their repro shapes. 6 are open. The residual files below are listed by file, with their shape.

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

- `analyze` is not used for these claims. Tree-sitter inserts zero-width `MISSING` nodes during error recovery, and those are hidden from the cursor and from `children()`. The base `first_error_pos` walks the cursor, so on a tree whose only error is a hidden `MISSING` it returns no position, and `analyze` reports `parse_error=None`. Earlier probes through `analyze` reported `parse_error=false` on inputs whose raw tree had `has_error=true`.
- On a tree branched from `5bbe796`, this means sloplint would treat a file with only a hidden `MISSING` as fully analyzed. The merged tree's `root_end_pos` fallback reports those files; this lane's tests do not rely on it.
- The `tree-sitter` CLI text output hides these nodes too, and its exit code is not a reliable raw check. It is not used here.
- Cache rule: `tree-sitter test` caches compiled parsers under `$HOME`, and does not rebuild when the grammar name matches. A reused `HOME` silently tests a different grammar. One run of this lane reported 18 of 18 on the published copy; that run had reused a `HOME` holding the fixed grammar, so the number was wrong. Every corpus number here comes from a fresh `HOME`. Every Rust run uses a fresh `CARGO_BUILD_BUILD_DIR` and `CARGO_TARGET_DIR`.

## Root cause fixed

A class member followed by `}` on the same line. The scanner emitted `_class_member_semi` only after a newline. The parser recovered with a zero-width `MISSING _class_member_semi` inside `class_body`, which is hidden from the cursor. The fix in `src/scanner.c`: when `_class_member_semi` is valid, `_semi` is not, and the next token is `}`, emit a zero-width `_class_member_semi`.

This one change also fixes enum entries whose bodies close on the same line (K22b), one-line local classes (K08, K08b, K08c), and `class A { fun f(): Int = 1 }`.

## Fixed (11 named clusters, for their repro shapes)

KCTX, KSUSP-L, KNULLREF, KDCOLON, KSTR5, KDOLLAR, KENUMENTRY, KGUARD (one condition before `if`), KGETVAL, KSETNEST, KENUMBODY.

Per-cluster corpus attribution was not measured. The corpus differential is file-level, and a file's first error cannot be assigned to a cluster without a per-cluster triage that I did not complete. The fixed status is established by the repro shapes only.

Limits:
- KGUARD accepts one condition before `if`. A guard after a comma-separated condition list is not handled.
- KGETVAL and KSETNEST are fixed for the repro shapes.
- KCTX is fixed for `context(...)` before a declaration, a function type, and a parenthesized receiver. `context(...)` after an annotation is not handled (see residual ERROR files).

## Open named clusters (6)

1. KSUSP-N (K05): `suspend` as an expression identifier, `fun g(suspend: Int, x: Int): Int = suspend + x`. The named label `g(suspend = 1)` is accepted. Making `suspend` reserved caused grammar conflicts. kotlinc reports `suspend` used as a bare expression only as a type or invocation error, so the expression form is not a syntax control.
2. KACCSET (K13): annotated setter after an `=` getter. A scanner change that skipped the annotation was tried and reverted: it made `ChannelProducerConsumerBenchmark.kt` line 43 fail with `MISSING "get"`, a file the published grammar parses cleanly.
3. KSPLITCTOR (K14): a constructor split from its class header by a newline or an annotation.
4. KSUSPPAREN (K19): `suspend ((Int) -> String?)`. A modifier prefix on `parenthesized_type` is ambiguous with `function_type`.
5. KPARENVAL (K20): `(fn)(call, it)` callee inside a lambda. Needs a declared conflict.
6. KVARARGARR (K23): `vararg val f: G = []`. kotlinc 2.4.21 reports no syntax error on K23, so the input is valid. `[]` must be limited to annotation-class parameter defaults; a grammar rule cannot express that without a dedicated rule, and a broader rule would accept invalid code.

## Residual files: hidden MISSING, no ERROR (4)

These four have `has_error=true` and a `MISSING` node but no `ERROR` node. They are the files sloplint would silently mis-analyze on a `5bbe796`-based tree.

- `kotlinx-coroutines-core/common/src/flow/terminal/Collection.kt` (corpus). Its first line is `@file:JvmMultifileClass @file:JvmName("FlowKt")`, two file annotations on one line. kotlinc 2.4.21: no syntax error. This is the KFILEANN shape. KFILEANN was skipped as inference-only in the sweep, and kotlinc now shows the syntax is valid, so it is a real over-rejection that this lane does not fix.
- K21b and K21c (repros). The same KFILEANN shape. kotlinc: no syntax error.
- K23 (repro). KVARARGARR, listed above.

## Residual files: ERROR (26 corpus files, by first error)

Counts are by the first `ERROR` node only. The 26 files contain no same-line member closed by `}`: that shape, which this lane fixes, accounts for none of them.

| Shape | Files | Files, first error |
|---|---|---|
| KSPLITCTOR (constructor split from header, or annotation before a constructor) | 10 | Channel.kt, EmbeddedServer.posix.kt, EmbeddedServerJvm.kt, EmbeddedServer.web.kt, EngineTestBaseNix.kt, EngineTestBase.web.kt, SessionsBuilder.kt, CharsetJVM.kt, and two DefaultPool.kt files (common and jvm) |
| KACCSET (annotated setter) | 2 | ResponseType.kt, HttpRequest.kt |
| KSUSP-N (`suspend` as a parameter name) | 1 | Semaphore.kt |
| KPARENVAL | 1 | FormAuth.kt |
| KCTX, annotation before `context(...)` | 2 | PrincipalContext.kt (verified: `@ExperimentalKtorApi` then `context(authCtx: ...)`), SessionAuthenticationScheme.kt (same pattern, not separately verified) |
| KGETVAL family (`get { }` block after a value) | 1 | StaticContent.kt |
| Not triaged | 9 | ThreadLocalsLeaksTest.kt (error at 1:1), DebugProbes.kt (object header), YamlConfigTest.kt (`"${NON_EXISTING:DEFAULT}"`), TestApplicationTestJvm.kt, ServerSentEventsTest.kt, DarwinClientEngineConfig.kt, DarwinLegacyClientEngineConfig.kt (class headers), ReferenceOr.kt, JWTAuth.kt (`block: Verification.() -> Unit = {}`) |

## Pre-existing acceptances and over-rejections (not introduced by this lane)

- Over-acceptance of syntax-invalid input: `set(s, 1)` as an accessor on a property (`var s: String = "d"` then `set(s, 1)`). kotlinc 2.4.21 reports `syntax error: Expecting ')'` at 2:12 and `syntax error: Expecting function body` at 2:14. The raw tree is clean on both the vendored and the published grammar. Pre-existing, not fixed.
- `::class` on its own line in a function body. kotlinc reports `class literals with empty left hand side are unsupported`. That is a semantic diagnostic, not a syntax error. The raw tree is clean on both grammars, so it is not counted as a syntax over-acceptance.
- Over-rejection of valid syntax: `class C { fun f() = 1 fun g() = 2 }`. kotlinc accepts it (exit 0, no diagnostics). Both grammars report an error. Not in any named cluster; not fixed.
- Over-rejection of a feature-gated construct: `val x = []` outside annotations. Without a flag kotlinc reports `array literals outside of annotations are unsupported`. With `-Xcollection-literals` the only remaining error is `cannot infer type for type parameter 'T'`, so the syntax is valid. Both grammars report an error. Not a named cluster; not fixed; not a control.

## Invalid controls (kotlinc syntax error; raw error on the vendored grammar)

- `fun f() { class C { fun g(): Int = 1 } val m = C::class.java }`: kotlinc `syntax error: Unexpected tokens (use ';' to separate expressions on the same line)`. Vendored ERROR; published ERROR.
- `enum class E { A { fun f() = 1 } B }`: kotlinc `syntax error: Expecting ';' after the last enum entry or '}' to close enum class body`. Vendored ERROR; published ERROR.
- `var x: Int = 1` followed by `get()` with no body: kotlinc `syntax error: Expecting function body`. Vendored ERROR; published ERROR.

## Validation (raw trees)

Corpus: 3688 `.kt` and `.kts` files under kotlinx.coroutines and ktor, plus the 27 repros.

| | clean | ERROR | MISSING only |
|---|---|---|---|
| Published | 3587 | 96 | 5 |
| Vendored | 3661 | 26 | 1 |

Transitions, published to vendored, over the 3688 corpus files: clean to clean 3587; ERROR to clean 70; MISSING to clean 4; ERROR to ERROR 26; MISSING to MISSING 1; clean to error 0; clean to MISSING 0. The 74 files that become clean were all error or missing on the published raw tree and clean on the vendored raw tree.

Repros: all 27 are raw errors on the published grammar. On the vendored grammar, 19 are clean (K01, K02, K03, K04, K06, K07, K08, K08b, K08c, K09, K10, K11, K12, K15, K16, K17, K18, K22, K22b), 5 are ERROR (K05, K13, K14, K19, K20), and 3 are MISSING only (K21b, K21c, K23). The 8 open repros are 6 named clusters plus the KFILEANN pair.

Rust gap tests, `crates/sloplint-syntax/tests/grammar_gaps/kotlin.rs` (23 tests, each clean check asserting the raw tree and each invalid control asserting the raw tree has an error):
- Vendored path dependency: 23 pass.
- Published `tree-sitter-kotlin-ng` 1.1.0: 20 fail and 3 pass. The 3 that pass are the invalid controls.

Corpus cases (`tree-sitter test`, fresh `HOME`):
- Vendored: 40 of 40. These are the 22 upstream cases, the 14 `sloplint_kotlin.txt` cases, and the 4 `sloplint_kotlin_same_line.txt` cases. The expected trees have no `ERROR` or `MISSING`.
- Published copy, verified to match the registry crate's `grammar.js`, `src/parser.c`, and `src/scanner.c`: 0 of 18 pass (the 14 sloplint cases and the 4 same-line cases). The 22 upstream cases passed on the published grammar before any change.

## Code notes

- `get_call_follows` advances the lexer before it returns false. Tree-sitter rewinds on a false return, so behavior is correct, but the helper should restore the position explicitly.
- `context` was added to `_reserved_identifier`. Identifiers named `context` are lexed as a keyword in some states. No test covers this.
