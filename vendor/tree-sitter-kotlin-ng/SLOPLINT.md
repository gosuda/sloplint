# tree-sitter-kotlin-ng, vendored for sloplint

Status: PARTIAL. 18 of the 19 named Kotlin clusters are fixed for their repro shapes. 3 are open. The residual files below are listed by file, with their shape.

Round 14 adds loop statements to `if`/`else` branch bodies (K-IFLOOP):
`if (m) for (c in ys) D()`, `else while (d) g()`, `if (a) for ((p, q) in m)
{ … }`, `if (m) do { … } while (d)`. Upstream v1.1.0 has only
`block`/`expression`/`assignment`/`;` as branch bodies, so `for` lexed as
an identifier — the braced form `if (m) for (c in ys) { D() }` silently
mis-parsed as a `for(c in ys)` call with a trailing lambda (no ERROR), and
the bare-expression form errored at the call site. `for_statement`,
`while_statement` and `do_while_statement` join all three branch choices,
with three new conflict declarations (`statement`/`assignment`,
`_property_declaration_head`, `delegation_specifiers`) for the merged
if-then state.

Round 13 closes a word-scan hole: `scan_words` uses `iswalnum`, which in the C locale stops at non-ASCII bytes — `isÉ` scanned as `is`, so `var c = 1; get\nisÉ()` lost its bare getter to the `as`/`is`/`in` infix branch. That branch now also requires the lookahead to not continue the word (`iswalnum || '_' || > 127` is rejected), so `isÉ()` reads as a call on the next line and `get` keeps its accessor slot. Note the flag that prompted the investigation, `x\nis Int` at file level, stays an error — `is` also opens `when` entry conditions, so the newline scanner cannot unconditionally continue on it (upstream parity: it fails identically without the `;`). A `;`-followed `get`/`set`/`field` word then reads as an identifier exactly the way a plain `x` does there.

Round 2 adds: K-SOFTID (`open`/`override` usable as identifiers), K-PARENANNO (annotation on a parenthesized function type), K-ARRLIT (empty `[]` in an annotation argument), K-NEWLINECTOR (a newline or doc comment between the class name and the primary-constructor parameter list, which resolves the KSPLITCTOR shape), and K-ARRLIT also resolves the KVARARGARR repro (`= []`). A fresh dogfood pass over 274 corpus files previously flagged with parse errors leaves 171 still erroring; the cleared 102 are concentrated in installerx-revived (Room `indices = []`, `@Composable` paren types), facebook-android-sdk, dbflow, and mlkit (doc comment before the constructor).


Round 3 repairs a K-NEWLINECTOR regression: a `(`-led line after a *complete* statement (`fun g() {}`, `do { } while (c)`, `val x: Int`, `class D {}`) again takes its separator — the scanner now suppresses the semi only where the `constructor` keyword is valid (the declaration-head position) or where a nullable-chain/`!is` continuation is still open. The same round binds `suspend` in `suspend (Int).() -> Unit` to the function type rather than the parenthesized receiver: parenthesized types now take annotations only, via `paren_type_modifiers`.

Round 4 covers two continuations the scanner cannot disambiguate. `class D : Base\n() {}` fails because the `(` after `Base` carries the same valid-symbol mask as a `(` after a complete statement, so the semi cannot be suppressed; instead the delegation's constructor invocation absorbs an optional `_semi` (`_delegation_constructor_invocation`, aliased to `constructor_invocation` so the node shape is unchanged). `suspend ((A) -> B)` — ktor's JWT verifier returns this shape — fails because no rule admits a type modifier before a parenthesized type; a second `parenthesized_type` arm at dynamic precedence −1 accepts `suspend` there, while `suspend (Int).() -> Unit` keeps binding `suspend` to the enclosing function type.

Round 5 adds: trailing commas in `type_parameters`/`type_arguments` (`fun <T,>`, `box<Int, String,>()`), unnamed context parameters (`context(_: T)`), untyped anonymous-function parameters (`fun(m) = m`, `fun(x) { return x }` — named functions still require types), and secondary-constructor heads split across lines (`@Anno(args)` then modifiers then `constructor` — `actual`/`expect` added to the modifier scan). A recount over 294 corpus files previously flagged with parse errors leaves 138 still erroring (aerial-tv 25, neriplayer 12, mpandroidchart 12, facebook-android-sdk 10 — the `dynamic` soft type, backtick function names, and `suspend` identifiers dominate the tail).

