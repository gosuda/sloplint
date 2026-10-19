# tree-sitter-objc, vendored for sloplint

Date: 2026-10-20.

## Origin

- Upstream repository: https://github.com/tree-sitter-grammars/tree-sitter-objc
- Upstream tag: v3.0.2 (`18802acf31d0b5c1c1d50bdbc9eb0e1636cab9ed`, matches the
  crate `.cargo_vcs_info.json` sha1).
- Published crate: tree-sitter-objc 3.0.2 (crates.io). Name and version are unchanged.
- This directory is the published crate plus the patch below. `Cargo.toml`,
  `Cargo.toml.orig`, `README.md`, `queries/`, `bindings/rust/` and `src/` are
  the published files (the crate ships no `LICENSE` file; the tag's is used).
- Added from the upstream tag (the crate does not ship them): `LICENSE`,
  `grammar.js`, `tree-sitter.json`, `test/corpus/` (the upstream C regression
  suite, the only corpus upstream ships). `test/` is byte-identical to the tag
  apart from the new `sloplint_*.txt` files.
- Added for packaging, not from upstream: `.gitattributes` pins
  `test/corpus/c/crlf.txt` to `-text`, so the CRLF fixture keeps its carriage
  returns on machines with `core.autocrlf=input` (without the pin, `git add`
  normalizes them away and the CRLF case stops testing CRLF handling).
- `sloplint-grammar.patch` is the diff of `grammar.js` against the upstream
  tag. It applies with `git apply` in a clone of tag v3.0.2 (verified) and
  reproduces this `grammar.js` byte for byte.
- Upstream `master` is byte-identical to v3.0.2 in `grammar.js`; none of these
  fixes exist upstream.

## Why

The published grammar reports a parse error on valid Objective-C that ships in
AFNetworking and SDWebImage headers. sloplint skips a whole file on any error
node, and `.h` files that fail as C fall back to C++ then Objective-C, so
these headers were skipped entirely. These patches remove the errors
reproduced on the two corpora (343 `.h`/`.m` sources). Every fix below was
checked against the real headers; Apple-macro shapes follow the Foundation
headers (`NSObjCRuntime.h`, `NSAvailability.h`), not the preprocessor output.

## Regenerating src/

- Tool: tree-sitter-cli 0.27.1.
- The `grammar.js` starts with `require('tree-sitter-c/grammar')`. For
  regeneration it resolves to a clone of
  https://github.com/tree-sitter/tree-sitter-c at tag v0.23.4 (commit
  `3efee11f784605d44623d7dadd6cd12a0f73ea92`), the version pinned by the
  upstream `package-lock.json` (`tree-sitter-c 0.23.4`). Place it at
  `node_modules/tree-sitter-c` next to `grammar.js` (a symlink to a tag clone
  works). No file from it is copied here. Remove `node_modules/` before
  committing; it is not vendored.
- Commands, run in this directory with that symlink in place:
  `tree-sitter generate`, then `tree-sitter test`.
- It rewrites `src/parser.c`, `src/grammar.json`, `src/node-types.json` and
  `src/tree_sitter/*.h`. There is no `src/scanner.c`.
- Regenerating the unpatched grammar with 0.27.1 moves the parser from ABI 14
  to ABI 15 with no rule and no node-kind change (454 kinds before and after).
- `test/corpus/sloplint_*.txt` holds 20 new guard cases (upstream corpus
  format: 5 enum, 4 string-enum, 6 annotation, 5 preproc). They were produced
  by `tree-sitter test --update` and read by hand: none contains an ERROR or
  MISSING node, and the `enum_specifier` alias yields exactly one node per
  `NS_ENUM` typedef (an early revision aliased an inline `seq` and produced
  seven fragments; the corpus caught it before commit).
- A note on harnesses: the `tree-sitter` CLI caches compiled parsers by
  language name, so every CLI number below was measured with a private
  `XDG_CACHE_HOME`. Cargo-harness numbers use the vendored crate through the
  workspace build. Verdicts throughout are raw-tree `root.has_error()` plus a
  `to_sexp` MISSING scan, which also sees hidden recovery nodes that cursor
  walks miss.

## Changes to grammar.js

