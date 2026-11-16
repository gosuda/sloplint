# tree-sitter-c, vendored for sloplint

Date: 2026-10-20.

## Origin

- Upstream repository: https://github.com/tree-sitter/tree-sitter-c
- Upstream tag: v0.24.2 (commit b780e47fc780ddc8da13afa35a3f4ed5c157823d).
- Published crate: tree-sitter-c 0.24.2 (crates.io). Name and version are unchanged.
- This directory is the published crate plus the patch below. `README.md`, `LICENSE`,
  `queries/`, `bindings/rust/` and `Cargo.toml` are the published files.
- There is no external scanner in this crate, and none was added.
- `sloplint-grammar.patch` is the diff of `grammar.js` against the
  upstream tag. It applies with `git apply` in a clone of tag v0.24.2.

## Why

The published grammar reports a parse error on valid C. sloplint skips a whole file on
any error node. These patches remove the errors that were reproduced on real code.
Every repro below was first checked with `gcc -fsyntax-only -std=gnu2x`
(gcc 16.0.1; clang 23.1.3 agrees except where noted).

## Regenerating src/

- Tool: tree-sitter-cli 0.27.1.
- Command, run in this directory: `tree-sitter generate`
- It rewrites `src/parser.c`, `src/grammar.json`, `src/node-types.json` and
  `src/tree_sitter/*.h`. There is no `src/scanner.c`.
- Regenerating the unpatched grammar with 0.27.1 changes `src/parser.c` by 15 bytes
  (3,872,012 to 3,872,027) and refreshes `src/tree_sitter/array.h`. It adds or removes
  no node kind and no field.
- Run `tree-sitter test` in an upstream clone with this `grammar.js` and the
  `test/corpus/sloplint_*.txt` files copied in. Upstream does not ship them.
  Note: the CLI caches compiled parsers by language name in
  `~/.cache/tree-sitter/lib/`; parallel workers for other languages share that
  cache, so set `XDG_CACHE_HOME` to a private directory when testing.

## Changes to grammar.js

- Complex types: `type_qualifier` accepts `_Complex` and `complex`
  (`double complex z;`, `complex double u;`, `long double _Complex q;`).
  No scanner is used; `complex` is a plain keyword, so a declarator literally
  named `complex` right after a primitive type (as in `complex z;` with a user
  typedef) no longer parses. See the sweep result below.
- Thread-local storage: `storage_class_specifier` accepts `_Thread_local`
  (`_Thread_local int x;`, `static _Thread_local int y;`).
- Atomic functional form: `macro_type_specifier` accepts `alias('_Atomic',
  $.identifier)` as its name, so `_Atomic(int)` reuses the existing
  `macro_type_specifier` node (`typedef _Atomic(int) t;`,
  `struct S { _Atomic(unsigned) u; };`, `void f(_Atomic(int) *p);`).
  The plain qualifier form (`_Atomic int b;`) is unchanged.
- `#embed` in initializers: `initializer_list` accepts `$.preproc_call` as an
  element, so an embed directive parses alone in braces or between commas
  (`char d[]={` newline `#embed "x" limit(5)` newline `};`,
  `{1,` newline `#embed <x>` newline `, 2}`). Two embeds in a row with no comma
  between them are still rejected.
- Feature checks in `#if`: `preproc_argument_list` accepts `$.string_literal`
  and `$.system_lib_string`, so `__has_include(<endian.h>)`,
  `__has_include("x.h")`, `__has_include_next(<x.h>)` and
  `__has_embed("x")` parse. `__has_attribute(x)` and the other
  identifier-argument checks already parsed.
- Fixed enum underlying types: a new hidden `_enum_underlying_type` rule
  (with `prec.right`) accepts `primitive_type` or `sized_type_specifier` after
  the colon, in both the named and the anonymous branch
  (`enum : unsigned short {A=2};`, `enum E : unsigned short {B=2};`).
- Unnamed bitfields: `_field_declaration_declarator` accepts a bare
  `$.bitfield_clause` in the comma-separated list
  (`struct S{unsigned:0; int x;};`, `struct S{int a:3, :4, b:2;};`).
