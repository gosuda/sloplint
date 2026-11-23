# tree-sitter-scala, vendored for sloplint

## Origin

- Upstream: https://github.com/tree-sitter/tree-sitter-scala, tag `v0.26.3`, commit `851032fc97297bbe299dabe5bd826f778794d03c`.
- Published crate: `tree-sitter-scala` 0.26.3 (crates.io). `grammar.js`, `src/scanner.c`
  and `src/parser.c` of the published crate are byte-identical to the upstream tag.
- Copied verbatim from the published crate: `Cargo.toml`, `Cargo.toml.orig`, `Cargo.lock`,
  `LICENSE`, `README.md`, `bindings/`, `queries/`, `tree-sitter.json`.
- Copied from upstream: `test/corpus/*.txt` (10 files, 305 cases, all passing before and after).
- Changed: `grammar.js` and `src/scanner.c` (see `sloplint-grammar.patch`), and `src/` regenerated.
- Added: `test/corpus/sloplint_endnew.txt`, `sloplint_parenif.txt`, `sloplint_instar.txt`,
  `sloplint_oplam.txt`, `sloplint_multipat.txt`, `sloplint_softkw.txt`, `sloplint_enum.txt`,
  `sloplint_opcmt.txt`.

## Regeneration

- Tool: tree-sitter CLI 0.27.1 (pinned binary), run in this directory: `tree-sitter generate`.
  The generation is warning-free for this grammar.
- Pinned CLI on the unpatched upstream grammar does not reproduce the published `src/`
  byte-for-byte. `grammar.json` is identical. `parser.c` and `node-types.json` differ:
  the published `node-types.json` lists the hidden supertypes `_definition` and `_pattern`,
  and the 0.27.1 output expands them to concrete kinds in child-type lists. The visible node
  kinds are identical (266 = 268 − the two hidden supertypes). `src/scanner.c`
  carries the sloplint edits below (SC-SOFTENUM plus the operand-probe rounds):
  `enum` removed from the postfix-operator heuristic's `definition_words`
  list, and the `/`-tail and postfix operand probes gained the soft-keyword
  lookahead, line-comment transparency and blank-line termination rounds.
- Test: `tree-sitter test` in this directory: 331/331 (305 upstream + 26 sloplint cases).

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
4. SC-OPSLASH: the operator partition accepts `/`-led names that end in `/`
   (`def /:/`, breeze / twitter-algebird). The external `_op_left_*` tokens never claim
   a `/`-led operator, so the regex side now admits `/` bound to a following non-slash
   non-star opchar (`/[${OP_NSS}]${OP_RUN}` in the mul class, `/[${OP_NSS}]${OP_RUN_SLASH}`
   in `OP_ID_UNION`). `//` and `/*` stays excluded — a `/` followed by another `/` or `*`
   still opens a comment. `x/y` and `/x/` with a letter `x` are unchanged.
5. SC-WILDLAMBDA / SC-SEMI: `_: T =>` admits `wildcard` beside `operandName`
   in the unparenthesized typed-lambda slot (`{ _: Any => () }`, algebird's
   `prepareMonoid`); and a top-level separator may be a run of literal `;`
   (`package breeze.plot;;;`). SC-SELFWILD (new): `wildcard` is dropped from
   `_braced_typed_lambda`'s parameters and `self_type` is raised to dynamic 3,
   so `new T { _: (A, B) => ... }` reads as a wildcard self-type instead of a
   block-argument lambda or an error (a typed wildcard lambda is written
   `(_ : T) => ...`). SC-LEADSEMI (new): a stray literal `;` run may also lead
   the file (`;;;def x = 1`); it is literal-`;` only because a leading
   `_automatic_semicolon` made the separator boundary ambiguous.
