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
  regeneration it MUST resolve to this repo's own `vendor/tree-sitter-c`
  (e.g. `NODE_PATH=<repo>/vendor`): the grammar inherits and references
  sloplint-only C rules such as `preproc_ifdef_in_statement`, so pointing
  it at upstream tree-sitter-c (any tag) fails to load with
  `Undefined symbol 'preproc_ifdef_in_statement'`. No file from it is
  copied here. Do not leave a `node_modules/` dir behind.
- Commands, run in this directory:
  `NODE_PATH=<repo>/vendor tree-sitter generate`, then
  `NODE_PATH=<repo>/vendor tree-sitter test`.
- It rewrites `src/parser.c`, `src/grammar.json`, `src/node-types.json` and
  `src/tree_sitter/*.h`. There is no `src/scanner.c`.
- Regenerating the unpatched grammar with 0.27.1 moves the parser from ABI 14
  to ABI 15 with no rule and no node-kind change (454 kinds before and after).
- `test/corpus/sloplint_*.txt` holds 33 new guard cases (upstream corpus
  format: 5 enum, 4 string-enum, 6 annotation, 5 preproc, 3 atomic
  linkage, 1 signed-number call arg, 1 macro-argument enum, 4 member
  macros, 1 signature preproc, 3 class-dump). They were produced
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

- Member macro generators: `interface_declaration` and
  `implementation_definition` accept `$.macro_declaration`, so a bare
  `NAME(args)` line between members parses as a declaration macro
  (`AAPropSetFuncImplementation(AAOptions, NSString *, name)` in AAChartKit,
  `QMUISynthesizeBOOLProperty(isOn, setOn)` in QMUI_iOS).
- Trailing method attributes: `method_definition` and `method_declaration`
  take `repeat(choice($._declaration_modifiers, $.macro_qualifier))`, so
  identifier-shaped attribute macros after the selector parse
  (`- (void)m SPU_OBJC_DIRECT { ... }` in Sparkle).
- Bare statement macros: `_non_case_statement` gains
  `alias(prec.dynamic(-2, $.identifier), $.macro_declaration)`, so a lone
  identifier on its own line inside a method body parses as a statement
  macro (`MJRefreshCheckState`, `BeginIgnorePerformSelectorLeaksWarning`).
  Unreachable as an `expression_statement` (no `;`) and dynamic -2 keeps any
  declarable read ahead of it.

- `#if` inside a method signature's selector list: a new
  `preprocIf('_in_method_signature')` suite whose arm items are
  `selector ':' param` pairs or `, ...`, aliased into the
  `method_declaration`/`method_definition` parameter repeats —
  `- (instancetype)initWithEd:(NSString *)ed\n#if LEGACY\n
  dsa:(NSString *)dsa\n#endif\n error:(NSError **)e;` (Sparkle).
- The `preprocIf('')` arm set and C-base rules ride the vendored C
  grammar via `node_modules/tree-sitter-c`; regenerating pulls in the
  latest C fixes (bare `NAME` arms, `__ASM`, declaration-specifier arms).

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
- `- (instancetype)initWithEd:(NSString * _Nullable)ed` newline `#if
  SPARKLE_BUILD_LEGACY_DSA_SUPPORT` newline `dsa:(NSString *
  _Nullable)dsa` newline `#endif` newline `error:(NSError **)error;`
- `#if __has_warning("...")`, `#if __has_include(<...>)/("...")`
- `#ifdef` around `@property` in `@interface` and around a method definition
  in `@implementation`
- `- (void).cxx_destruct;` — clang's synthesized `.cxx_destruct` selector
  in class-dumped headers; `method_selector_no_list` accepts a leading
  `.` before the identifier
- `enum { kMax = 100, };` and `struct _opaque_pthread_rwlock_t;` inside
  `@interface` ivar braces — `instance_variable` gained a declarator-less
  `enum_specifier`/`union_specifier`/`struct_specifier` + `;` arm
- `SEL selector __unused = 0;` — `init_declarator` mirrors the C base's
  post-declarator `macro_attribute`/`macro_qualifier`/`ms_call_modifier`
  repeat (the ObjC override had dropped it)

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

## Sweep evidence (second corpus, 2026-11-18)