- Register variables with an asm label and an initializer: `init_declarator`
  accepts an optional `$.gnu_asm_expression` before the `=`
  (`register long a7 __asm__("a7") = n;`). The label without an initializer
  already parsed. One conflict is declared for this:
  `[$._declarator, $._declaration_declarator]`.
- `__signed` spellings: the `signed`/`unsigned`/`long`/`short` modifier lists
  in `sized_type_specifier` and `_type_declarator` also accept `__signed__`
  and `__signed` (`typedef __signed char i8;`).
- `extern "C"` inside preproc guards: a new `guarded_linkage` rule covers the
  shared-header idiom where the opening brace lives inside one conditional
  block and the closing brace inside a later one, so neither block parses on
  its own (`#ifdef __cplusplus` newline `extern "C" {` newline `#endif`,
  declarations, then `#ifdef __cplusplus` newline `}` newline `#endif`).
  A hidden `_linkage_guard` matches `#ifdef`/`#ifndef`/`#if` heads on both
  sides. The rule sits in `_top_level_item` and `_block_item`, uses
  `prec.dynamic(10)` to win the ambiguity against `preproc_if` and
  `preproc_ifdef`, and declares two conflicts: `[$._linkage_guard,
  $.preproc_if]` and `[$._linkage_guard, $.preproc_ifdef]`. It keeps the
  `name` and `value` fields of `linkage_specification`.
- Generic macro fallbacks for names no literal list can enumerate
  (`ASIO_DECL`, `FMT_API`, `MYLIB_EXPORT` and friends):
  - `macro_qualifier` is a bare `identifier` (optionally with an argument
    list) usable only in the pre-type position of `_declaration_specifiers`
    and before a struct/union/enum name. It is deliberately absent from
    `_declaration_modifiers`'s post-type position: there an identifier is a
    declarator, and allowing it produced `int name : 1` and
    `bool f(T a, T* b)` mis-parses. `prec.dynamic(-1)` makes every real type
    or declarator read win.
  - `macro_declaration` is a bare `identifier` with an optional argument
    list and optional `;`, added to `_top_level_item`, `_block_item`,
    `_non_case_statement` and `_field_declaration_list_item`. It only
    catches items no other rule accepts (a lone `NAME` or `NAME(...)` with
    no trailing `;`), chiefly inside struct bodies where expression
    statements cannot appear. `prec.dynamic(-1)` keeps real statements and
    declarations preferred.
  - `macro_argument_list` is the permissive token list these two rules
    share: identifiers, primitive and sized types, literals, commas and the
    operator characters that appear inside real macro arguments
    (`<`, `>`, `*`, `&`, `=`, `!`, `?`, `:`, `.`, `->`, `~`, arithmetic
    and comparison operators, `const`).
  - `preproc_params` accepts a trailing `...` and `identifier...` so
    variadic function-macro definitions parse.
  - `primitive_type` gains the compiler-extension spellings seen in corpus
    headers: `_Bool`, `__fp16`, `__bf16`, `__vector`, `__pixel`,
    `__int128_t`, `__int{8..128}`, `_Float{16,32,64,128}`,
    `_Float{32,64,128}x`, `_Decimal{32,64,128}`.