Round 6 adds: `var x = 1; private set` — a `;` before an accessor (`get`/`set`/`field`, optionally behind annotations and modifiers) no longer acts as a member/statement separator. A real `;` can never lex as a literal `';'` where externals are valid — the scanner emits exactly one of SEMI/CLASS_MEMBER_SEMI — so it now peeks past whitespace/comments, annotations and modifier words; when the next real word is `get`/`set`/`field` and a property tail is reachable it emits the new `_accessor_semi` external, which only `property_declaration`'s tail accepts. No LR ambiguity: putting `_class_member_semi` inside the property tail was tried first and swallowed every post-property member separator (any member following an accessor, or another property, broke); the distinct token makes the separator ownership unambiguous. Works at member and file level (`var top = 1; private set`). Scanner robustness: `MAX_WORDS` 16 -> 20 so the 17-word modifier table fits; a `skip_string` helper keeps parens inside literals out of the depth counter; the `@`-branch annotation loop gained an EOF guard; `crossed_modifier` marks consumed modifier words so `actual as T` still separates; a `}` same-line after a member emits CLASS_MEMBER_SEMI when valid. Two new `sloplint_kotlin_same_line` corpus cases pin the `;`-accessor trees (50 of 50 pass).

Round 7 hardens the `;`-accessor probe for real-corpus shapes: every whitespace skip inside the probe now crosses `//` and nested `/* */` comments too (`var x = 1; get(/*note*/) = 2`), a getter/setter body may start on the line after the signature (`get()\n= 2`), the setter's parameter list may open with an annotation or a backtick-quoted name (``set(@Deprecated("x") `v`) {}``), and comments inside the parameter list no longer end the balanced-paren scan early. The lookalike rejects still apply: `get(2)`, `set(v)`, `set_up()`, `set2()`, and `get = 2` after `;` stay statements. On the sloplint side, `fun(/* c */ x)` and `fun(@Anno y)` bare params now bind their name — the delimiter check walks back past `line_comment`/`block_comment`/`parameter_modifiers`/`annotation` siblings instead of only accepting an immediate `(` or `,` sibling. A new same-line corpus case pins the comment/annotation/newline shapes (51 of 51 pass), and the `mutable-span` Kotlin near-miss fixture covers the commented and annotated binding paths end to end.

Round 8 fixes bodyless delegations: `class T : M<Int>` (and `Marker<Int,>`) at a line end died because the declaration-terminating `_semi` shifted into `_delegation_constructor_invocation`'s tolerated separator, which then demanded `(`. The tolerated separator is now a dedicated external `_delegation_semi`, emitted instead of `_semi` only when the separator is followed by `(`; a `_semi` after a delegation type now always ends the declaration. The `x\n(y)` expression split is unchanged (two statements). Two corpus cases pin the shapes (55 of 55 pass).

Round 9 fixes statement-position properties picking up accessor tails. Kotlin gives accessors and explicit backing fields to member and top-level properties only, but `statement` routed through the same `declaration` — so `var x = 1; field = 5` in a function body bound `field = 5` as a backing field, and `; get() {}` became a getter instead of a call with a trailing lambda. `property_declaration` is now split into a shared `_property_declaration_head` plus the tail; statement positions (blocks, `if`/`while`/`do`/`for`/`when` branches, lambda bodies) use `_local_statement`, whose `local_property_declaration` ends at the initializer and is aliased to `property_declaration` so the node kind is unchanged. File-level and member-level properties keep the full tail. The tail also accepts accessors after a backing field (`field = it` then `get() = field.value`), which was unreachable before. Two corpus cases pin the shapes (57 of 57 pass).

Round 10 reparents the split so `statement` stays queryable: the local-position rule is now `statement` itself (blocks, branches, lambda bodies and `when` arms use it), whose `local_property_declaration` members keep the `property_declaration` node kind via the alias, so `(statement) @match` and `(block (statement) ...)` custom-rule queries compile and match local statements again — before this round they were inlined members of the hidden `_local_statement`, which made the block-scoped pattern an impossible pattern at query-compile time. File level uses `_file_statement` (`declaration` plus the statement rest), keeping full accessor tails and the annotated-declaration resolution at top level. The round also accepts `;` around accessors inside the tail (`field = 1; get() = field`, `get(); set()`) and drops the now-unreachable `modifiers`/`annotated_lambda` conflict. Three corpus cases pin the shapes (58 of 58 pass).

