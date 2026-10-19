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

## Adapter impact

Node kinds and fields that `crates/sloplint-syntax/src/lang/c.rs` depends on
keep their names. `node-types.json` only gains anonymous tokens (`_Complex`,
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
- New cases in `test/corpus/sloplint_*.txt`: 20 cases (16 repros, 4 controls).
  The unpatched grammar fails 16 of the 20. The patched grammar passes all 20.
- Total with the patch: 105 of 105.

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

## Size

`src/parser.c`: 4,039,361 bytes (published: 3,872,012; regenerated unpatched
with 0.27.1: 3,872,027). States: 2054 (published: 2015). Symbols: 366
(published: 360). Tokens: 166 (published: 161).
