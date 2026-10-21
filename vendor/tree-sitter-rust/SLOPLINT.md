SLOPLINT NOTES FOR VENDORED tree-sitter-rust
=============================================

Upstream repo: https://github.com/tree-sitter/tree-sitter-rust
Upstream tag: v0.24.2 (commit 77a37477a9bb61a5aec68b2dec39ba4c933bcfcb).
Published crate: tree-sitter-rust 0.24.2 (name and version unchanged).
Regenerated: 2026-10-20 with tree-sitter-cli 0.27.1.

Command used (run in a clean v0.24.2 checkout after editing grammar.js):

  tree-sitter generate

Regenerated files: src/parser.c, src/grammar.json, src/node-types.json,
src/tree_sitter/array.h. The array.h change is the CLI 0.27.1 bundled
runtime header (C++ decltype cast fix); scanner.c is byte-identical to
upstream. The full grammar.js diff against the v0.24.2 tag is saved in
sloplint-grammar.patch. Guard cases live in
test/corpus/sloplint_cases.txt (22 cases, upstream corpus format).

Why vendored: several valid Rust constructs fail to parse with 0.24.2
and make sloplint skip whole files with meta/parse-error. Each fix below
was validated against rustc (editions 2015, 2021, 2024): a construct was
only accepted when rustc parses it too.

Grammar.js changes, one bullet per construct fixed:

- Row 1: `associated_type` takes an optional `visibility_modifier`, so
  `pub type Foo;` parses inside `extern` blocks. The node kind stays
  `associated_type` with the same fields.
- Row 2: `macro_invocation` and `generic_function` heads accept a
  primitive name (`str![""]`, `u16!(..)`, `f32::<T>`) aliased to
  `identifier`. `generic_type_with_turbofish` accepts a primitive head
  (`u8::<T>::new()`).
- Row 3: `macro_invocation` head accepts `try` (`try!(foo())`), aliased
  to `identifier`.
- Row 4: `where_predicate` left side accepts `unit_type`
  (`where (): Trait<T>`), `function_type`, and `macro_invocation`.
- Row 5: `~` added to TOKEN_TREE_NON_SPECIAL_PUNCTUATION (`foo![~]`).
- Row 6: a lone `$` (lower precedence than repetition `$()`) is allowed
  in token trees and token tree patterns (`[$]`, `$ crate::Foo`).
- Row 7: attributes allowed before non-first tuple expression elements,
  struct pattern fields, and closure parameters.
- Row 8: `struct_item` accepts a where clause before a bare `;`
  (`struct S<T> where T: Send;`).
- Row 9: function parameters accept repeated attributes.
- Row 10: `type_arguments` accepts `negative_literal` (`R<-5, 5>`).
- Row 11: `\u{...}` escapes accept underscore separators (`\u{0__}`).
- Row 13: `const_item` and `static_item` names accept
  `_reserved_identifier` (`pub const default: u8`). The `const default`
  ambiguity with function modifiers is a declared conflict; the wrong
  GLR branch always dies on the next token.
- Row 14: `generic_function` head accepts `_reserved_identifier`
  (`union::<H>()`, `raw::<H>()`).
- Row 15: `_type` accepts a bare `_reserved_identifier` aliased to
  `type_identifier` (`fn f(x: raw)`); `captured_pattern`,
  `generic_pattern`, and `tuple_struct_pattern` accept reserved heads
  (`raw @ _`, `let raw::<u8> = 1;`).
- Row 16: the `<=` operator is a higher-precedence token so it wins over
  the type-argument `<` after a scoped type in an `as` cast
  (`x as libc::c_int <= 3`). `<<` still lexes as two `<` (rustc rejects
  `x as T << 3` too, so the grammar agrees).
- Row 17: `dynamic_type` accepts a leading lifetime
  (`Box<dyn 'a + Send>`). `impl 'a + Send` already parsed via
  `bounded_type` and is unchanged.
- Row 18: `range_expression` accepts a prefix `..=` (`..=5`,
  `|end| ..=end`). Without it `..=5` silently misparsed as an
  assignment of `5` to the range `..`, and `..=end` failed outright.
- Row 19: a where predicate may end in a bare colon with empty bounds
  (`where C: {}`). This is a declared conflict; the empty branch only
  applies before `,` or a block and never before a bound.
- Row 20: `function_modifiers` accepts `safe`; `static_item` accepts
  `safe` and `unsafe` (`safe fn foo();` in `unsafe extern` blocks).
  `safe` was added to `_reserved_identifier` so existing uses of `safe`
  as a variable, field, parameter, or type name keep parsing.
- Row 21: `struct_pattern` accepts a turbofish type
  (`let R::<u8> { s, e } = r;`).

Constructs verified fixed: all of the above, each with a corpus case in
test/corpus/sloplint_cases.txt.

Constructs known still failing (deliberately, rustc rejects them too):

- Attributes in tuple patterns: `let (a, #[cfg(x)] b) = t;`.
- Bare `<` after `as` on a scoped type: `x as libc::c_int < 3`.
- `<<` after `as`: `x as T << 3`.

No change needed:

- Row 12 (cargo-script frontmatter) and row 22 (nightly-only syntax):
  out of scope, unchanged.

Adapter stability: node kinds and fields used by
crates/sloplint-syntax/src/lang/rust.rs are unchanged. Trees of
previously valid code are byte-identical to the stock grammar except
`try!(...)`, which changed from an ERROR recovery to a clean
`macro_invocation`. Empty where bounds produce a `where_predicate`
without a `bounds` field; the adapter never reads that field.

Corpus results with tree-sitter-cli 0.27.1 `tree-sitter test`:

- Before (pristine v0.24.2 checkout): 151/151 pass.
- After (patched): 151/151 pass, plus 22/22 new sloplint cases.
- One interim regression during development (`impl 'a + Clone` tree
  shape) was caught by the corpus and reverted before vendoring.
