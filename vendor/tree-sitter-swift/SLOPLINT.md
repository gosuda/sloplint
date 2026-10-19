# tree-sitter-swift, vendored for sloplint

Date: 2026-10-20.

## Origin

- Upstream repository: https://github.com/alex-pinkus/tree-sitter-swift
- Upstream tag: 0.7.4 (`171fa3bc343233fe07f5e17205aaada33d778825`, matches the
  crate `.cargo_vcs_info.json` sha1).
- Published crate: tree-sitter-swift 0.7.4 (crates.io). Name and version are unchanged.
- This directory is the published crate plus the patch below. `Cargo.toml`,
  `Cargo.toml.orig`, `LICENSE`, `README.md`, `queries/`, `bindings/rust/` and `src/`
  are the published files. `Cargo.lock` and `node_modules/` from the crate are not
  vendored, following the other vendored grammars.
- Added from the upstream tag (the crate does not ship them): `grammar.js`,
  `tree-sitter.json`, `test/corpus/` (upstream cases).
- `sloplint-grammar.patch` is the diff of `grammar.js` and `src/scanner.c` against
  the upstream tag. It applies with `git apply` in a clone of tag 0.7.4.
- Upstream `main` has since gained ~15 lines of grammar fixes; they were read but
  deliberately not taken, to keep this patch reviewable and pinned to 0.7.4.

## Why

The published grammar reports a parse error on valid Swift that ships in
swift-nio and swift-collections (Swift 6 patterns: ownership modifiers, `unsafe`
expressions, `sending`, lifetime attributes, `#if` between attributes). sloplint
skips a whole file on any error node. These patches remove the errors reproduced
on the two corpora (1290 Swift files).

Every fix below was validated with `swiftc 6.4.1 -parse`: a construct was only
accepted when swiftc parses it too. Invalid controls (`@_lifetime(self: consume
self)`, `@_lifetime(consume self)`, trailing comma in `where`, `;`-terminated
labeled closures) still error; see "still failing".

## Regenerating src/

- Tool: tree-sitter-cli 0.27.1, `--js-runtime native` equivalent (default Node runtime
  was also used during development; both give identical output for this grammar).
- Command, run in this directory: `tree-sitter generate`
- It rewrites `src/parser.c`, `src/grammar.json`, `src/node-types.json` and
  `src/tree_sitter/*.h`. `src/scanner.c` is edited by hand and is not generated.
- The crate's `src/scanner.c` is byte-identical to the tag's; only the `;` fix below
  touches it.
- `test/corpus/sloplint_*.txt` holds 34 new guard cases (upstream corpus format).
  They were produced by `tree-sitter test --update` and read by hand: none contains
  an ERROR or MISSING node, and every source passes `swiftc -parse`.
- A note on the toolchain: regenerating the *unpatched* tag grammar with CLI 0.27.1
  already moves two shapes relative to the published `parser.c` (which the publisher
  built from a dirty tree with an older CLI): `if unsafe a == b` starts parsing
  (previously an error) while `unsafe s.p = q` stops parsing (previously clean).
  Both are covered by cases below; the operand rules make the behavior deliberate
  rather than accidental in either direction.

## Changes to grammar.js

- SW-VOID: `tuple_expression` gains an explicit empty `seq("(", ")")` choice, so
  `()` parses as a value (`let x = ()`, `return ()`, `g(())`, `case .a: ()`).
  An `optional()` wrapper instead of the explicit choice was tried and reverted:
  it perturbs LR states enough that `(~x)` misparses. A `token("()")` was also
  tried and reverted: the lexer token hijacks type-position parens (`() -> Void`).
- SW-LIFETIME: `_attribute_argument` gains three narrow branches for compiler
  lifetime attributes: `borrow <ident>` (pinned to the word `borrow`, which has no
  operator or contextual-keyword role, so no new conflicts), `self: <expr>`, and
  `<ident>: borrow <ident>` (`dependent: borrow source`). `copy X` keeps parsing
  through the `consume`-operator expression branch. Pinning `copy` as well was
  tried and reverted: it is genuinely ambiguous with the consume operator.
- SW-OWN: `_parameter_ownership_modifier` gains `__consuming`, `__owned`,
  `__shared` (legacy spellings, still used across both corpora). They also widen
  `_contextual_simple_identifier`, so they stay usable as identifiers.