- Fixed-underlying-type enum typedefs: `type_definition` gains a
  `typedef NS_ENUM(UIntegerType, Name) { ... };` alternative, also accepting
  `NS_OPTIONS`, `NS_ERROR_ENUM`, `CF_ENUM`, `CF_OPTIONS` and `NS_CLOSED_ENUM`.
  The macro call is parsed by a hidden `_ns_enum_specifier` rule aliased to
  `enum_specifier` with the same `name`, `underlying_type` and `body` fields
  a plain `typedef enum Name : Type { ... };` would carry. The rule must be
  named: aliasing an inline `seq` renames each child instead of wrapping them.
  `NS_ERROR_ENUM`'s first argument is a domain value, not a type, but it takes
  the same `underlying_type` field position; adapters ignore the field value.
- Trailing string-enum annotations: `_type_definition_declarators` accepts one
  of `NS_STRING_ENUM`, `NS_EXTENSIBLE_STRING_ENUM`, `NS_TYPED_ENUM`,
  `NS_TYPED_EXTENSIBLE_ENUM` after the declarators
  (`typedef NSString * Name NS_STRING_ENUM;`). Anonymous tokens add no node.
- Bare Apple annotation macros: a hidden `_standalone_annotation` rule matches
  `NS_ASSUME_NONNULL_BEGIN`, `NS_ASSUME_NONNULL_END`, `NS_SWIFT_NONISOLATED`
  and `NS_SWIFT_UI_ACTOR` on their own line. It is a member of
  `_top_level_item`, `_block_item` and `interface_declaration`, covering
  regions, `@interface` prefixes, and the `NS_ASSUME_NONNULL_END` that sits
  inside `@interface..@end` (SDImageFramePool.h). Every occurrence in the
  corpora is on its own line, so no declaration-modifier membership was
  needed. Hidden with only anonymous tokens: no new node kind.
- Preprocessor macro names: the new keywords beat `identifier` in the lexer,
  which would break the `#ifndef NS_ENUM` / `#define NS_ENUM(...)` fallback in
  SDWebImageCompat.h. A hidden `_preproc_macro_name` rule (plain identifier or
  any introduced keyword) is now the `name` of `preproc_def`,
  `preproc_function_def`, the generic `preproc_ifdef` / `preproc_elifdef`, the
  objc `preproc_undef`, and every `preproc_ifdef`/`preproc_elifdef` the
  `preprocIf` helper generates.
- Feature checks in `#if`: `preproc_argument_list` also accepts
  `string_literal` and `system_lib_string`, so `#if
  __has_warning("-Wunguarded-availability-new")` and `#if
  __has_include(<UIKit/UITraitCollection.h>)` parse. One conflict is declared
  for this: `[$._preproc_expression, $.preproc_argument_list]`.
- Availability attributes: `API_DEPRECATED_WITH_REPLACEMENT` joins the
  `availability_attribute_specifier` names; its string plus
  platform-version-group arguments already fit the argument shapes
  (`API_TO_BE_DEPRECATED` is an identifier inside a version group).
- Stacked trailing attributes: `declaration` and `struct_declaration` take
  `repeat` instead of one `optional` declaration modifier, so
  `... API_UNAVAILABLE(ios, tvos, watchos) API_DEPRECATED("...", ...)` parses
  on properties and declarations (methods already repeated).
- Mid-enumerator attributes: `enumerator` accepts declaration modifiers
  between the name and the `= value`
  (`Foo API_DEPRECATED("...", ...) = 1 << 18,`). One conflict is declared:
  `[$.enumerator]`.
- `#ifdef` where `#if` already worked: `interface_declaration` and
  `implementation_definition` now use the specialized
  `preproc_ifdef_in_interface_declaration` /
  `preproc_ifdef_in_implementation_definition` (generated all along, never
  referenced) instead of the generic `preproc_ifdef`, so `#ifdef` around an
  `@property` or a method definition parses as interface content rather than
  misparsing as expressions. The generic entry was replaced, not added
  alongside: keeping both parses `- (void)bar;` two ways and the expression
  shaped one reports phantom calls.

## Adapter impact

Node kinds and fields that `crates/sloplint-syntax/src/lang/objc.rs` depends
on keep their names. `node-types.json` gains no new named kind (every new rule
is hidden or an alias); `enum_specifier` gains one optional `macro` field that
no hook reads. `tag_typedef` reads the aliased `name`/`body` fields, so
`NS_ENUM` names are type facts with no Rust change.
`crates/sloplint-syntax/src/lang/objc.rs` is untouched.

One known shape change: in `#ifndef NS_ENUM` / `#define NS_ENUM(...)` (and the
`NS_OPTIONS` pair) the macro name is now an anonymous keyword token instead of
an `(identifier)` node. No hook reads preproc macro names (only `value` is
read, for `##`), but a custom query matching `(preproc_ifdef name:
(identifier))` no longer matches those four spots.