6. SC-TYPEDLAMBDA: `lambda_expression` admits an unparenthesized typed parameter
   `a: T => body` in expression position (scalding's `case c => a: Array[Long] => new W(a)`).
   It is kept at dynamic -5, below colon-argument (0) and the ascription arrow tail (-1),
   so both keep their upstream trees wherever they can apply; the lambda only wins when
   the `=>` tail cannot parse as a type (`=> new …`, and expression bodies in general
   that are not valid function-type tails). `_braced_typed_lambda` is raised to dynamic 2
   so the braced form keeps winning inside `{ … }`.
7. The conflict `[$.repeat_pattern, $._simple_expression]` is removed (generator reported it
   unnecessary). The conflict `[$.block, $.capture_set]` is moved, with identical content.
8. SC-SYMNAME (revised): `quote_expression` names an operator (`'*)`, scalding) or
   the keyword `val` (`'row, 'col, 'val` column literals). The operator name is now
   a `token.immediate` regex (`(?:[noSlash(OP)]|/[OP_NSS])+|/`) at `PREC.macro`, so
   `' +` is rejected (the `'` must be adjacent to the name), a `/` never opens a
   comment, and the reserved spellings `'=`, `':`, `'<-`, `'->`, `'/` lex. A
   trailing `'` cannot be swallowed: `'='` stays a character literal because its
   three characters outmatch the two of `'=`.
9. SC-MULTIPAT: `val_definition`/`var_definition` take a comma-separated pattern
   list, so `val a2_1, List(a2_2) = …` (SLS PatDef ::= Pattern2 {',' Pattern2}) parses — a
   real form in scalameta's Issue1872 fixture. The first element keeps the full
   `_definition_pattern` (a lone `val x * y = v` infix pattern still parses); each
   comma successor is a `_comma_definition_pattern`, the same alternatives minus
   `infix_pattern`, so `val a, b | c` and `val a, b :: c` error while the
   parenthesized `(b | c)`/`(b :: c)` forms stay legal. This deliberately lands
   between the dialects: scalac 2's `PatDef ::= Pattern2 {',' Pattern2}` admits
   full pattern successors including `::`, scalac 3.10's `patDefOrDcl` admits
   only identifiers after the first comma, and we accept patterns minus
   unparenthesized infix. The pure name list keeps its
   `identifiers` node via `prec.dynamic(1)`. New conflicts:
   `[val_declaration, identifiers, _comma_definition_pattern]`,
   `[val_declaration, _comma_definition_pattern]`,
   `[_comma_definition_pattern, identifiers]`. Field iteration also yields the
   anonymous `,` separators, so `decl_names` filters `is_named()`.
10. SC-SOFTKW: Scala 3 keywords keep their Scala 2 name readings where the corpus needs
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

   Merge round (with the operator/lambda work above): `given` joined `enum`
   and `export` in leaving `definition_words` for a peek, so `given : T => b`
   forks three ways — `_simple_expression`, `given_definition`, and the new
   unparenthesized typed lambda — and `given { … }` adds `self_type` and
   `_type_identifier` to the same fork. Three conflict tuples keep those
   readings alive; the wider tuples subsumed the older
   `_braced_typed_lambda` variants the generator now reports unnecessary.

   Merge round (SC-MULTIPAT + SC-SOFTKW):
   - `write_root` keeps the PR52 improvement in its new `writes.rs` home:
     `operator_identifier` roots and `unquote`, so `var `a-b` = …` and
     operator-named bindings resolve writes under the unquoted name.
   - The `[identifiers, val_declaration]` conflict is dropped (generator
     reported it unnecessary once SC-SOFTKW widened the name sets).


## Fixed shapes (scalac 3.10.0 `-Ystop-after:parser` accepts each)

- SC-ENDNEW: `trait Foo` / `val foo =` / `  new Foo:` / `  end new` (empty body).
  Also with a comment-only body. Real occurrence: `tests/pos/i24250.scala`.
- SC-PARENIF: `if x.exists` / `(x => x == 10) then`. Real occurrence: `tests/pos/i12757.scala`.
- SC-INSTAR: `case class *(...)` with `val P(a, b * c) = p`. Real occurrence:
  `tests/pos/t5702-pos-infix-star.scala`.
- SC-MULTIPAT: `val a2_1, List(a2_2) = List(1, 2)` and the `var` form — an
  SLS-2 form (scalac 3.10's `patDefOrDcl` takes only identifiers after the
  comma, so this is a 2.13-dialect shape the grammar deliberately admits).
  Real occurrence: scalameta `semanticdb/integration/src/main/scala/example/Issue1872.scala`.
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
| cats parse-error files | 1 | ≤1 (residual) |
| scala3-library parse-error files | 17 | ≤14 |
| fixed (error → clean) | | 3: i12757, i24250, t5702-pos-infix-star |
| clean → error | | **0** |

The vendored counts were measured at the SC-SEMI round; later rounds only add fixes (each
round verifies `clean → error` stays 0), so they are upper bounds.

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
| slash_tail_operator_names_parse (SC-OPSLASH) | FAIL | PASS |
| unparenthesized_typed_lambda_parses (SC-TYPEDLAMBDA) | FAIL | PASS |
| wildcard_typed_lambda_parses (SC-WILDLAMBDA) | FAIL | PASS |
| keyword_and_operator_symbol_literals_parse (SC-SYMNAME) | FAIL | PASS |
| stray_semicolons_after_package_parse (SC-SEMI) | FAIL | PASS |
| wildcard_self_type_in_new_template_parses (SC-SELFWILD) | FAIL | PASS |
| spaced_quote_operator_still_errors | PASS | PASS |
| reserved_operator_symbol_literals_parse (SC-SYMNAME) | FAIL | PASS |
| leading_stray_semicolons_parse (SC-LEADSEMI) | FAIL | PASS |
| multi_pattern_val_and_var_definitions_parse (SC-MULTIPAT) | FAIL | PASS |
| keyword_named_arguments_and_parameter_names_parse (SC-SOFTKW) | FAIL | PASS |
| context_function_parameter_types_parse | FAIL | PASS |
| annotation_arguments_named_using_stay_annotation_arguments | FAIL | PASS |
| given_is_definition_before_a_following_statement | FAIL | PASS |

The FAILs are the parse-error assertion ("valid Scala must parse cleanly"), not harness
panics. The vendored run is 45/45.

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
matching the existing treatment of `extension`. Slash-tailed operators (`def /:/`, `x /:/ y` — breeze) are
handled by the SC-OPSLASH round: `/` bound to a non-`/` non-`*` opchar joins the
operator partition instead of being split off as a comment start.
`<removed_id>` placeholders in the-algorithm are sanitized-out identifiers, not
valid Scala — corpus-content ceiling.

Corpus delta (same files, same grammar build):

| repo | files with parse errors before | after |
|---|---|---|
| the-algorithm | 42 | 14 (all `<removed_id>` placeholders) |
| phantom | ~10 | 0 |
| scalding | ~8 | 6 (multi-line curried signatures — residual cluster) |
| breeze | ~12 | 4 (unexamined residuals; the `/:/` ceiling is handled by SC-OPSLASH) |

### SC-OPCMT / SC-OPRIGHT / SC-COLONMID (later round)

A comment may cut an operator's `/`-run: `x /+/* note */ y` is the infix
`x /+ y` with a `block_comment` between operator and right operand. The internal
run cannot spell that token — its `/`-step would swallow the comment's opener —
so the scanner emits the operator's class itself. The class is read from the
last accepted character, not the first (`op_stopped_class`): a `:` tail is
right-associative (`x */+:/*c*/ y`), an `=` tail is an assignment
(`x */+=/*c*/ y`), everything else keeps the first-char left class. The
right-associative and assign spellings only exist as new externals
(`_op_right_*`, `_op_assign`) because the comment-stopped token is the one form
the internal regex cannot reproduce; the productions take
`choice($._op_right_X, OP_TOKEN.XRight)` so a re-lexable operator still goes
out through the internal tokens.

`ends_in_slash` decides between the postfix and operand readings: after a
`/` the scanner reads past blanks and comments, and the token is a postfix
operator when an operand follows (`x /:/`, `p /:/ q`'s first colon-op), or the
start of a right operand when the line ends (`val l = x /` + newline). The
`/`-run also makes a `*`-led operator ineligible for `POSTFIX_STAR`, so
`f(x */+)` is a call to `f` with the postfix `*/+`, not a vararg splice.

`'op` symbols may end in `/` too (`'+/`, `'/:/`): the scanner emits `_op_symbol`
when the run's tail is `/`, and declines otherwise so `'+/*c*/` splits into the
`'+'` quote and a `block_comment`.

`_colon_mid` lexes a `:` only when same-line content follows. In
`_braced_typed_lambda` the parameter colon takes it, so `{ int: Int => b }`
stays a typed lambda (the corpus pins that reading) while `{ f:` + an indented
`cb => y` resolves to the fewer-braces colon call, matching scalac.


Round: `'/` followed by `'` is a character literal (`path.split('/')`), not an
operator symbol — the `/` check now also stops at `'`. The `:` + `/` ambiguity
resolves the comment case first: `x:/*note*/ Int => b` keeps its `_colon_mid`
because a `/` that opens `//` or `/*` does not continue the operator, while a
non-comment `/` is marked into the token so `:/=` still reads as one operator.
The `'op` run accepts the non-ASCII \p{Sm} set (`is_unicode_op_char`, mirroring
grammar.js's SM_NONASCII), so `'+/×` is handed back whole to the internal regex
and `'×/` emits as one operator symbol.

Round: `is_unicode_op_char` now also accepts the \p{So} codepoints of the
Miscellaneous Symbols + Dingbats blocks (0x2600–0x2767 and 0x2794–0x27bf —
0x2768–0x2793 are ornate parens Ps/Pe and dingbat digits No, not So),
matching the grammar.js `OP` regex which already includes `\p{So}` —
`'☃`/`'★` quote literals scan whole instead of splitting (plain `a ★ b`
infix ops were already covered by the internal lexer; the external scanner
only runs for `'op` symbol literals). And a lone `:` followed by a comment
opener decides blankness itself: a `//` run makes the rest of the line
blank, while `/*` consumes the whole comment and then the trailing-content
probe judges the line `*/` lands on — `f(1,2) : // note\n y=>g(y)` keeps
its `colon_argument` reading, `x :/*note*/ Int => b` keeps `_colon_mid`,
`(x: /*two\nlines*/ Int => x)` parses, and a lone `/` after a comment is
still code (`Map[String, /*c*/ / => Int]`).

Round: a newline *inside* the colon's trailing block comment ends the colon's
line (scalac's COLONeol is comment-transparent). The colon probe now watches
the comment body for a newline and — only when the fewer-braces colon is in
valid_symbols — calls the line blank, so `{ f(1,2) : /* a\nb */ y => g(y) }`
keeps `f` as a call instead of the `=>` tail swallowing it into a lambda
parameter. Inside braces COLON_EOL is never on the table, so the braced
ascription / typed-lambda reads are unchanged. Residual: content that sits
on the comment's own `*/`-close line still goes through the ascription fork
when no real newline follows — same as upstream.

Round: scanner-only fixes for slash runs, quotes, and unicode operands.
A `/`-tailed run that meets a non-ASCII char hands the token to the
internal lexer (`10/π` reads `10 / π`, `a /★ b` stays one op) — the
internal char classes know whether the char is an opchar or a letter.
The operand probe also skips bare line ends, so `x /\n y` stays an infix
(SLS 1.2: a line ending in an operator continues). `op_stopped_class`
takes the run length: the `<=`/`>=`/`!=` carve-out applies only to the
two-character spellings, so a comment-stopped `+!=` still classifies as
an assignment. And `'op` quotes ending at a comment opener emit the
quote before the `/` (`'x//c` = `'x` + comment) while a bare `'//`/`'/*`
is emitted whole so trailing text errors like scalac.
Residuals, kept deliberately: `/X/`-with-comment shapes (`x /:/*c*/ y`,
`x /+!=/*n*/ y`) ride the internal `mulLeft` regex — removing that arm
corrupts the `:`-external state machine enough to break
`new T { _: (A, B) => }` wildcard self-types (the `(A, B)` tuple forks
against `_colon_bindings`, a context question regexes cannot answer).
`{ _: T => body }` wildcard-typed lambdas keep the ascription read for
the same reason: a `$.wildcard` parameter collides with self_type and
colon arguments inside `new T {` templates.

Round: multi-slash op classes and definitions after a postfix `/`.
`op_stopped_class` now takes the total operator length (`total_op_len`),
counting every character the `/`-runner accepted, not just the run before
the first `/`: `x +?/!=/* note */ y` is a four-character `=`-tailed
operator, so the two-character `<=`/`>=`/`!=` comparison carve-out no
longer fires mid-op. The operand probe that follows a `/`-tail also
recognises a definition keyword (`val`, `def`, `class`, `case`, …) as
"no operand": `x /\n  val v = 1` is a postfix `/` before a new
definition, not `x / val`. The list mirrors the `definition_words`
check on the postfix path (`enum` stays out — it can be the operand
identifier — and `case` joins it since a `case` clause is never an
operand).

Round: `export` is a soft identifier after a `/`-tail and postfix op.
`export` was removed from both keyword lists (the operand probe's `decls`
and the postfix path's `definition_words`) and given the same lookahead
`enum` uses: a following identifier start, `=`, or `{` keeps the
declaration read (`x /\nexport a.b`, `x --?\nexport = 1`), while any
other continuation makes the word an operand (`10 / export`,
`x /\nexport`, `export { … }` calls with a brace block). Before this,
`val export = 2; 10 / export` reported a syntax error at `export`.

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

Round: the operand probe treats a line comment as transparent. After a
`/`-tailed or multi-char operator, `x +/ // note` + newline + `y` is still one
infix expression: the probe skips the comment body and keeps looking, so `y`
stays the right operand instead of becoming a stray statement (previously the
operator read postfix and `y` a separate statement).

Round: the operand probe stops at a blank line. `has_operand` and the
slash-tail probe count line breaks (a CRLF pair once; a blank line is two),
so `val a = x --?` + newline + `export()` keeps the call as the operand while
the same text with a blank line ends the initializer and `export()` parses as
its own statement. This matches scalac, where one newline continues an infix
expression and two end it.

Round: the `export` declaration lookahead is shared and comment/name aware.
Both probes call one `soft_keyword_starts_declaration` helper that skips
block and line comments the way the parser's export declaration does and
accepts every path spelling — ASCII and non-ASCII identifier starts,
back-quoted segments, and a standalone `=` after intervening comments
(`export /* note */ = 1` stays an assignment). Before this,
`x /` + `export /* facade */ delegate.value` misread the declaration as an
operand and errored, and `export `weird`.value` / `export Émile.value` did
the same.