Round 11 fixes two scanner edges: newlines inside a `/* */` comment no longer detach an accessor after `;` (Kotlin does not insert a semi inside a block comment — `var x = 1; /* a\nb */ get() = x` keeps its getter; `//` comments still end the property since their terminating newline is a real break), and `scan_words` now treats `_`/digits as identifier continuations so `constructor_legacy()`/`constructor2` after a bodyless class are calls, not constructors. The vendoring patch is resynced to cover the `fun(open)` parameter and accessor fixes. The operator lookalikes `get() == x`/`set(v) == x` and `field::class` are rejected (`=`/`:` followed by itself is never an accessor body or field prefix), and a bare accessor followed by a newline always ends the property — a next-line word like `42` in `set\n42.toString()` starts a new statement, not an infix callee. (64 of 64 corpus cases pass.)

Round 12 closes three `;`-accessor gaps from dogfooding. Kotlin ignores a newline before the hard infix keywords `as`/`is`/`in`, so `var x = 1; get\nas Int` is the cast `get as Int` — a `get`/`set`/`field` probe word followed by a newline and `as`/`is`/`in` now stays a statement separator (`as?` shares the `as` word; `!is`/`!in` already read as the `!` lookalike). Second, the accessor slot is single-use: `_property_tail` offers `getter` then `setter` (or the reverse), so after `var x: Int get() = 1` only `set` remains valid — the probe now checks `valid_symbols[GET]`/`valid_symbols[SET]` for the scanned word, and `; get` where `GET` is no longer offered lexes `get` as an identifier (`get\nfoo()` is a call, not a duplicate getter). Third, `skip_ws_comments_nl` treated `\r`/`\r\n` as ordinary whitespace while the `;` scanner counted them as line breaks — `\r` now sets the newline flag there too, so `set\r42.toString()` behaves like `set\n42.toString()`.

## Origin

- Upstream: https://github.com/tree-sitter-grammars/tree-sitter-kotlin, tag v1.1.0, commit 77dd60ea0a9003ce062c9728a513ffe1aaff8c82.
- Published crate: tree-sitter-kotlin-ng 1.1.0 (crates.io). Its `grammar.js` and `src/scanner.c` are byte-identical to the upstream tag.
- Copied unchanged from the published crate: `Cargo.toml`, `Cargo.toml.orig`, `Cargo.lock`, `README.md`, `bindings/rust/`, `tree-sitter.json`.
- Copied from the upstream tag, because the published crate does not ship it: `LICENSE` (MIT).
- Copied from the upstream tag: `test/corpus/declarations.txt`, `expressions.txt`, `semi.txt`.
- Added: `test/corpus/sloplint_kotlin.txt` (18 cases, one per fixed repro) and `test/corpus/sloplint_kotlin_same_line.txt` (7 cases for same-line bodies).
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

## Fixed (16 named clusters, for their repro shapes)

KCTX, KSUSP-L, KNULLREF, KDCOLON, KSTR5, KDOLLAR, KENUMENTRY, KGUARD (one condition before `if`), KGETVAL, KSETNEST, KENUMBODY, KBFIELD, K-SOFTID, K-PARENANNO, K-ARRLIT, K-NEWLINECTOR.

Per-cluster corpus attribution was not measured. The corpus differential is file-level, and a file's first error cannot be assigned to a cluster without a per-cluster triage that I did not complete. The fixed status is established by the repro shapes only.

