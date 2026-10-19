# tree-sitter-java, patched for sloplint

Date: 2026-10-20.

## Origin

- Upstream repo: https://github.com/tree-sitter/tree-sitter-java
- Upstream tag: v0.23.5 (commit 94703d5a6bed02b98e438d7cad1136c01a60ba2c).
- Published crate: tree-sitter-java 0.23.5. Name and version are unchanged.
- Files copied unchanged from the published crate: Cargo.toml, Cargo.toml.orig,
  README.md, tree-sitter.json, bindings/rust/lib.rs, bindings/rust/build.rs,
  queries/highlights.scm, queries/tags.scm, src/tree_sitter/alloc.h.
  (Verified byte-identical to the v0.23.5 tag for grammar.js, src/,
  queries/, and bindings/rust/ before patching.)
- The published crate does not ship test/corpus, so it is vendored from the
  v0.23.5 tag instead (comments, declarations, expressions, literals,
  precedence, types). test/highlight is not vendored.
- Changed: grammar.js (one rule), src/ (regenerated).
- Added: test/corpus/sloplint_varargs_annotations.txt,
  sloplint-grammar.patch, this file.

## How src/ is generated

- Tool: tree-sitter-cli 0.27.1 (the pinned
  `.outline/tools/tree-sitter-cli/bin/tree-sitter`).
- Command, run in this directory: `tree-sitter generate`
- JS runtime: the default (node). Regenerating twice gave identical files.
- Regenerating the unpatched upstream grammar with 0.27.1 does not reproduce
  the published src/ byte for byte: LANGUAGE_VERSION 14 to 15, the bundled
  runtime headers (array.h gains the C++ decltype cast, parser.h gains
  TSLanguageMetadata), and node-types.json drops the `_simple_type`
  supertype entry (`_simple_type` is both a supertype and inlined, and
  0.27.1 warns and ignores the supertype). These differences exist without
  any sloplint patch; the same warning fires on the unpatched grammar.
- STATE_COUNT is 1390 (was 1385); SYMBOL_COUNT, TOKEN_COUNT unchanged.
  node-types.json is byte-identical between the unpatched 0.27.1 regen and
  the patched regen: the patch adds no node kind and no field.

## grammar.js changes

`sloplint-grammar.patch` is the unified diff against the v0.23.5
grammar.js. Apply it from this directory with
`patch -p1 < sloplint-grammar.patch` on the upstream file (verified with
`patch --dry-run`).

- J-ANNVARARGS: `spread_parameter` accepts `repeat($._annotation)` between
  `$._unannotated_type` and `'...'`, per JLS 8.4.1
  (`LastFormalParameter: {VariableModifier} UnannType {Annotation} ...`).
  The existing `repeat($._annotation)` after `'...'` is kept as is.
  `String @N ... xs`, `@N Object @N ... a`, and `String @lib.Valid ... v`
  now parse; the leading `@N` in the second shape still parses as
  `modifiers`, and the pre-ellipsis annotation lands inside
  `spread_parameter`, so `crates/sloplint-syntax/src/lang/java.rs`
  (`spread_param`, which reads the type text up to `...`) is unchanged.

## Constructs verified fixed

Each repro is accepted by `javac 25.0.4.1` and errored with the published
crate. `crates/sloplint-syntax/tests/grammar_gaps/java.rs` and
`test/corpus/sloplint_varargs_annotations.txt` guard them.

- `void m(String @N ... xs) {}`
- `void m(@N Object @N ... a) {}`
- `void m(String @Valid ... v) {}`, `void m(String @lib.Valid ... w) {}`,
  `void m(String @V2("x") ... v) {}`
- `void m(int @N ... ns) {}`, `void m(String @A @B ... xs) {}`
- `C(String @N ... xs) {}` constructors and
  `record R(String @N ... xs) {}` headers.
- Controls that already parsed and still parse with unchanged trees:
  `void plain(String ... xs) {}`, `void dims(String @N [] f) {}` (still a
  `formal_parameter` with an `array_type`), and
  `<T extends @N Object> T id(T x)` (still an `annotated_type` bound).

## Behaviour changes to know about

- Real sloplint binaries, built from the unpatched and the patched grammar,
  run with `--no-cache --format json --fail-on never --rule
  meta/parse-error`. Files that report `meta/parse-error`: guava 13 to 0,
  spring-framework 61 to 3, jdk21-src 0 to 0 (3275 + 9291 + 15619 .java
  files). Compared as sorted path sets, no file reports a parse error with
  the patched grammar that did not report one before, and 71 Java files
  stop reporting (13 guava, 58 spring).
- Annotations after `...` (`void m(String ... @B xs) {}`) were already
  accepted before this patch and still are, even though javac rejects them.
  That leniency predates this lane and is out of scope: removing it would
  newly error files that parse today.

## Known still failing

These still report `meta/parse-error` after this patch and are out of scope.

- Three Kotlin test files in spring-framework using `vararg val` record
  components (`PersonWithAlias.kt`, `PersonWithoutAlias.kt`,
  `AnnotationAsyncExecutionInterceptorKotlinTests.kt`). They parse with the
  Kotlin grammar (`tree-sitter-kotlin-ng`), not this one; a Kotlin lane
  owns them.

## Upstream corpus (`tree-sitter test`, tree-sitter-cli 0.27.1)

- Before, unpatched v0.23.5: 108 cases, 108 pass.
- After, this directory: 112 cases, 112 pass. That is the 108 upstream
  cases (no expectation changed) and 4 sloplint cases in
  test/corpus/sloplint_varargs_annotations.txt.
