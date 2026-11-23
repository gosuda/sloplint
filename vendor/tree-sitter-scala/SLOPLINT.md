# tree-sitter-scala, vendored for sloplint

## Origin

- Upstream: https://github.com/tree-sitter/tree-sitter-scala, HEAD `master`
  at merge `01876d1` ("Merge pull request #759", 2026-11-23). The manifest
  (`vendor/grammars.toml`) pins this commit; no published crate matches it
  (Cargo metadata at the pin says `0.26.14`, unreleased), so the crate fields
  stay empty and the Cargo metadata is taken verbatim from upstream.
- Previous base was tag `v0.26.3` plus a 1985-line self-written patch.
  The rebase absorbs everything upstream fixed since: releases v0.26.4
  through v0.26.13 and the unreleased fix PRs #752 (extension clauses on
  the next line), #753 (polymorphic function bodies), #755 (infix star
  inside parentheses), #756 (empty colon body before an end marker),
  #758 (argument clause on a deeper line), #759 (end marker after
  semicolon lines). Node kinds are unchanged from v0.26.3 to HEAD, so the
  Scala adapter needs no changes.
- Copied verbatim from upstream: `Cargo.toml`, `Cargo.toml.orig`,
  `Cargo.lock`, `LICENSE`, `README.md`, `bindings/`, `queries/`,
  `tree-sitter.json`, and `test/corpus/*.txt` (10 files).
- Changed: `grammar.js` and `src/scanner.c` (see `sloplint-grammar.patch`,
  1978 lines, 54 hunks), and `src/` regenerated.
- Added: `test/corpus/sloplint_endnew.txt`, `sloplint_parenif.txt`,
  `sloplint_instar.txt`, `sloplint_oplam.txt`, `sloplint_multipat.txt`,
  `sloplint_softkw.txt`, `sloplint_enum.txt`, `sloplint_opcmt.txt`.
- Removed: `scala.so` (a stale build artifact with no references).

## Regeneration

- Tool: tree-sitter CLI 0.27.1 (pinned binary), run in this directory:
  `tree-sitter generate`. Give the run a private `TREE_SITTER_LIBDIR`
  and an isolated `XDG_CACHE_HOME`.
- The generation reports the same warnings as pristine upstream HEAD:
  two `_definition`/`_pattern` supertype-inlined notes and one
  pre-existing unnecessary-conflict note
  (`_simple_expression`, `_single_lambda_param`).
- Test: `tree-sitter test` in this directory: 367/367 (upstream cases
  plus the sloplint files below).

## Changes to grammar.js

Kept hunks (each names the shape it fixes; none of these parses cleanly
on pristine HEAD):

- SC-MULTIPAT: `val_definition`/`var_definition` take a comma-separated
  pattern list (`val a2_1, List(a2_2) = …`; SLS PatDef). Successors after
  the first comma exclude unparenthesized infix patterns, so
  `val a, b | c` stays an error. New `_comma_definition_pattern` rule
  plus three declaration conflicts. The pure name list keeps its
  `identifiers` node; `decl_names` in the adapter filters separators.
- SC-SOFTKW_ARG: `keyword_named_argument` (`using = v`, `given = v`)
  aliased to `assignment_expression` in `_argument_list` and
  `_exprs_in_parens` (jackson-module-scala `@JsonDeserialize(using = …)`,
  Scala 2 named-argument idiom). Scoped to argument lists only.
- SC-OPSLASH: `/`-led operator names ending in `/` (`def /:/`, breeze)
  admitted to the operator partition without opening comments.
- SC-SOFTENUM: `enum` stays usable as a Scala 2 operand and call callee
  after a line-broken operator (`1 +\nenum`, `enum(2)`); the definition
  reading itself is upstream's (`enum_head` precedence plus `_name_gate`).
- SC-TYPEDLAMBDA: an unparenthesized typed parameter in expression
  position (`case c => a: Array[Long] => new W(a)`), kept below the
  colon-argument and ascription readings by dynamic precedence. The
  parameter takes plain names only, so soft keywords keep their
  upstream identifier readings.
- SC-SOFTKW_CTXF: `(using T, …) ?=> R` context function types via a
  dedicated `_context_parameter_types` rule outside `parameter_types`.
- SC-SEMI: a top-level separator may be a run of literal `;`
  (`package breeze.plot;;;`), including a leading run.
- SC-SYMNAME: quote names for operators (`'*`, scalding) and keywords
  (`'row`, `'col`, `'val`), with the immediate-regex bound and the
  `_op_symbol` external for `/`-final spellings.
- SC-OPCMT / SC-OPRIGHT: operators cut short by a comment keep their
  class (`_op_right_*`, `_op_assign` externals); slash-tail operand and
  postfix probes with comment transparency and blank-line termination;
  `export`/`given` soft-keyword operand lookaheads.
- SC-WILDLAMBDA: `{ _: Any => () }` (kept; upstream covers it too, but
  the hunk is load-bearing for the exercised shapes).
- SC-SELFWILD: `new T { _: (A, B) => … }` reads as a wildcard self-type
  (`self_type` dynamic 3 beats the braced-lambda reading).
- Braced-lambda colon: the `_braced_typed_lambda` parameter takes the
  `_colon_mid` external (a colon with same-line content after it)
  instead of `colonEol`, so an end-of-line colon after a braced
  parameter keeps the fewer-braces colon-argument reading and both
  spellings produce the flat upstream tree.