- Calling-convention and pointer-model macros inside declarators
  (`void* __cdecl memset(...)`, `VOID(NTAPI* FN)(...)`, `fd_set FAR*`,
  `int WINAPI WinMain(...)`, `_In_`-style SAL annotations):
  - `repeat(choice($.ms_call_modifier, $.far_ptr_modifier,
    $.macro_attribute, $.macro_qualifier))` before `'*'` and a merged
    literal-only `repeat(choice($.ms_pointer_modifier,
    $.type_qualifier, $.ms_call_modifier, $.far_ptr_modifier,
    $.macro_attribute))` after it, in all four pointer-declarator rules;
    `repeat(choice($.ms_call_modifier, $.macro_attribute))` before the
    inner declarator of the four parenthesized-declarator rules and in the
    `declaration`/`function_definition` pre-declarator slots.
  - `far_ptr_modifier` is a literal list (`FAR`, `NEAR`, `_far`, `_near`,
    `__far`, `__near`, `__huge`, `_huge`); WINAPI/NTAPI/APIENTRY and
    friends already live in `macro_attribute`, so they were not duplicated
    into `ms_call_modifier`.
  - The pre-declarator slots are literal-only (`ms_call_modifier` +
    `macro_attribute`): a generic `macro_qualifier` there let `f(...)`
    fork a GLR branch whose argument-list interpretations multiplied
    parse versions until the real declarator path was pruned. The generic
    `macro_qualifier` stays before `'*'` in C: the C++-only `delete [] p`
    ambiguity that forbids it there cannot occur here, and
    `typedef CONST STRING* X` needs it. Post-`'*'` is split by rule kind:
    concrete pointer declarators keep `macro_qualifier`
    (`*RESTRICTED_POINTER x`), but `abstract_pointer_declarator` does not —
    a generic post-`'*'` identifier made `(*cmp)` an abstract declarator
    (`abstract_parenthesized_declarator` out-scores the concrete
    `parenthesized_declarator`, prec -10) and lost the parameter's name.
- Attribute-like declaration macros: vendor headers pre-decorate declarations
  with calling-convention, storage and compiler-attribute macros
  (`__IO uint32_t ISER[1];`, `VKAPI_ATTR VkBool32 VKAPI_CALL cb(void *);`,
  `NAPI_CDECL f(int)` with the macro in the type position). The grammar's
  `word` token intercepts every identifier-shaped input, so a regex token
  can never match an identifier subset; a new `macro_attribute` rule
  enumerates the widespread literals instead (Windows calling conventions,
  CMSIS, Linux sparse attributes, glibc attribute macros and the corpus'
  frequent `*_API` modifiers). It joins `_declaration_modifiers` and
  `type_specifier`, with conflict `[$._declaration_modifiers,
  $.type_specifier]`. Trade-off: an identifier equal to a listed literal can
  no longer be a real type or declarator.

## Adapter impact

Node kinds and fields that `crates/sloplint-syntax/src/lang/c.rs` depends on
keep their names. `node-types.json` gains the `guarded_linkage` node kind
(`name` and `value` fields, same as `linkage_specification`), the
`macro_attribute`, `macro_qualifier`, `macro_declaration` and
`macro_argument_list` node kinds, anonymous tokens
(`_Complex`,
`complex`, `_Thread_local`, `__signed`, `__signed__`) and new homes for
existing named kinds (`system_lib_string` in preproc arguments,
`sized_type_specifier` as an enum underlying type, `gnu_asm_expression` in
`init_declarator`, `preproc_call` in `initializer_list`). No Rust adapter
change was needed. `crates/sloplint-syntax/src/lang/c.rs` is untouched.

## Constructs verified fixed

Each was a parse error with the published crate and is valid for
`gcc -fsyntax-only -std=gnu2x`.

- `double complex z;`, `float complex z;`, `long double complex z;`
- `double _Complex z;`, `_Complex double z;`, `complex double z;`
- `typedef double complex c;`, `double complex f(double complex x);`
- `_Thread_local int x;`, `static _Thread_local int x;`, `extern _Thread_local int e;`
- `typedef _Atomic(int) t;`, `_Atomic(int) a;`, `_Atomic(int*) c;`
- `char d[]={` newline `#embed "x" limit(5)` newline `};`
- `#if __has_include(<endian.h>)`, `#if __has_include("x.h")`,
  `#if __has_include_next(<x.h>)`, `#if __has_embed("x") == 1`,
  `#if defined(__has_include) && __has_include(<x.h>)`
- `enum : unsigned short {A=2};`, `enum E : unsigned short {A=2};`
- `struct S{unsigned:0; int x;};`, `struct S{int a:3, :4, b:2;};`
- `register long a7 __asm__("a7") = n;`, `int x __asm__("y") = 3;`
- `typedef __signed char i8;`, `__signed int x;`, `__signed__ int y;`
- `ASIO_DECL int f(void);`, `CUSTOM_API struct S { int x; };`, a bare
  `MY_MARKER` or `MY_MARKER(x, y)` item inside a struct body, and
  `#define LOG(fmt, ...) printf(fmt, __VA_ARGS__)`

