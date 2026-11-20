# tree-sitter-scala, vendored for sloplint

## Origin

- Upstream: https://github.com/tree-sitter/tree-sitter-scala, tag `v0.26.3`, commit `851032fc97297bbe299dabe5bd826f778794d03c`.
- Published crate: `tree-sitter-scala` 0.26.3 (crates.io). `grammar.js`, `src/scanner.c`
  and `src/parser.c` of the published crate are byte-identical to the upstream tag.
- Copied verbatim from the published crate: `Cargo.toml`, `Cargo.toml.orig`, `Cargo.lock`,
  `LICENSE`, `README.md`, `bindings/`, `queries/`, `tree-sitter.json`.
- Copied from upstream: `test/corpus/*.txt` (10 files, 305 cases, all passing before and after).
- Changed: `grammar.js` (see `sloplint-grammar.patch`), and `src/` regenerated.
- Added: `test/corpus/sloplint_endnew.txt`, `sloplint_parenif.txt`, `sloplint_instar.txt`,
  `sloplint_softkw.txt`, `sloplint_enum.txt`.

## Regeneration

- Tool: tree-sitter CLI 0.27.1 (pinned binary), run in this directory: `tree-sitter generate`.
  The generation is warning-free for this grammar.
- Pinned CLI on the unpatched upstream grammar does not reproduce the published `src/`
  byte-for-byte. `grammar.json` is identical. `parser.c` and `node-types.json` differ:
  the published `node-types.json` lists the hidden supertypes `_definition` and `_pattern`,
  and the 0.27.1 output expands them to concrete kinds in child-type lists. The visible node
  kinds are identical (266 = 268 − the two hidden supertypes). `src/scanner.c` carries one
  sloplint edit (SC-SOFTENUM, below): `enum` removed from the postfix-operator heuristic's
  `definition_words` list.
- Test: `tree-sitter test` in this directory: 318/318 (305 upstream + 13 sloplint cases).

## Changes to grammar.js

4. SC-SOFTENUM: `enum` joined the `_soft_identifier` set, so Scala 2 sources may
   use it as a parameter name or named argument (`def f(enum: E)`, `Map(enum = 1)`)
   while `enum Foo { ... }` still parses as `enum_definition`. A
   `[enum_definition, _soft_identifier]` GLR conflict keeps both readings alive,
   `enum_definition` gained `prec(1)`, and `enum` dropped out of
   `_namespace_path_segment`'s explicit alias since `_identifier` now covers it.

1. SC-ENDNEW: a new `instance_expression` alternative for `new T:` with an empty body
   closed by `end new`. Upstream's indented template needs a body statement, so the marker
   read as one and its tag word errored. The alternative puts the marker directly after the
   indent. A dedicated alternative keeps the shared template rule unchanged.
2. SC-PARENIF: `_if_condition_then` gains a second alternative, `split_call_condition`:
   a call whose argument list starts on the next line (`if x.exists` + newline +
   `(x => x == 10) then`). The function and arguments keep their fields, so the Scala
   adapter reports the same call fact as the single-line form.
3. SC-INSTAR: `infix_pattern` admits the shared `_asterisk` token as its operator, as
   `infix_type` already does. This makes `val P(a, b * c) = p` parse.
4. The conflict `[$.repeat_pattern, $._simple_expression]` is removed (generator reported it
   unnecessary). The conflict `[$.block, $.capture_set]` is moved, with identical content.