- SC-PARENIF_NEXT: when `then` sits on the line after the condition
  arguments (`if x.exists` + `(p)` + `then`), the condition keeps the
  `split_call_condition` reading (function plus arguments across the
  line break) instead of failing. Upstream e2f0983 covers only the
  same-line `then`; covered by `sloplint_parenif.txt`.

Dropped hunks (upstream now covers them, each verified clean on HEAD):

- SC-INSTAR (`_asterisk` in `infix_pattern`; upstream 2fd3302, same fix
  plus a precedence repair).
- SC-PARENIF (`split_call_condition` for same-line `then`; upstream
  e2f0983 continues the condition in the scanner instead; the
  next-line-`then` shape keeps the narrow SC-PARENIF_NEXT rule above).
- SC-ENDNEW (dedicated `instance_expression` alternative; upstream
  dc2b699 closes empty colon bodies, see the guard hunk below).
- SC-SOFTKW_PARAM (`given` as a parameter name; upstream 51aae0f).
- The `_soft_identifier` widening to `enum` (upstream 6779fac reads
  `enum` as an identifier; operand positions keep the scanner probes).
- The moved `[block, capture_set]` duplicate and given-definition
  conflicts upstream subsumed.

Conflict declarations: the fork set the wider identifier reads need
(`using`/`given` as names, `enum` as a name, polymorphic `[T] =>`
bodies, unparenthesized typed lambdas, self-types, ascriptions). Each
names the two readings and the token that decides; five proposed forks
proved unnecessary under `tree-sitter generate` and were pruned, and
the upstream pin already carries the enum-narrowed braced-lambda forks.

Guard hunk (the one intentional difference from upstream HEAD):

- Upstream dc2b699 lets an end-of-line colon alone stand as an empty
  template body, which also accepts a marker-less empty `new:` template
  (the invalid control `val x = new Foo:` with no `end new` parses
  clean on HEAD; scalac rejects it). The grammar's empty-body
  alternative takes a new external token instead of `_colon_eol`, and
  the scanner emits it only when the next significant line starts with
  an `end` marker — every other flow is byte-identical to upstream.

## Changes to src/scanner.c

- The operand, postfix, slash-tail, quote-tail, and soft-keyword probes
  described above, with comment transparency and blank-line termination.
- The empty-body guard: `empty_body_end_follows` (blank rest-of-line,
  then the next significant line must start with `end` plus a name)
  and the `after_colon_eol` bookkeeping for the new token.
- The mid-line colon probe: `COLON_MID` is emitted only when the rest
  of the line holds same-line content, so the braced typed lambda's
  parameter colon cannot be the fewer-braces colon argument's
  end-of-line colon.
- Upstream scanner work (perf lexing, trivial-end semicolon, continuing
  paren lines, NAME_GATE, word-operator EOL, case-body indent) is taken
  verbatim.

## Intentional divergence (documented, not a regression)

None: the rebased tree reproduces every upstream corpus shape exactly,
including the varied-indentation braced lambda (both statements inside
the lambda, as upstream pins it). The `_colon_mid` token keeps it that
way: with the patch's extra typed-lambda forks, the braced parameter's
end-of-line colon would otherwise take the narrower fewer-braces
reading, so the braced reading takes a mid-line colon only. The
invalid-control guard only narrows the empty-body alternative.

## Fixed shapes (scalac 3.10.0 `-Ystop-after:parser` accepts each)

- All six upstream fix shapes above (their corpus cases pin them).
- SC-MULTIPAT val and var lists (scalameta Issue1872 fixture).
- SC-SOFTKW_ARG (`@JsonDeserialize(using = classOf[MyDeserializer])`)
  and SOFTKW_PARAM (`given: Int`, filodb).
- SC-SOFTENUM operand and call forms, SC-OPSLASH (`def /:/`),
  SC-TYPEDLAMBDA (scalding case body), SC-SOFTKW_CTXF
  (`(using T) ?=> R`), SC-SEMI (breeze `;;;`), SC-SYMNAME (scalding
  symbol literals), SC-OPCMT slash runs, SC-WILDLAMBDA, SC-SELFWILD.
- SC-POLYARG (`[X] => (_: List[X]).headOption`, cats
  FunctionKLiftSuite; `[t] => _ match`, tests/pos/i23774typeclass):
  fixed upstream by #753, now clean.

## Still failing (residual)

- Experimental-flag and spec-string constructs (modularity,
  subCases, singleton-ctx-bound, spec-strings): upstream will not fix
  these; out of scope.
- End-marker alignment is not checked (pre-existing; upstream also
  unchecked): a misaligned `end` marker still parses.
- Out of scope: `tests/pos/i19642/i19642.java` (Java).
- Invalid shapes checked to stay errors: the marker-less empty `new:`
  template above; dangling `[X] =>`; spaced `' +`; bare infix in
  pattern lists; `(using T) => R` with a plain arrow.

## Verification

- Parse battery over the 28 sloplint corpus inputs: 27 clean, only the
  invalid control errors (byte-identical accept set to the vendored
  0.26.3 tree plus the six upstream fixes and the two polyarg probes).
- `tree-sitter test`: 366/366 (upstream 340 plus sloplint files).
- `cargo test -p sloplint-syntax` plus the workspace scala suites and
  `cargo clippy --workspace --all-targets --locked -- -D warnings`:
  see the lane packet gate log.
- `scripts/grammars.sh check` passes; `verify tree-sitter-scala`
  reproduces this tree from the pinned commit plus the patch.