Limits:
- KGUARD accepts one condition before `if`. A guard after a comma-separated condition list is not handled.
- KGETVAL and KSETNEST are fixed for the repro shapes.
- KCTX is fixed for `context(...)` before a declaration, a function type, and a parenthesized receiver. `context(...)` after an annotation is not handled (see residual ERROR files).
- KBFIELD covers the Kotlin 2.x explicit backing field in the accessor position: `val x: T` followed on the next line by `field = expr` or `field: U = expr`, optionally with modifiers (`private field = false`), annotations, or comments between `field` and its `:`/`=` (`field /* c */ = x`, `field /*/a/b*/ = x`, `private /* note */ field = x`). The grammar gains a named `backing_field` rule beside `getter`/`setter` in `property_declaration` that accepts `optional($.modifiers)`, `optional(':' $.type)`, `= $.expression`. The semicolon scanner suppresses the auto-semi before `field` when, after skipping whitespace and `//` or nested `/* */` comments, a `:` or `=` follows (`::` and `==` still split). A typed `field:` tail suppresses wherever `_semi` is valid (a `field:` statement is never legal), while a `field =` tail suppresses only at class member position because it is ambiguous with a plain assignment to an identifier named `field` — a bare identifier cannot start a class member. An untyped `field = v` at top level therefore stays an assignment statement, which parses without error but does not produce a `backing_field` node; and `field = v` inside a function body stays an assignment.

The modifier word scan also skips comments so `private /* note */ field = v` still attaches, but a comment crossed there no longer lets a continuation keyword suppress the separator: `else`/`in`/`instanceof`/`by`/`as`/`where` after a comment-crossed modifier word emit the semi exactly as upstream does when a comment ends the word scan (`x\nfoo /* c */ as Int` splits instead of merging). The accessor keywords `get`/`set`/`constructor`/`field` keep their own checks after a comment, which is an improvement over upstream, where a comment between a modifier and `get`/`set` already broke the chain.

## Open named clusters (4)

1. KSUSP-N (K05): `suspend` as an expression identifier, `fun g(suspend: Int, x: Int): Int = suspend + x`. The named label `g(suspend = 1)` is accepted. Making `suspend` reserved caused grammar conflicts (`suspend '{'` collides with `lambda_literal`, `suspend '@'` with `function_modifier`/`type_modifiers`); it was dropped from `_reserved_identifier` in round 2 for that reason. kotlinc reports `suspend` used as a bare expression only as a type or invocation error, so the expression form is not a syntax control. `dynamic` was tried in `_reserved_identifier` too and reverted: it collides with the `dynamic` type literal at `'.'`/`'?'` follow positions.
2. KACCSET (K13): annotated setter after an `=` getter. A scanner change that skipped the annotation was tried and reverted: it made `ChannelProducerConsumerBenchmark.kt` line 43 fail with `MISSING "get"`, a file the published grammar parses cleanly.
3. KSUSPPAREN (K19): `suspend ((Int) -> String?)`. A modifier prefix on `parenthesized_type` is ambiguous with `function_type`.
4. KPARENVAL (K20): `(fn)(call, it)` callee inside a lambda. Needs a declared conflict.

KSPLITCTOR (K14) is fixed in round 2 by the scanner change below. KVARARGARR (K23) parses as of round 2 because `collection_literal` now permits zero elements; the acceptance is broader than kotlinc's annotation-only rule (see over-acceptance notes).

## Residual files: hidden MISSING, no ERROR (4)

These four have `has_error=true` and a `MISSING` node but no `ERROR` node. They are the files sloplint would silently mis-analyze on a `5bbe796`-based tree.

- `kotlinx-coroutines-core/common/src/flow/terminal/Collection.kt` (corpus). Its first line is `@file:JvmMultifileClass @file:JvmName("FlowKt")`, two file annotations on one line. kotlinc 2.4.21: no syntax error. This is the KFILEANN shape. KFILEANN was skipped as inference-only in the sweep, and kotlinc now shows the syntax is valid, so it is a real over-rejection that this lane does not fix.
- K21b and K21c (repros). The same KFILEANN shape. kotlinc: no syntax error.
- K23 (repro). KVARARGARR, listed above.

## Residual files: ERROR (26 corpus files, by first error)

Counts are by the first `ERROR` node only. The 26 files contain no same-line member closed by `}`: that shape, which this lane fixes, accounts for none of them.