5. SC-SOFTKW: Scala 3 keywords keep their Scala 2 name readings where the corpus needs
   them. `parameter`/`class_parameter` name slots accept `using`/`given` (filodb's
   `WrongNumberArguments(given: Int, …)`); `_argument_list` and `_exprs_in_parens` accept
   `using = v`/`given = v` as `keyword_named_argument`, aliased to `assignment_expression`
   for node parity (`@JsonDeserialize(using = classOf[X])`, jackson-module-scala's
   pervasive idiom). Scoped to argument lists only: at statement level the keyword must
   not shadow the identifier read (`using()` is a call).


   Round 2 (review-driven):
   - `nameChoice` also admits `using`/`given` as plain identifiers, so `f(1, using())`,
     `using.Context` qualified types, and expression-position `using`/`given` parse.
     `given_definition` in `_definition` carries `prec.dynamic(10)` so the definition
     read still wins the statement-head fork (a bare `given T is U:` with an indented
     body otherwise loses to a call/infix-expression read when a statement follows).
   - `parameter_types` gained a `("(" "using" [erased] FunArgTypes ")")` arm for
     context function types, superseded in round 3 (below) by
     `_context_parameter_types`, which binds `?=>` only.
   - `class_parameter` splits its name slot: `paramName` (nameChoice minus
     `using`/`given`) needs no type, while a `using`/`given`-named parameter must be
     typed — `(using = 1)` after an annotation is therefore the annotation's argument
     list, never an untyped parameter.
   - items.rs `ambient_head`: an `assignment_expression` nested under `arguments`
     is a named argument, not a write — `g(using = 4)` no longer reports GlobalWrite
     on a file-scope `using` (mirrors `write_roots`' existing guard).

   Round 3 (review-driven):
   - `function_type` splits context parameter types into
     `_context_parameter_types` (aliased to `parameter_types`), bound to the
     `?=>` arrow only: `(using T) => R` is now a parse error, matching the
     Scala 3 rule that context function types require `?=>`. Conflict pairs
     move from `parameter_types` to `_context_parameter_types` accordingly.
   - scanner.c `definition_words` drops `given` and gives it the `enum` peek:
     after a postfix-operator decision point, `given` + identifier-start/`=`
     emits `postfix_sym` (new statement) and anything else keeps the infix
     operand read, so `x +` newline `given(1)` parses as a call operand
     instead of a `given_definition`.
   - items.rs `report_assign_write`: only a bare `identifier` left side skips
     the named-argument guard, so `sink(obj.x = v)` still reports a GlobalWrite
     on `obj` — a field path can't name a parameter — while `sink(name = v)`
     stays a named argument.

   Round 4 (review-driven):
   - `keyword_named_argument`'s right side is now
     `choice(statementExpression($), $.indented_block)`, the same set
     `assignment_expression` accepts, so `f(using = do {} while (x))` and
     multiline block values parse instead of erroring.
   - scanner.c `given` peek: after the postfix-operator decision point a `[`
     skips the balanced bracket group and classifies on the follower —
     `=>`, `:`, an identifier start, `_` or a backtick stays a definition,
     anything else (including `(`) is a postfix operand. `given[Int](1)`
     after a line-ending operator now parses as a generic call operand.
   - scanner.c `given` peek: `given(` is a postfix operand unless the `(` is
     directly followed by `using` that is not followed by `=`, so
     `given(using x)`/`given(using = v)`-style operand calls join the infix
     while `given (using T)` definitions keep their read.
   - items.rs `named_arg_verdict` walks the scope chain innermost-first and
     the first scope declaring `def fname` decides, so a member method
     shadows a same-named file-scope signature; def names, parameter names
     and argument labels all pass through `unquote` so a backquoted
     parameter matches its plain argument label.
   - items.rs `push_pattern_names`: `given` extractor patterns only skip the
     binding when more than one `pattern` field is present, so
     `case given(status)` binds `status` while the tuple-type reading
     `case given (status, String)` still binds nothing.
   - items.rs write machinery moved to `scala/writes.rs` (`write_roots`,
     `is_compound_write`, `report_assign_write`, `named_arg_verdict`) to
     keep the adapter under the 600-line self-lint budget.

## Fixed shapes (scalac 3.10.0 `-Ystop-after:parser` accepts each)

- SC-ENDNEW: `trait Foo` / `val foo =` / `  new Foo:` / `  end new` (empty body).
  Also with a comment-only body. Real occurrence: `tests/pos/i24250.scala`.
- SC-PARENIF: `if x.exists` / `(x => x == 10) then`. Real occurrence: `tests/pos/i12757.scala`.
- SC-INSTAR: `case class *(...)` with `val P(a, b * c) = p`. Real occurrence:
  `tests/pos/t5702-pos-infix-star.scala`.
- SC-SOFTKW: `@JsonDeserialize(using = classOf[MyDeserializer])` and `given: Int`.
  Real occurrences: twitter/util `caseclasses.scala`, linkerd `H2Config.scala`,
  feathr `FeatureDefinition.scala`, filodb `ComputedColumn.scala`. Round 2 adds
  `f(1, using())`, `(using using.Context)`, `(using T) ?=> R` context function types,
  `class E @deprecated(using = 1)(x: Int)`, and `given T is U:` before a following
  top-level statement. The `@deprecated(using = 1)` shape is an SLS-2 form
  (jackson-module-scala compiles under Scala 2.13; scalac 3's `parArgumentExprs`
  treats a leading `using` in parens as a context marker, so this bullet is a
  2.13-dialect shape the grammar deliberately admits).

## Still failing (residual)

- SC-POLYARG, not fixed. `[X] => (_: List[X]).headOption` (`cats` FunctionKLiftSuite.scala:31)
  and `[t] =>` / `_ match` (`tests/pos/i23774typeclass.scala:67`).
  Attempted: a body restricted to placeholder-led forms. The `_ match` alias produced
  malformed trees (scrutinee dropped, zero-width `match_expression` nodes). The
  `(_: T).member` form needed conflict forks that could not be resolved without admitting
  term-parameter bindings. The body is reverted to upstream `choice(parenthesized_expression,
  block)`.
- Over-acceptance, pre-existing gap: the end-marker alignment is not checked. The SC-ENDNEW
  alternative accepts `  val x = new Foo:` / `  end new` (misaligned; scalac rejects with
  "misaligned end marker"). The base already accepts the misaligned non-empty form.
- Invalid shapes checked to stay errors: polymorphic bodies `[A] => f(x)`, `[A] => x.foo`,
  `[A] => if c then x else y`, `[A] => { case _ => 1 }`, `[A] => a + b`, `[A] => x: Int`,
  bare `[X] => x`.
- Residue, not isolated (valid scalac, not reduced to a minimal repro):
  `tests/pos/26238.scala`, `tests/pos/i11581.scala`.
- Experimental-flag and spec-string constructs, from the sweep skip list (not individually
  re-verified in this round): `applied_constructor_types.scala` (modularity), `closure-args.scala`
  (preview colon application), `i24496.scala` (relaxedLambdaSyntax), `inline-match-sub-cases.scala`,
  `match-single-sub-case.scala`, `match-sub-sub-cases.scala`, `sub-cases-exhaustivity.scala`,
  `sub-cases-reachability.scala` (subCases), `singleton-ctx-bound.scala` (modularity /
  erasedDefinitions), `spec-strings.scala`.
- Out of scope: `tests/pos/i19642/i19642.java` (Java).

## Verification

Corpora: `cats`, `scala3-library` (sloplint `check`, product criterion: a visible ERROR or
MISSING node).

| | published 0.26.3 | vendored |
|---|---|---|
| cats parse-error files | 1 | 1 (residual) |
| scala3-library parse-error files | 17 | 14 |
| fixed (error → clean) | | 3: i12757, i24250, t5702-pos-infix-star |
| clean → error | | **0** |

- Clean under both grammars: 6029 files. Trees (S-expression) changed: **0**.
- Findings: cats 3319 → 3319. scala3-library 10450 → 10449. Non-fixed-file deltas excluding
  `grep/homonym-definitions` counts: 0. Homonym counts rose by exactly 1 for 7 names
  (defined in newly parsed files).

Fail-proof (`tests/grammar_gaps/scala.rs`, through `analyze`; dependency swapped to the
published crate for the proof, then restored):

| test | published | vendored |
|---|---|---|
| split_applied_if_condition_parses (SC-PARENIF) | FAIL | PASS |
| empty_new_template_with_end_marker_parses (SC-ENDNEW) | FAIL | PASS |
| comment_only_new_template_with_end_marker_parses (SC-ENDNEW) | FAIL | PASS |
| infix_star_extractor_patterns_parse (SC-INSTAR) | FAIL | PASS |
| single_line_applied_if_condition_still_parses | PASS | PASS |
| nonempty_new_template_with_end_marker_still_parses | PASS | PASS |
| empty_new_template_without_end_marker_still_errors | PASS | PASS |
| dangling_polymorphic_function_body_still_errors | PASS | PASS |
| star_repeat_and_vararg_still_parse | PASS | PASS |
| typed_term_parameter_polymorphic_function_still_parses | PASS | PASS |
| keyword_named_arguments_and_parameter_names_parse (SC-SOFTKW) | FAIL | PASS |
| context_function_parameter_types_parse | FAIL | PASS |
| annotation_arguments_named_using_stay_annotation_arguments | FAIL | PASS |
| given_is_definition_before_a_following_statement | FAIL | PASS |

The five FAILs are the parse-error assertion ("valid Scala must parse cleanly"), not harness
panics. The vendored run is 11/11.

### SC-SOFTENUM (later round)

`enum` is a Scala 3 keyword but a legal Scala 2 identifier. Dogfooding the corpus
(the-algorithm, phantom) showed ~50 files failing on `enum` used as a parameter
name or named argument. Upstream fails the same files — this is a genuine gap,
not a regression. The scanner's postfix-operator heuristic also had `enum` in its
`definition_words` list, so `val n = 1 +` + newline + `enum` was split. `enum`
now stays out of that list except when a following token proves the definition
reading — a name start (`enum Foo`) or `=` (`enum = 1`, a standalone assignment
to a variable named `enum`) keep the statement-break hint; anything else makes
it the right operand (`1 +\nenum`, `1 +\nenum(2)` — an enum declaration
always names itself first, so `(` can only be a call of the identifier),
matching the existing treatment of `extension`. `/:/`-style operators (`def /:/`, `x /:/ y` — breeze) remain a
documented upstream-parity ceiling: an operator may not end in `/` per SLS 1.1,
and the scanner's trailing-slash path only covers non-slash-initial operators.
`<removed_id>` placeholders in the-algorithm are sanitized-out identifiers, not
valid Scala — corpus-content ceiling.

Corpus delta (same files, same grammar build):

| repo | files with parse errors before | after |
|---|---|---|
| the-algorithm | 42 | 14 (all `<removed_id>` placeholders) |
| phantom | ~10 | 0 |
| scalding | ~8 | 6 (multi-line curried signatures — residual cluster) |
| breeze | ~12 | 4 (`/:/` ceiling + unexamined residuals) |

Round: `given` continuation peeks now cover `given [A] =>`, `given (T, U) = …`/`with`/`extends`, and non-ASCII given names after a line-broken operator; `given(using = 2)` keeps the call read. `nameChoice` (the soft-keyword identifier set this PR routes through) feeds parameter and argument names AND general expression positions, bindings, and qualified types — keyword-as-identifier support is deliberately broader than named arguments alone.

Round: `given` operand lookaheads read complete tokens.
- After `given` and after its `[T]` group, an expression-tail keyword
  (`match`, `catch`, `finally`, `else`, `then`, `do`, `yield`, `while`,
  `with`, `extends`) keeps the operand read, so `x +` + `given[Int] match
  {…}` stays one initializer.
- A lone `=` assigns the variable (`given = 1`); `=` continuing into an
  operator is infix (`given == y`, `given(2) == 3`), and `:` after a
  balanced group is the call's type ascription, not a declaration marker.
- Non-ASCII lookahead now separates identifier starts from \p{Sm}
  operators (`given ≤ 2` compares; `given É` still names a given), using
  the same SM_NONASCII ranges as grammar.js.
- The `[]`/`()` balancing loops skip strings (including `"""`), char and
  symbol literals, backquoted identifiers, and comments, so `) =` inside
  `given("x) = y")` no longer reads as declaration syntax.
