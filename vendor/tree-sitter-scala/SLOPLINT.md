# tree-sitter-scala, vendored for sloplint

## Origin

- Upstream: https://github.com/tree-sitter/tree-sitter-scala, tag `v0.26.3`, commit `851032fc97297bbe299dabe5bd826f778794d03c`.
- Published crate: `tree-sitter-scala` 0.26.3 (crates.io). `grammar.js`, `src/scanner.c`
  and `src/parser.c` of the published crate are byte-identical to the upstream tag.
- Copied verbatim from the published crate: `Cargo.toml`, `Cargo.toml.orig`, `Cargo.lock`,
  `LICENSE`, `README.md`, `bindings/`, `queries/`, `tree-sitter.json`.
- Copied from upstream: `test/corpus/*.txt` (10 files, 305 cases, all passing before and after).
- Changed: `grammar.js` (see `sloplint-grammar.patch`), and `src/` regenerated.
- Added: `test/corpus/sloplint_endnew.txt`, `sloplint_parenif.txt`, `sloplint_instar.txt`.

## Regeneration

- Tool: tree-sitter CLI 0.27.1 (pinned binary), run in this directory: `tree-sitter generate`.
  The generation is warning-free for this grammar.
- Pinned CLI on the unpatched upstream grammar does not reproduce the published `src/`
  byte-for-byte. `grammar.json` is identical. `parser.c` and `node-types.json` differ:
  the published `node-types.json` lists the hidden supertypes `_definition` and `_pattern`,
  and the 0.27.1 output expands them to concrete kinds in child-type lists. The visible node
  kinds are identical (266 = 268 − the two hidden supertypes). `src/scanner.c` is unchanged.
- Test: `tree-sitter test` in this directory: 313/313 (305 upstream + 8 sloplint cases).

## Changes to grammar.js

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

## Fixed shapes (scalac 3.10.0 `-Ystop-after:parser` accepts each)

- SC-ENDNEW: `trait Foo` / `val foo =` / `  new Foo:` / `  end new` (empty body).
  Also with a comment-only body. Real occurrence: `tests/pos/i24250.scala`.
- SC-PARENIF: `if x.exists` / `(x => x == 10) then`. Real occurrence: `tests/pos/i12757.scala`.
- SC-INSTAR: `case class *(...)` with `val P(a, b * c) = p`. Real occurrence:
  `tests/pos/t5702-pos-infix-star.scala`.

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

The four FAILs are the parse-error assertion ("valid Scala must parse cleanly"), not harness
panics. The vendored run is 10/10.