Recount over the ObjC error-cluster repositories of the 23-language corpus
(`meta/parse-error` findings, vendored grammar vs the previous vendored
revision): AAChartKit 114 to 34, QMUI_iOS 62 to 22, Sparkle 127 to 69,
SDAutoLayout 30 to 10, MJRefresh 26 to 11, JXCategoryView 18 to 7, FLEX 23 to
16, HexFiend 16 to 12, EarlGrey 31 to 26, Darling 240 to 226, Masonry
unchanged. Residual clusters are `SHADER_STRING(...)` raw-GLSL bodies
(GPUImage, 134 files; the macro argument is GLSL, not ObjC) and `#if` arms
splitting binary expressions (Masonry's `MASAttribute` bitmask chain): both
need the real preprocessor and stay errors by design. Upstream corpus plus
`sloplint_*.txt`: 103 of 103.

## Sweep evidence (third corpus, 2026-11-19)

Same recount, class-dump round (`method_selector_no_list` dot selector,
declarator-less ivar declarations, post-declarator macro attributes):
XVim 529 to 212 still-erroring files (317 went fully clean). GPUImage
stays at 134 — all of it is `SHADER_STRING(...)` raw-GLSL argument
bodies, the preprocessor ceiling already documented. Residual XVim
errors are C++ template syntax inside ObjC headers
(`struct vector<T*, allocator<...>> {}`), which a C-family grammar does
not model. Upstream corpus plus `sloplint_*.txt`: 109 of 109.

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
- **`#` lines with no directive name** — `preproc_null` (same shape as the
  C grammar's: whole-line banner token + `'#'` + immediate-newline arm)
  joins `_top_level_item`, `_block_item`, `interface_declaration`,
  `implementation_definition` and `_field_declaration_list_item`. Covers
  the null directive and the `# comment` banner idiom that BSD-derived
  sources use; see the C grammar's SLOPLINT.md for why it is a dedicated
  token/rule rather than an optional directive name or a `prec` token.
- **`extern "C"` braces split across preproc guards** — `guarded_linkage`
  (ported from the C grammar; the stock C base lacks it) joins
  `_top_level_item` and `_block_item`, covering the shared ObjC/C++ header
  idiom `#ifdef __cplusplus` / `extern "C" {` / `#endif` … `#ifdef` / `}` /
  `#endif`. Two new conflicts mirror the C grammar's: `_linkage_guard` vs
  `preproc_if`, and `_preproc_macro_name` vs `_linkage_guard` (this
  grammar's ifdef head uses `_preproc_macro_name`, not a bare identifier).
- **`_Atomic (type)` as a type specifier** — `macro_type_specifier` gains
  an `alias('_Atomic', $.identifier)` name arm (the C grammar's fix; the
  stock C base's `name` is a plain identifier), so `_Atomic (BOOL)*` works
  in ivars, method parameters, properties and casts. The dedicated
  `atomic_declaration` rule was removed — it only covered the
  `_Atomic (T) name;` ivar shape, not the pointer and parameter forms, and
  `struct_declaration` + `macro_type_specifier` now parse all of them
  uniformly.
- **Operator tokens as preproc call arguments** — `preproc_argument_list`
  gains the same comparison-operator item arm as the C grammar
  (`BOOST_WORKAROUND(BOOST_MSVC, < 1300)`, `CHECK(x, ==, y)`).
- **Bare top-level macro calls** — `_macro_declaration_top` +
  `macro_argument_list` (ported from the C grammar; the stock C base lacks
  them) join `_top_level_item` via `prec.dynamic(-2)`, covering
  declaration-expanding macros like `DEBUG_OPTION_DECLARE_GROUP (name)`
  and `DEBUG_OPTION_DECLARE_SWITCH_D (name)`. Unlike C, `NAME(args);`
  also resolves as `macro_declaration` (the macro read absorbs the `;`):
  header lines of that shape are practically always generator macros, so
  the macro fact is the more truthful record. `concatenated_string` is
  overridden without the inherited `call_expression call_expression`
  first pair — adjacent `NAME(args)` lines otherwise merged into one
  fake concat at `prec.dynamic(2)`, outranking the macro read and
  emitting phantom calls. New conflicts mirror the C grammar's
  `_macro_declaration_top` set plus `macro_argument_list` vs
  `type_specifier`/`type_qualifier`/`sized_type_specifier`/
  `generic_specifier`/`macro_type_specifier` and `expression` vs
  `concatenated_string`; the inherited `type_specifier` ×
  `_top_level_expression_statement` pair is now filtered out (made
  redundant by the three-way that includes `_macro_declaration_top`).
- **`number_literal` drops its leading sign** — same fix as the C grammar
  (see `vendor/tree-sitter-c/SLOPLINT.md`): the inherited
  `macro_argument_list` free-atom stream makes `number_literal` valid
  after an operand, so `f(1)-1`, `f(1) -1` and `s + sdslen(s) - 1` lexed
  the signed literal and left binary `-`/`+` with no operator — a
  whole-file ERROR. Signed operands still parse through
  `unary_expression` (`f(-1)`, `x - 1`); exponent tails keep their own
  `optional(/[-\+]/)`. The C corpus `Number literals` case now expects
  `-0.1f` as `unary_expression(number_literal)`.
- **`macro_argument_list` accepts `'__kindof'`, `'@selector'` and
  `block_literal`** — macro invocations like QMUI's
  `ExtendImplementationOfNonVoidMethodWithoutArguments([C class],
  @selector(init), __kindof C *, ^__kindof C *(C *s, __kindof C *r) {
  ... })` carry `__kindof`/`@selector` keywords and a
  `^type(params){body}` block literal as plain arguments; `';'` joined
  the token set for the bodies that ride the raw stream. Without them
  the `NAME(...)` read fell back to `call_expression`, whose
  expression-only `argument_list` dies on the type tokens
  (`__kindof C *`). The `@selector`/`__kindof`/`block_literal` keeps
  the parse inside `macro_argument_list` so the statement is a
  `macro_declaration`; the block still builds a real `block_literal`
  node. New conflict: `macro_argument_list` vs `block_literal` (a `^`
  token vs block start). Note: injecting the `preproc_*` directive
  rules into `enumerator_list` (X-macro enum bodies, whisper.cpp /
  CocoaAsyncSocket) was tried and reverted — it inflates the generated
  `parser.c` past GitHub's 100 MB file cap; those enum-directive
  clusters stay a known residual.