| Shape | Files | Files, first error |
|---|---|---|
| KSPLITCTOR (constructor split from header, or annotation before a constructor) | 10 | Channel.kt, EmbeddedServer.posix.kt, EmbeddedServerJvm.kt, EmbeddedServer.web.kt, EngineTestBaseNix.kt, EngineTestBase.web.kt, SessionsBuilder.kt, CharsetJVM.kt, and two DefaultPool.kt files (common and jvm). The newline-separated shape is fixed in round 2; the annotation-separated shape (`class C @Anno (x)`) remains open. |
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
- Over-rejection of a feature-gated construct: `val x = []` outside annotations. Without a flag kotlinc reports `array literals outside of annotations are unsupported`. With `-Xcollection-literals` the only remaining error is `cannot infer type for type parameter 'T'`, so the syntax is valid. Round 2 accepts `[]` in every position (the `collection_literal` grammar cannot scope empties to annotation arguments), so the vendored grammar now over-accepts the unflagged form — the same lint-tooling tradeoff as the pre-existing `set(s, 1)` note above.

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

Rust gap tests, `crates/sloplint-syntax/tests/grammar_gaps/kotlin.rs` (52 tests, each clean check asserting the raw tree and each invalid control asserting the raw tree has an error):
- Vendored path dependency: 52 pass.

Corpus cases (`tree-sitter test`, fresh `HOME`):
- Vendored: 57 of 57. These are the 22 upstream cases, the 22 `sloplint_kotlin.txt` cases, the 7 `sloplint_kotlin_same_line.txt` cases, and the 6 `sloplint_softids.txt` cases. The expected trees have no `ERROR` or `MISSING`.
- Published copy, verified to match the registry crate's `grammar.js`, `src/parser.c`, and `src/scanner.c`: 0 of 29 pass (the 22 sloplint cases and the 7 same-line cases). The 22 upstream cases passed on the published grammar before any change.

## Code notes

- `get_call_follows` advances the lexer before it returns false. Tree-sitter rewinds on a false return, so behavior is correct, but the helper should restore the position explicitly.
- `context` was added to `_reserved_identifier`. Identifiers named `context` are lexed as a keyword in some states. No test covers this.
- Round 2: `open` and `override` joined `_reserved_identifier` (conflicts added against `inheritance_modifier` and `member_modifier`). `parenthesized_type` gained `optional($.type_modifiers)` with `prec.dynamic(1)` so `@Composable (() -> Unit)` binds the annotation inside the paren rather than guessing `nullable_type` (a `[$.parenthesized_type]` self-conflict covers the receiver ambiguity). `collection_literal` permits zero elements. The scanner suppresses `_semi` before `(` only when neither `_q_dot` nor `_not_is` is valid — after a class/object name `x\n(y)` keeps parsing as a primary constructor, while in expression position `x\n(y)` still splits into two statements.
- Round 3: `_reserved_identifier` dropped to `prec.dynamic(0)` so an annotated modifier no longer loses to an infix-expression reading — `@Deprecated("x") open class C {}` is a `class_declaration` again while `open`/`override` still parse as plain identifiers in expression position. `callable_reference` accepts `prec.dynamic(2, parenthesized_expression)` beside `_receiver_type` so `(v)::class.java` on a value receiver keeps a `parenthesized_expression` (real type shapes like `(Pair<A, B>)::class` still go `parenthesized_type`); new conflict `[$.callable_reference, $.primary_expression]`. Residual: `suspend (Int).() -> Unit` binds `suspend` inside `parenthesized_type` rather than at the function-type level (a tree-shape divergence, not an error), and a `(`-led line after a local `fun`/class/`do`/`val` can still merge into the prior declaration because no external symbol marks the decl-head state.
- Round 4: `type_parameters`/`type_arguments` accept a trailing comma. `context_parameters` accepts `_:` entries. `anonymous_function` takes `alias($._untyped_function_value_parameters, $.function_value_parameters)`, where each entry is `choice($.parameter, $.identifier)` — bare names allowed without changing the emitted node kinds; two conflicts (`_untyped_function_value_parameters` × `_simple_user_type`, × `function_type_parameters`) cover the `(` vs `<` fork. The `@`-then-`constructor` scanner branch now loops per annotation (skipping balanced-paren argument lists and ws/comments) and then scans the modifier-word table including `actual`/`expect` before matching `constructor`. `scan_words` buffers the word it scanned even when it doesn't match, so a follow-up check must go through `scan_words` (it re-checks the buffer) rather than re-reading the lexer — reading lookahead after a missed `scan_words` sees the character *after* the word.