## Constructs known still failing

- `complex z;` where `complex` is a user typedef name. Making `complex` a
  grammar keyword (no scanner in this crate) reserves the word, so this rare
  form changed from clean to error. No currently clean corpus file uses it
  (see sweep result); the `double complex` headers and libc test files that
  use the word are already errors for other reasons or now parse. Kept the
  keyword because the assigned `double complex` repro needs it.
- Two `#embed` directives in a row with no separating comma.
- `vector` type names (AltiVec, target-specific and contextual; out of scope).
- `__kconfig` (a BPF section-attribute macro, not a language construct).
- `typedef enum : unsigned char t;` (anonymous enum with an underlying type
  but no body). gcc and clang both reject this, so it correctly stays an error.
- Macro-dependent code (annotation macros, split `#ifdef` statements,
  unknown-macro statements) stays out of scope per the gap list.

Inputs that are not valid C and so stay errors: `int x = ;`, unterminated
`struct S{unsigned:0;`, a missing function-body brace.

## Corpus results

- Upstream `tree-sitter test` at tag v0.24.2, before: 85 of 85 pass.
- After the patch: 85 of 85 upstream cases pass.
- New cases in `test/corpus/sloplint_*.txt`: 25 cases (21 repros, 4 controls).
  The unpatched grammar fails 21 of the 25. The patched grammar passes all 25.
- Total with the patch: 110 of 110.

## Sweep result

Tree comparison between the published parser and this one on 30496 `.c` and
`.h` files from the cargo registry, the Go module cache, miniconda
site-packages and `~/compiler`: files with an error node 18954 before, 18871
after. 83 files changed from error to clean. No file went from clean to
error. The fixed files are the patched constructs: `_Thread_local`
declarations, `double complex` variables, `__has_include` checks, fixed
underlying enums, unnamed bitfields, `_Atomic(...)`, `_Complex`, `#embed`
and register variables with asm labels. 15 files (all copies of one Go DWARF
`typedef.c` test file) stay clean with one changed node: an unnamed
`float complex` parameter that the old grammar read as a parameter named
`complex` and the new grammar correctly reads as the complex type. 2619
still-error files have a partly changed tree; every other tree is identical.

The `macro_attribute` change was measured separately on 412 corpus files
that carried error nodes (the most error-dense .c/.h files of the big
repos run): error nodes 19,312 before, 17,937 after; files with any error
403 before, 397 after; six files fully clean; no clean file went to error.

## Size

`src/parser.c`: 7,488,274 bytes (published: 3,872,012; regenerated unpatched
with 0.27.1: 3,872,027). States: 2122 (published: 2015). Symbols: 367
(published: 360). The literal enumeration of `macro_attribute` costs most of
the size growth.

- **MSVC `__asm {}` / `_asm {}` blocks** parse as a statement-level
  `ms_asm_statement` wrapping a `ms_asm_block` token soup (identifiers,
  literals, brackets, operators, comments). Instruction mnemonics and
  register names are ordinary identifiers, so the block accepts a broad
  token set instead of modelling real assembly. It lives at statement
  position (`_non_case_statement`) because MSVC blocks take no trailing
  `;`. The GNU `asm(...)` form stays in `gnu_asm_expression`, where it is
  also used after declarators (`void f() asm("alias")`). Known gap: the
  single-instruction form `__asm int 3` (no braces) is not covered —
  the token soup cannot see newlines to bound the instruction.

- **`#ifdef` blocks splitting a `parameter_list`** (`f(int a #ifdef X ,
  int b #endif , int c)` — nghttp2-style conditional signatures) parse
  via `preproc_ifdef_in_parameter_list`/`preproc_if_in_parameter_list`
  (from `preprocIf('_in_parameter_list', ...)`) whose items each carry an
  optional leading comma, so a block may sit between parameters without
  a comma before the directive. Blocks are either standalone items in
  the list or attached to the preceding parameter; commas between real
  parameters stay strict, which keeps the template-argument ambiguity
  (`T<A, U<B>, C>`) resolving on type-argument.