- SW-RUNTIME: `_compilation_condition` gains `_runtime(<ident>)` and
  `objectFormat(<ident>)` branches, mirroring the existing `os(...)`/`arch(...)`
  style, covering `#elseif (compiler(<6.1) && !os(WASI)) || (compiler(>=6.1) &&
  _runtime(_multithreaded))`.
- SW-UNSAFE: `unsafe_expression`, `await_expression` and `consume_expression`
  operand choices gain `prec.left(0, $._binary_expression)`, mirroring the
  long-standing `try_expression` shape (whose comment describes exactly this
  `if foo { ... }` brace ambiguity). This fixes `if unsafe a == b`,
  `while unsafe ...`, `if await ...`, `if consume ...`. Declared conflicts
  `[X, call_suffix]`, `[X]`, `[X, lambda_literal]` and direct condition/switch
  branches were all tried for the remaining bare-identifier shapes and reverted:
  the generator flags them unnecessary (the ambiguity resolves statically) and
  the extra derivations won merges wrongly. Bare-identifier/prefix/navigation
  operands before `{` (`if unsafe x {`, `if let y = unsafe z.w {`,
  `switch consume v {`) still fail; see below.
- SW-ASSOCWHERE: `_modifierless_typealias_declaration` gains
  `optional($.type_constraints)` (`typealias R<B> = Range<B> where ...`);
  `associatedtype` bounds become `sep1(..., ",")`
  (`associatedtype Index: Equatable, Comparable, Hashable`); `type_constraints`
  uses `sep1` instead of `sep1Opt` because swiftc rejects a trailing comma, and
  the trailing form let a newline after the comma end the list early
  (`A.C == B,\n A.D == B` in protocols).
- SW-ATTRCLASS: `modifiers` is a pinned-first `seq` (a lone directive still parses
  as a directive statement) whose tail admits `$.directive`, so attributes may
  wrap `#if` blocks (`@a #if X @b func`, `#if !DEBUG @exclusivity(unchecked)
  #endif ... var x`). Five conflict sets cover the new `{ @attr` competitions;
  the generator demanded each one. `nonisolated(unsafe)` is accepted in local
  scope too, via a named `_nonisolated_modifier` rule aliased to
  `member_modifier` (an inline `alias(seq(...))` distributes over its tokens and
  yields four sibling nodes; aliasing the named rule keeps one node).
- Accessors: `borrow_specifier`, `mutate_specifier`, `unsafe_address_specifier`
  and `unsafe_mutable_address_specifier` fold into the existing
  `computed_getter`/`computed_setter` rules (no new conflict entries needed) and
  into `protocol_property_requirements` (`subscript(i:) -> Element { borrow
  mutate }`). New specifiers are ordinary keywords; `mutate` stays usable as an
  identifier outside accessor position (state-driven lexing) and the sweep shows
  no clean-file change from it.
- Case ranges: `switch_pattern` gains `_pattern_range` (tuple or `.member`
  pattern, range operator, expression), aliased to `$.pattern` so the adapter is
  unaffected (`case (a + 1)...b:`, `case .min..<0:`).
- Sending: `function_type` results accept `parameter_modifiers`
  (`() async throws -> sending R`), mirroring function declarations, which
  already allowed them.

## Changes to src/scanner.c

- Explicit `;`: when `;` follows whitespace, the branch never assigned the result
  symbol and never marked the token end, emitting whatever was on the stack, so
  `{ 4 ; 5 }` and `let x = 4 ; let y = 5` failed while `{4; 5}` passed. The branch
  now sets `*symbol_result = EXPLICIT_SEMI` and marks the end. No other scanner
  change.

## Verified fixed (each fails on the published crate, passes here)

- Void values, all lifetime argument forms in the corpora (24 distinct, enumerated
  by sweeping `@_lifetime(`), `__consuming`/`__owned`/`__shared`, compound
  `_runtime`/`objectFormat` conditions, binary-operand unsafe/await/consume in
  `if`/`while`/`guard`, unsafe calls and trailing closures in return position,
  borrow/mutate/unsafeAddress/unsafeMutableAddress accessors, typealias `where`,
  multi-bound associatedtypes, multiline dotted `where` constraints, attributes
  around `#if`, local `nonisolated(unsafe)`, case ranges, `sending` results,
  spaced semicolons.

## Constructs known still failing

Valid Swift, documented for follow-up (each confirmed with `swiftc -parse`):