- Round 5: `_file_statement` is `choice(declaration, statement)` so `statement` stays in `source_file`'s children and `(source_file (statement))` custom-rule queries compile again; `local_property_declaration` carries `prec.dynamic(-1)` so the full property (accessor tail) wins at top level. The accessor probe distinguishes three semis: `_field_semi` (word `field`, valid only at the tail start — `val t = 1; get() = 1; field = 2` parses the last clause as an assignment) and `_accessor_semi` for `get`/`set`, which now requires `(` unless a modifier/annotation preceded the word (`private set` still binds; `get.run()` and `get = 2` stay statements). The stray `kotlin.so` (built without the vendored name) was dropped and `kotlin-ng.so` rebuilt; `bindings/rust/build.rs` compiles the C sources directly and uses neither binary.

Round 6 adds bare default accessors after a semicolon: `var x = 1; set` and
`var x = 1; get` now parse, matching the newline-separated form. The
`get`/`set` probe rejects only expression continuations (`=`, `.`, binary
operators, `)`, `]`, `,`, `:`); `get = 4` at class level remains an error
since a getter body still needs `get() = expr`.
Round 7 keeps script statements out of the accessor probe: `set[i] = v`,
`get { }`, `get as T`, and the other word-form infix operators (`is`, `in`,
`to`, `by`, `and`, `or`, `xor`, `shl`, `shr`, `ushr`) reject the token, so
indexed assignments and word-operator expressions after `;` stay statements.
Round 7b generalizes the accessor-probe word check: Kotlin infix callees
are arbitrary identifiers, so instead of a fixed operator list the probe
rejects any non-member word followed by an expression start (`get until 2`,
`x foo y`, `get as Int`), while member keywords and modifiers (`val`,
`private`, `set`, ...) still let the bare accessor through.
Round 8 validates what follows a member word in the accessor probe: after a
member-set match, an identifier-continue character (`context2`, `datax`)
rejects, and the next token decides — member keywords take a keyword or a
name (`get data class`), while a literal, operator or paren reads the word
as an infix callee (`get context 2`, `get data(2)`, `get open -1`,
`get data < 2`). `context`/`init`/`companion` pin narrow followers
(`context(`, `init {`, `companion object`), and `(`/`:`/`<` are member
syntax only for `constructor`/`val`/`var`/`get`/`set`, `object`, and `fun`
respectively.

## Round 9

The `;`-accessor probe's member-word follower check closed three more
shapes: a bare `get`/`set` before `}` or `;` ends its own member
(`var x = 1; get\nset }`), `<` after `val`/`var` opens a generic
extension property (`val <T> List<T>.head`), and `(` after `fun` opens a
parenthesized extension receiver (`fun (String).ext()`). Only the
accessor words themselves may close at `}`/`;` — `get data }` still
fails inside `data`.

## Round 10

The member-word follower check rejects a repeat of the same accessor
word — `var x = 1; get\nget` (or `set\nset`) can never be a second
accessor of the same kind, so the second word now reads as a script
statement instead of a duplicate getter the parser would reject.

## Round 14 (loop statements as if/else branch bodies)

Adds `for_statement`/`while_statement`/`do_while_statement` to the
`if`/`else` branch choices (upstream only allowed block/expression/
assignment), with conflict declarations covering the new merge points
(`statement`/`assignment`, `_property_declaration_head`,
`delegation_specifiers`).

Known inherited limitations (verified byte-identical on the pinned
upstream v1.1.0 parser, not introduced by this round):

- An unbraced loop body on the next line (`for (i in xs)\n  g(i)`) ends
  the loop after `)` and parses the body as a sibling statement — the
  scanner's `_semi` wins over `optional(choice(block, statement))`. This
  happens identically at statement level upstream; the same misparse is
  now reachable inside `if`/`else` branches, where upstream produced an
  equally wrong `call_expression(for, i in xs)` mis-shape instead.
- `;` immediately before `else` (`if (c) x; else y`, any branch kind)
  errors: a real `;` lexes as the external `_semi`, never as the literal
  `';'` the `if_expression` rule lists, so the `optional(';')` path is
  dead. Upstream errors identically on `if (x) f(); else g()`.
