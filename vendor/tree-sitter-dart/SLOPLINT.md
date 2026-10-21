# tree-sitter-dart, vendored for sloplint

Date: 2026-10-20.

## Origin

- Published crate: tree-sitter-dart 0.2.0 (crates.io, checksum
  `325dd1e24ee9ee21111e9c43680ae7d6010aaa9f282b048a99b9c7163c1cf553`).
- Upstream repository (crate metadata): https://github.com/nielsenko/tree-sitter-dart.
  Upstream tag not verified; no git clone was made.
- The crate ships no `test/` directory. No upstream corpus is vendored and no
  `test/corpus/sloplint_*.txt` cases were added.
- Base: every file the published crate ships, copied verbatim: `Cargo.toml`,
  `Cargo.toml.orig`, `Cargo.lock`, `LICENSE`, `README.md`, `bindings/`, `grammar.js`,
  `queries/`, `src/`, `tree-sitter.json`.

## Changed and added files

- Changed: `grammar.js` and `src/scanner.c` (hand-edited); generated `src/parser.c`,
  `src/grammar.json`, `src/node-types.json`, `src/tree_sitter/*.h`.
- Added: `SLOPLINT.md`, `sloplint-grammar.patch` (`diff -u` of `grammar.js` and
  `src/scanner.c` against the published crate).
- `grammar.js` carries no explanatory comments. The added comment lines were removed
  after the grammar was final; the regenerated `parser.c`, `grammar.json`, and
  `node-types.json` are byte-identical before and after that removal.

## Regeneration

- Tool: tree-sitter-cli 0.27.1 at `.outline/tools/tree-sitter-cli/bin/tree-sitter`,
  default JavaScript runtime.
- Command, run in this directory: `tree-sitter generate`.

## Fixed shapes (each valid per `dart format --output=none`, Dart 3.13.5, no flags)

- Primary-constructor headers on class, mixin class, enum, and extension type:
  `class C(final int x) {}`, `class const C(...)`, `class C<T>.named() {}`,
  `class C.named();`, `enum E() {}`, `enum const E(int a) { x(0) }`, empty `enum E {}`.
  Parameter lists take optional and named groups, trailing commas, `int this.z`,
  `super.x`, and declaring/initializing mixes. `var this.z` and `final this.z` stay
  errors (dart rejects them).
- Bodyless declarations: `class C;`, `sealed class R;`, `extension E on int;`.
- Trailing comma in an extension type representation: `extension type E(int it,) {}`.
- Constructor declarators: `new name(...)`, `new.name(...)`, `const new name(...)`,
  `const new() {}` with a body, `new e(this.p) : q = p;` with an initializer list.
- `this` primary-body forms: `this;`, `this : assert(...);`, `this { ... }`,
  `this : x = x {}`, in class and extension type bodies.
- Empty-name redirecting factory under a primary header: `factory() = D;`.
  `const factory C() => C();` with a body.
- `await` as a type name (`class await {}`, `await y, z;`).
- Function-typed parameter inside a function type: `int Function(int f(String x))`.
- `~` as an infix operator in expression position. The reference parser accepts it;
  the analyzer reports `not_binary_operator` semantically, so this is parse-level only.
- `yield:` as a label.
- `async*` / `sync*` on `=>` bodies. The reference parser accepts them; the error is
  semantic.
- NUL byte inside a string literal (`src/scanner.c`: NUL is consumed when not at EOF).

## Validation

- Corpus: `.outline/corpora/dart-sdk`, pkg subset, 16,068 `.dart` files analyzed by
  sloplint. Baseline: published grammar. Validator: `dart format --output=none`.
- Baseline parse-error files: 1,117 = 640 dart-accepted + 477 dart-rejected.
- Dart-accepted (640): 630 now parse clean; 10 still error (see below).
- Dart-rejected (477): 462 still error; 15 now parse clean (see below).
- Clean→error: 4 files, all dart-rejected (rc 65 each). These were baseline
  false-cleans: the published grammar accepted invalid code. Clean→error on
  dart-accepted files: 0.
- Tests: `crates/sloplint-syntax/tests/grammar_gaps/dart.rs`, 36 tests, all green on the
  vendored grammar. 24 fix tests fail on the published crate. The 12 that pass on both
  are 11 error-controls and one non-regression guard.

## Acceptance status

- Met: zero clean→error on dart-accepted files; 24/24 fix tests red on the published
  crate.
- Not met as written: "the 477 intentional-error files MUST still report parse errors".
  15 of them now parse clean (listed below). Orchestrator accepted these as residue.
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

## Residue: dart-accepted files that still error (10)

- `issue_40267_case_01.dart`, `_case_02.dart`, `_case_02_2.dart`, `_case_03.dart`,
  `_case_05.dart`, `_case_05_2.dart` (6): null-aware case heads (`case b?[c] : ...`,
  `case x?[4]: ...`). A rule for these heads also accepted reference-rejected siblings
  with the same head (`issue_40267_case_01_2`, `_03_2`, `_04`, `_04_2`), so the rule was
  removed. The reference picks the conditional-expression reading from context, which
  this grammar cannot express.
- `declaring_parameter.dart:8`, `class C(var f()) {}`: function-typed declaring primary
  parameter with `var`. Not implemented.
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