- Bare-identifier/prefix/navigation `unsafe`/`consume` operands directly before
  `{`: `if unsafe x {`, `if unsafe !x {`, `if unsafe o.f {`, `if let y = unsafe
  z.w {`, `switch consume v {`. The trailing closure wins statically and no
  declared conflict engages the fork. `unsafe x = y` assignment operands regress
  under CLI 0.27.1 regeneration itself (the published `parser.c` accepts them);
  the operand-level `assignment` alternative was tried and did not restore them.
- Labeled closures across a newline: `g { ... }\nlabel: { ... }` ends the call at
  the implicit semicolon. A `call_suffix` continuation was tried and regressed
  hundreds of files (it competed for every statement-end semicolon); reverted.
- `(@convention(c) (A) -> B).self`: extending `_parenthesized_type` with function
  types flips `~` resolution in expression parens (`(~x)` starts parsing as a
  type), regressing two clean files; reverted.
- `class func` keeps parsing but resolves `class` to the anonymous slot instead
  of `property_modifier` on two files (`ServerSocket.swift`,
  `TestBigString.swift`), so the adapter's `global_scope` reads false there.
  Static and dynamic precedence both failed to flip it; the files parse cleanly
  and no error count changes.

Deliberately still errors (swiftc rejects them too):

- Trailing comma in `where` clauses; `@_lifetime(self: consume self)`;
  `@_lifetime(consume self)`; `;`-terminated labeled closures.
- BPFTraceParser/DTraceParser/HeaptrackParser regex literals (`/^]: …/`): swiftc
  hard-errors as well; out of scope per brief.

Known leniency (parser accepts, swiftc `-parse` rejects; pre-existing except
where noted):

- `g { .. } { .. }` (two unlabeled trailing closures): parses as nested calls;
  swiftc rejects at parse level (`consecutive statements ... must be separated
  by ';'`). Pre-existing upstream behavior, untouched.
- `@_lifetime(self: consume self)` / `@_lifetime(consume self)`: accepted through
  the generic labeled/expression attribute branches. Distinguishing parse- from
  type-level rejection matters here: tighten only with lifetime-specific value
  restrictions, out of scope.

## Adapter stability

Node kinds and fields read by `crates/sloplint-syntax/src/lang/swift.rs` are
unchanged, except: (1) new specifier nodes (`borrow_specifier` etc.) appear only
where the old grammar errored, folded under the existing `computed_*` nodes;
(2) `_pattern_range` is aliased to `pattern`; (3) `nonisolated` keeps the
`member_modifier` kind via alias; (4) repeated `must_inherit` fields on
multi-bound associatedtypes (the adapter reads the first); (5) `class func`
loses the `property_modifier` node on two files (see above). The `unsafe:`
`expr:` field now renders on `unsafe_expression` (matching the tag grammar
source; the published `parser.c` was built from a dirty tree without it). The
adapter never reads that field (`unwrap_statement` returns unsafe nodes as-is).

## Corpus results with tree-sitter-cli 0.27.1 `tree-sitter test`

- Upstream corpus at tag 0.7.4: the tag's own expectations pass 265 of 296 with
  both the published `parser.c` and a pristine 0.27.1 regeneration (identical
  failure lists: 31 stale expectations, pre-existing, not a CLI effect).
- After (patched): 296 of 296 upstream cases pass, plus 34 of 34 new
  `test/corpus/sloplint_*.txt` cases (void 4, lifetime 5, ownership 3, runtime 3,
  unsafe 8, assocwhere 3, modifiers 3, ranges 2, sending 3).
- Total with the patch: 330 of 330.

## Sweep result

Raw-tree verdicts (`root.has_error()`, which also sees hidden recovery MISSING
nodes invisible to cursor walks; cursor output is diagnostics only). Tree
comparison between the published parser and this one on 1290 Swift files
(`swift-nio` 556 + `swift-collections` 734):

- Files with an error node: 323 before, 53 after.
- Files that changed from error to clean: 270. Files that changed from clean to
  error: 0.
- Findings with `sloplint --no-cache` were not re-run here; the orchestrator gates
  them. The only known tree-shape changes on dual-clean files: ownership and
  addressor accessors now parse as accessors instead of misparsed calls with
  trailing closures (6 files); `unsafe:` gains its `expr:` field (5 files,
  adapter-neutral); `class func` loses `property_modifier` (2 files, see above).
