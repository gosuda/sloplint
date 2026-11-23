# tree-sitter-dart, vendored for sloplint

Date: 2026-11-23. Re-vendored from 0.2.0 to 0.3.0 (see Origin).

## Origin

- Published crate: tree-sitter-dart 0.3.0 (crates.io, checksum
  `973ff905142c61200ef0bf5e226f03a5f20a9a7d3a0ae2c4c86cdb42f98b63eb`).
- Upstream repository (crate metadata): https://github.com/nielsenko/tree-sitter-dart,
  tag `v0.3.0` (annotated; dereferences to
  `4928e18ce0c6a11ebd2bfb9db3766c70b17ad4dc`, which equals the published
  crate's `.cargo_vcs_info.json` sha1). `grammar.js` and `src/scanner.c` are
  byte-identical between the crate and the tag.
- The crate ships no `test/` directory. No upstream corpus is vendored and no
  `test/corpus/sloplint_*.txt` cases were added.
- Base: every file the published crate ships, copied verbatim: `Cargo.toml`,
  `Cargo.toml.orig`, `Cargo.lock`, `LICENSE`, `README.md`, `bindings/`, `grammar.js`,
  `queries/`, `src/`, `tree-sitter.json`.
- Changed: `grammar.js` (hand-edited, 6 hunks plus 1 conflict entry; see Patch
  hunks below); generated `src/parser.c`, `src/grammar.json`,
  `src/node-types.json`. `src/scanner.c` is unchanged from the crate: upstream
  0.3.0 fixed NUL bytes in strings and block comments itself (CHANGELOG, and
  commit `3d70102` "Fix NUL, CR, whitespace and error recovery lexing").
- Added: `SLOPLINT.md`, `sloplint-grammar.patch` (`diff -u` of `grammar.js`
  against the published crate; applies to the crate tree with `git apply -p1`).

## Regeneration

- Tool: tree-sitter-cli 0.27.1, default JavaScript runtime.
- Command, run in this directory: `tree-sitter generate`. Generates clean with
  no ambiguity warnings; ABI 15.

## Patch hunks (all of `sloplint-grammar.patch`; upstream has no tracker entry)

Upstream 0.3.0 carries none of these shapes, and its repository has no open
issues or pull requests covering them, so there is no upstream id to cite. Each
hunk below names the gap test that pins it.

- `[$._type_name, $.normal_parameter_type]` conflict entry. Supports the
  `normal_parameter_type` arm; `tree-sitter generate` names exactly this
  ambiguity without it. Carried over from the 0.2.0-based patch.
- `[$._type_name, $.extension_type_representation]` conflict entry. Supports
  the representation arm below; added during PR #89 review triage.
- Removed `[redirecting_factory_constructor_signature, _built_in_identifier]`
  conflict entry. The added `const` factory arm makes it unnecessary
  (`tree-sitter generate` says so); the 0.2.0-based patch removed this same
  line.
- `normal_parameter_type` function-form arm (`seq(optional($.type), $.identifier,
  $._formal_parameter_part, optional("?"))`). Carried over; pins
  `a_function_typed_parameter_inside_a_function_type_parses`.
- Additive `~` arm. Carried over; pins `a_tilde_between_operands_parses`. The
  reference parser accepts `~` in expression position; the analyzer reports
  `not_binary_operator` semantically, so this stays parse-level only.
- `async*` / `sync*` on `=>` bodies, in both `function_body` and
  `function_expression_body` (upstream 0.3.0 split the single 0.2.0 rule in
  two). Carried over; pins `a_generator_getter_with_an_arrow_body_parses`. The
  reference parser accepts these; the error is semantic.
- `method_signature` `constant_constructor_signature` arm (with optional
  initializers), so a const constructor keeps its body through
  `method_declaration`. Narrowed carry-over of the 0.2.0 method-signature hunk
  (the factory declarator arms are upstream now); pins
  `a_const_new_declarator_with_a_body_parses`.
- `method_signature` `const` factory arm, so `const factory` declarations keep
  their bodies. Added during PR #89 review triage
  (`a_const_factory_with_a_body_parses`); the 0.2.0-based patch had the same
  arm inside `optional("const")`.
- `field_formal_parameter` takes `optional($.type)` instead of
  `optional($._final_const_var_or_type)`. Upstream 0.3.0 unified
  primary and regular parameter lists, so bare `var`/`final` (rejected by
  dart) parse again. Pins
  `a_var_initializing_primary_parameter_still_reports_a_parse_error` and
  `a_final_initializing_primary_parameter_still_reports_a_parse_error`.
- `extension_type_representation` required form takes an optional function
  part, so `extension type E(int f(String s))` parses. Added during PR #89
  review triage (`a_function_typed_extension_type_representation_parses`).

## Dropped hunks (fixed upstream in 0.3.0)

Primary-constructor headers (`primary_constructor`, `primary_parameter_list`,
`_type_header`), `;` empty bodies, abbreviated `new`/`factory` constructors,
empty-name factory redirects, `this` primary bodies, extension-type
representations with trailing comma, enum headers with trailing comma, `await`
and `yield` as identifiers, `yield:` labels, NUL bytes in strings and block
comments (scanner fix) and the `_type_name` await hunk. The `const` factory arm
and the redirecting-factory conflict entry first dropped here returned during
PR #89 review triage. The 0.3.0 CHANGELOG documents the rewrite, the nine
node renames, and the NUL fix.

## Fixed shapes (each valid per `dart format --output=none`, Dart 3.13.5, no flags)

Covered by upstream 0.3.0 directly (20 of the 26 gap-test fix shapes):

- Primary-constructor headers on class, mixin class, enum, and extension type:
  `class C(final int x) {}`, `class const C(...)`, `class C<T>.named() {}`,
  `class C.named();`, `enum E() {}`, `enum const E(int a) { x(0) }`, empty `enum E {}`.
  Parameter lists take optional and named groups, trailing commas, `int this.z`,
  `super.x`, `var f()` function-typed parameters, and declaring/initializing mixes.
  `var this.z` and `final this.z` stay errors (dart rejects them; patch hunk above).
- Bodyless declarations: `class C;`, `sealed class R;`, `extension E on int;`.
- Trailing comma in an extension type representation: `extension type E(int it,) {}`.
- Constructor declarators: `new name(...)`, `new.name(...)`, `const new name(...)`,
  `new e(this.p) : q = p;` with an initializer list.
- `this` primary-body forms: `this;`, `this : assert(...);`, `this { ... }`,
  `this : x = x {}`, in class and extension type bodies.
- Empty-name redirecting factory under a primary header: `factory() = D;`.
  `const factory C() => C();` with a body.
- `await` as a type name (`class await {}`, `await y, z;`).
- `yield:` as a label.
- NUL byte inside a string literal (fixed upstream in `src/scanner.c`).

Kept by the patch (6 shapes, invalid nowhere, rejected by pristine 0.3.0):

- Function-typed parameter inside a function type: `int Function(int f(String x))`.
- `~` as an infix operator in expression position (parse-level only, see above).
- `async*` / `sync*` on `=>` bodies (semantic error in dart, see above).
- `const new() {}` with a body.
- `const factory C() => C._();` with a body.
- Function-typed extension type representation: `extension type E(int f(String s))`.

## Validation

- Corpus: `.outline/corpora/dart-sdk`, pkg subset, 16,068 `.dart` files analyzed by
  sloplint. Baseline: published grammar. Validator: `dart format --output=none`.
  (Measured 2026-10-20 against the 0.2.0-based tree; not re-measured here: the
  corpus is absent from this checkout. Deltas verified this run are noted below.)
- Baseline parse-error files: 1,117 = 640 dart-accepted + 477 dart-rejected.
- Dart-accepted (640): 630 now parse clean; 10 still error (see below).
- Dart-rejected (477): 462 still error; 15 now parse clean (see below).
- Clean→error: 4 files, all dart-rejected (rc 65 each). These were baseline
  false-cleans: the published grammar accepted invalid code. Clean→error on
  dart-accepted files: 0.
- Re-vendor differential (this run, 36 gap-test sources plus 65 sloplint fixture
  `.dart` files: 101 files, `tree-sitter parse` ERROR/MISSING counts with
  CLI 0.27.1): before (vendored 0.2.0-based) 90 clean / 11 error; after
  (vendored 0.3.0-based) 90 clean / 11 error, the same 11 invalid-control
  sources. No clean→error and no error→clean at file level.
- `class C(var f()) {}` (residue `declaring_parameter.dart:8`) parses clean on
  pristine upstream 0.3.0: fixed upstream, residue entry removed below.
- Null-aware case heads (`case b?[c]:`) still error on the new tree: residue kept.
- Tests: `crates/sloplint-syntax/tests/grammar_gaps/dart.rs`, 38 tests, all green
  on the vendored grammar. 6 fix tests fail on pristine upstream 0.3.0 (the
  patch-kept shapes); the other 20 fix shapes pass on both. The 12 that pass on
  both before and after are 11 error-controls and one non-regression guard.

## Acceptance status

- Met: zero clean→error on dart-accepted files; 6/6 patch-kept fix tests red on
  pristine upstream 0.3.0.
- Not met as written: "the 477 intentional-error files MUST still report parse errors".
  15 of them now parse clean (listed below; measured on the 0.2.0-based tree).
  Orchestrator accepted these as residue.
- Not met as written: literal zero clean→error. The 4 dart-rejected files are
  baseline false-cleans and now error correctly.
- Not available: upstream corpus counts (the crate ships no corpus).

## Over-accepts: 15 dart-rejected files that now parse clean

Each row: the baseline's first parse error in that file (line shown), and the reason
dart rejects the file. The baseline errored on every one of these 15 files; the
published grammar did not parse them. A snippet-level check also shows the published
grammar accepts some of the constructs in isolation, but the file-level status is
what counts here.

**(a) Version-pinned (4).** Each carries a `// @dart=` pin or is a `*_pre_feature`
version-gated negative. The same syntax is valid in Dart 3.13. The grammar is not
version-gated, so these cannot be rejected without breaking valid input.

| File | Baseline error (line) | Construct the grammar now accepts |
|---|---|---|
| `parser_testcases/primary_constructors/constructor_head_pre_feature.dart` | `const new () {}` (10) | `const new` declarator |
| `parser_testcases/primary_constructors/factory_constructor_head_pre_feature.dart` | `factory () = C;` (15) | empty-name redirecting factory |
| `parser_testcases/primary_constructors/no_class_extension_type_body_pre_feature.dart` | `class C;` (2) | bodyless class |
| `testcases/general/declaring_parameter_pre_feature.dart` | `extension type ET1(var int a) {}` (6) | extension type representation |

**(b) Experimental augmentation syntax (9).** Both directories carry
`folder.options` with `--enable-experiment=augmentations`, so augmentation forms are
invalid on the stable channel. The grammar parses `augment` on declarations, as the
published grammar does for functions and members. Under `augment`, this change also
accepts primary headers and `;` bodies, which is the mine-enabled part.

| File | Baseline error (line) | Construct the grammar now accepts |
|---|---|---|
| `parser_testcases/augmentation/class_primary_constructor_body.dart` | `augment class A() {` (1) | primary header after `augment` (this change) |
| `testcases/augmentations/extends.dart` | `class A;` (5) | bodyless class, plus `augment class B extends A;` (this change) |
| `testcases/augmentations/constructor_declarations.dart` | `class Class2();` (25) | primary header in a class body (this change) |
| `testcases/augmentations/named_parameters.dart` | `augment new constructor({int y = 5, int x = 6}) : value = x < y;` (14) | `augment new` constructor with initializer list |
| `testcases/augmentations/optional_parameters.dart` | `augment new constructor([int x = 6, int y = 5]) : value = x < y;` (14) | same |
| `testcases/augmentations/required_named_parameters.dart` | `augment new constructor({required int y, required int x}) : value = x < y;` (14) | same |
| `testcases/augmentations/unordered_constructor.dart` | `new withInitializers() : field = 0;` (18) | `new` declarator with initializer list |
| `testcases/augmentations/unordered_constructor_part1.dart` | `augment new withInitializers() : field = 0;` (29) | `augment new` declarator |
| `testcases/augmentations/unordered_constructor_part2.dart` | `augment new withInitializers() : field = 0;` (46) | `augment new` declarator |

**(c) `new` used as a constructor or factory name (2).** The identifier rule accepts
`new`, and tree-sitter regex has no negative lookahead to exclude it where no keyword is
valid. The reference rejects both names.

| File | Baseline error (line) | Reference message |
|---|---|---|
| `parser_testcases/primary_constructors/factory_new.dart` | `const factory new() => 0;` (5) | "Factory constructors can't be named 'new'" |
| `parser_testcases/primary_constructors/new_new.dart` | `const new new();` (3) | "Constructors declared with the 'new' keyword can't be named 'new'" |

## Residue: dart-accepted files that still error (9)

- `issue_40267_case_01.dart`, `_case_02.dart`, `_case_02_2.dart`, `_case_03.dart`,
  `_case_05.dart`, `_case_05_2.dart` (6): null-aware case heads (`case b?[c] : ...`,
  `case x?[4]: ...`). Still error on the 0.3.0-based tree (verified this run).
  A rule for these heads also accepted reference-rejected siblings
  with the same head (`issue_40267_case_01_2`, `_03_2`, `_04`, `_04_2`), so the rule was
  removed. The reference picks the conditional-expression reading from context, which
  this grammar cannot express.
- `erroneous_constructors.dart:14`, `external_field_errors.dart:5`: the reference parses
  these and reports the error semantically (wrong phase).
- `issue51626.dart:9`, `case 1 + 1:` under `// @dart = 2.19`: version pin.

## Not implemented

- Version gating (cannot be expressed in the grammar).
- `factory` used as a return type (`factory fact();` parses as a method returning
  `factory`).
- Exclusion of `new` as a constructor or factory name (see (c)).
- Narrowing `augment` forms so that primary headers and `;` bodies are rejected after
  `augment`. The 2 augment rows that this change enables (class_primary_constructor_body,
  extends) are the mine-enabled part of group (b).
- `augment enum E() {}` is accepted by the enum rule; not probed against dart and not
  present in the corpus.