## Constructs verified fixed

Each was a parse error with the published crate (raw tree, `has_error` plus
MISSING scan); each is clean now.

- `typedef NS_ENUM(NSUInteger, X) { ... };`, `NS_OPTIONS` with `1 << N`
  values, `typedef NS_ERROR_ENUM(Domain, X) { ... };`
- `typedef NSString * X NS_STRING_ENUM;` (and the `EXTENSIBLE` / `TYPED`
  siblings, pointer and integer forms)
- `NS_ASSUME_NONNULL_BEGIN` / `_END` around and inside `@interface`,
  `NS_SWIFT_NONISOLATED` / `NS_SWIFT_UI_ACTOR` before `@interface`
- `API_UNAVAILABLE(..) API_DEPRECATED("..", ..)` stacked on a property,
  `API_DEPRECATED_WITH_REPLACEMENT("..", ios(..), ..)` on a method,
  `API_AVAILABLE(macos(..), ..)` on a property
- `Foo API_DEPRECATED("...", macos(10.10, 10.10)) = 1 << 18` in an enum body
- `#if __has_warning("...")`, `#if __has_include(<...>)/("...")`
- `#ifdef` around `@property` in `@interface` and around a method definition
  in `@implementation`

## Sweep evidence (AFNetworking + SDWebImage, 343 `.h`/`.m` sources)

Raw-tree verdicts, published vs vendored crate through the same harness:

| Class | Count |
|---|---|
| Published failures | 69 (59 header paths = 39 unique contents incl. 20 `include/` mirrors; 10 `.m`) |
| Vendored failures | 10 (2 headers + 8 `.m`) |
| Fixed, fail to clean | 59 (57 header paths + 2 `.m`: `User.m`, `AFNetworkActivityIndicatorManager.m`) |
| New failures, clean to error | 0 |
| Clean under both, tree changed | 2 files (`SDWebImageCompat.h` + its `include/` mirror: only the four macro-name `(identifier)` nodes became keyword tokens, as designed) |

Upstream corpus: 75 of 75 before and after. New corpus: 20 of 20.
Gap tests (`crates/sloplint-syntax/tests/grammar_gaps/objc.rs`): 23 pass
vendored; 14 of them fail against the published crate, 5 pin already-clean
inputs, 4 pin invalid inputs that stay errors. Full `sloplint-syntax` and
`sloplint-core` suites pass.

## Still failing (honest residuals)

Every one needs a real preprocessor: the failing shape expands to different
token streams per configuration, so no fixed grammar accepts exactly the
valid inputs.

- `AFURLSessionManager.h:384,423`: the only failing header. Trailing
  `AF_API_AVAILABLE(ios(10), macosx(10.12), watchos(3), tvos(10))` and
  `AF_API_UNAVAILABLE(macos)` on method declarations. These are AFNetworking
  project macros, `#define`d in the same repo to `API_AVAILABLE(...)` /
  `API_UNAVAILABLE(...)` or to nothing. Accepting two third-party project
  names as grammar keywords would not generalize (next project brings its own
  prefix); Apple-platform names are in, project names are out. Everything else
  in the file parses.
- `SDInternalMacros.h:129`: `MISSING #endif`. `#if defined(__cplusplus)` /
  `extern "C" {` / `#endif` ... `#if defined(__cplusplus)` / `}` / `#endif`:
  the linkage braces balance only after preprocessing.
- `.m` files, same verdict under both grammars (positions may shift with
  recovery, all fail before and after):
  - `AFNetworkReachabilityManager.m`: `#if`/`#endif` splitting an
    `if`/`else if`/`else` chain inside a function.
  - `AFURLSessionManager.m`: cascades from the `AF_API_*` macros above.
  - `SDAnimatedImageView.m`: method declarators split across `#if`/`#else`
    (`- (void)viewDidMoveToSuperview` vs `- (void)didMoveToSuperview`) with a
    shared body after `#endif`.
  - `SDGraphicsImageRenderer.m`: `#if`/`#endif` splitting `{`/`}` inside
    method bodies.
  - `SDImageCoderHelper.m`: `SD_SEL_SPI(deviceInfoForKey:)`: a function-like
    macro invoked with trailing-colon selector syntax.
  - `SDImageIOAnimatedCoder.m`, `UIView+WebCache.m`, `SDUtilsTests.m`:
    same-position failures under both grammars, not in the header lane scope;
    left for a `.m` follow-up, if ever.
