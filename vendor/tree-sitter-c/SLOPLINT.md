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

- Tool: tree-sitter-cli 0.24.7 (`cargo install tree-sitter-cli --version
  0.24.7 --locked`). The generated `src/parser.c` and `src/tree_sitter/*.h`
  must come from the SAME CLI version (0.24.7 emits `TSFieldMapSlice`
  headers, 0.27.x emits `TSMapSlice`) or the crate fails to compile.
  After regenerating, purge `$XDG_CACHE_HOME/tree-sitter`.
- Command, run in this directory: `tree-sitter generate`
- It rewrites `src/parser.c`, `src/grammar.json`, `src/node-types.json` and
  `src/tree_sitter/*.h`. There is no `src/scanner.c`.
- Regenerating the unpatched grammar with 0.27.x changes `src/parser.c` by 15 bytes
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
  - `macro_qualifier` is a bare `identifier` with an optional
    `macro_argument_list` usable only in the pre-type position of
    `_declaration_specifiers` and before a struct/union/enum name. It is
    deliberately absent from `_declaration_modifiers`'s post-type
    position: there an identifier is a declarator, and allowing it
    produced `int name : 1` and `bool f(T a, T* b)` mis-parses.
    `prec.dynamic(-1)` makes every real type or declarator read win.
    The argument list is `macro_argument_list` only, never
    `argument_list`: `identifier + argument_list` is token-identical to
    `call_expression`, and sharing it collapsed the merged `(`-list parse
    states so in-body `NAME(CALL(args), ...)` degraded to `ERROR`.
  - `macro_declaration` is a bare `identifier` with an optional
    `macro_argument_list` and optional `;`. At `_top_level_item` the
    strict `_macro_declaration_top` variant is used (aliased at
    `prec.dynamic(-2)`): it requires an argument list or `;`, so a bare
    `NAME` cannot fragment `NAME(args)` into
    `macro_declaration` + `parenthesized_expression` + `MISSING ;`. The
    lenient rule is added to `_field_declaration_list_item` and catches
    items no other rule accepts, chiefly inside struct bodies where
    expression statements cannot appear. It is deliberately absent from
    `_block_item`: inside function bodies `NAME(args)` must always be a
    `call_expression` — reachable there, the `;`-tolerant fallback stole
    call sites whose arguments contain concatenated literals
    (`log_error("a" "b")`) and no precedence setting reliably gives them
    back. `_non_case_statement` instead carries the strict
    `_macro_declaration_stmt` variant (aliased at `prec.dynamic(-2)`):
    `identifier + macro_argument_list` and nothing else — no bare `NAME`,
    no `;`. `NAME(args)` with no `;` cannot be an expression statement,
    so it can only be a table-generator or statement macro
    (`FLATBUFFERS_GEN_TYPES(TD)` inside a switch). `NAME(args);` still
    resolves as `call_expression` + `;` unless a real macro item already
    appeared in the same block — the GLR merge then keeps the macro read
    and `;` lands as an empty `expression_statement`, which is benign for
    the dominant case (`FLATBUFFERS_ASSERT(0)`-style true macros).
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
- Win32 SDK headers (the ntdll.h family):
  - `type_definition` is two full alternates now — one with declarators,
    one without — so a declaratorless `typedef enum X {...};` inside an
    `#if` block parses. `optional($._type_definition_declarators)` is not
    used: the empty branch inserts a zero-width `MISSING` node, which
    counts as a parse error.
  - `_type_definition_type`'s pre-type qualifier slot accepts
    `_typedef_macro_qualifier` (aliased to `macro_qualifier`), a bare
    `identifier` with NO argument list — `typedef CONST char *PCSZ`.
    The full `macro_qualifier` (identifier + optional
    `macro_argument_list`) must not sit here: `typedef VOID (*P)(T)`
    would let `VOID` swallow `(*P)` as its macro arguments and kill the
    function-pointer typedef.
  - `midl_attribute_list` (`'[' commaSep1($._midl_attribute) ']'`,
    `_midl_attribute` = `'default'` or `choice('case', $.identifier) +
    optional($.macro_argument_list)`) leads `field_declaration` and
    `parameter_declaration` for `[size_is(N)]`, `[case(A,B)]`, `[default]`
  - Parenthesized declarator parameters (`void f(const J j, T (*arr)[N])`):
    the pre-type `macro_qualifier` swallowed `(*arr)` as its argument
    list. `parameter_declaration` now uses `_parameter_specifiers`,
    identical to `_declaration_specifiers` except the pre-type qualifier
    is a bare identifier (`_macro_qualifier_bare`, aliased back to
    `macro_qualifier`). `MACRO(args) T p` parameters are the accepted
    loss.
    members/params, and stands alone in `_field_declaration_list_item`
    for the `#ifdef MIDL_PASS` guard pattern where the bracket and the
    field it annotates sit in different preproc blocks.
  - `struct_specifier`'s named branch accepts `':' commaSep1(seq(
    optional($.identifier), $._type_identifier))` so `struct X : public Y`
    inside `#ifdef __cplusplus` in shared headers parses.
  - `_argument_list_type` carries `prec.dynamic(-1)`: with
    `_argument_list_real` open to type spellings, `int (*F)(int)` forked
    the read `call(*F, (int))` against the function-declarator read;
    -1 keeps the declaration. It must not go to -2 — there it ties
    `macro_declaration`/`_macro_declaration_stmt` and flips
    `SWAP(const char*, &a, &b)` to the macro read.

- GNU case ranges: `case_statement` accepts
  `case <expr> ... <expr>:` (`case '0' ... '9': return 1;`), the gcc/clang
  extension accepted by `gcc -fsyntax-only -std=gnu2x`. The high bound gets
  field `end`.

## Adapter impact

Node kinds and fields that `crates/sloplint-syntax/src/lang/c.rs` depends on
keep their names. `node-types.json` gains the `guarded_linkage` node kind
(`name` and `value` fields, same as `linkage_specification`), the
`macro_attribute`, `macro_qualifier`, `macro_declaration`,
`macro_argument_list` and `midl_attribute_list` node kinds, anonymous tokens
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
- `typedef CONST char *PCSZ;`, `typedef VOID(NTAPI *F)(IN PVOID X);`,
  `typedef VOID (*F)(IN PVOID X);`, `[size_is(N)] PUCHAR Buf;` on fields
  and parameters, `[size_is(...), length_is(...)]` inside a `#ifdef
  MIDL_PASS` block with the field after `#endif`, a declaratorless
  `typedef enum X {...};` inside `#if`, `struct X : public Y {...}`
  inside `#ifdef __cplusplus`, and `int (*F)(int)` keeping the
  function-declarator read under the widened argument list.
- `void f(const J j, T (*arr)[N])` and `int f(fd_set FAR* s, _In_ PUCHAR d)`
  — parenthesized declarator parameters and bare SAL qualifiers under
  `_parameter_specifiers`.
- Initializer value and the declaration's `;` carried inside preproc
  arms: `T x =\n#if A v1;\n#else v2;\n#endif`. `declaration` gained a
  second alternative ending in `_init_declarator_preproc` (aliased to
  `init_declarator`), whose value is `preproc_if`/`preproc_ifdef`
  `_in_init_declarator` — a new `preprocIf` suite whose arm items are
  `expression ';'`, `preproc_def`, `preproc_call`, `preproc_null`. The
  declarator is restricted to non-function forms (identifier, pointer,
  array, parenthesized): a `= #if` initializer is never a function, and
  allowing function declarators collided with old-style definitions.

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
- This vendor dir ships only the `sloplint_*.txt` corpus files (upstream
  corpus is not vendored): 16 files, 52 cases, all passing with the patch
  (`tree-sitter test` reports 52/52). The latest additions cover the Win32
  SDK cluster: typedef qualifier macros, function-pointer typedefs with
  SAL-annotated parameters, MIDL brackets, declaratorless typedefs inside
  `#if`, and `#ifdef`-guarded inheritance.

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

- **Preproc blocks splitting comma lists** (`parameter_list` and
  `initializer_list`) model each list item as
  `choice(real, real·blocks·[real], blocks·[real])` inside `commaSep1`,
  where a real item may follow a block comma-free because the separator
  comma often hides inside the block. Between two real items commas
  stay strict, which keeps the template-argument ambiguity
  (`T<A, U<B>, C>`) resolving on type-argument.

- **`argument_list` requires commas between real items** — the body is
  `repeat(choice(real·',', preproc·atom, ','))` then `optional(real)`,
  not the commaSep1 model above. Adjacent expressions can no longer
  split into two arguments, so `"a" "b"` is forced through
  `concatenated_string` and `MACRO "x"` arguments keep their
  identifier+literal chain shape. Preproc atoms
  (`preproc_if/ifdef_in_argument_list`, `preproc_def`, `preproc_call`)
  and bare `,` stay a free stream so separator commas hidden inside
  `#ifdef` blocks still work.

- **Commas are loose atoms inside the blocks themselves**
  (`#ifdef X , y #endif` in a list): the `preprocIf('_in_*_list', ...)`
  content is `repeat(choice(item, ','))`, so a `,` never demands a
  following item. Earlier shapes — per-item `optional(',')` edges and
  strict `seq(',', item)` separators — both committed the `,` to a
  required continuation on `#endif` and produced `MISSING identifier`
  or wrapped the block in `ERROR`. Making `,` a permissive atom is
  unambiguous at LR(1).

- **Infix continuations** (`a | b #ifdef | c #endif` — the alternate
  tokens of a binary expression hidden inside a preproc block): the
  `_in_argument_list` and `_in_initializer_list` block contents also
  accept `seq(op, expression)` items, where `op` is any binary
  operator. `binary_expression` itself gains a fallback row
  `seq(left, repeat1($._infix_continuation_preproc))`, and
  `_infix_continuation_preproc` aliases the
  `preproc_if/ifdef_in_argument_list` rules so an `expr #ifdef`
  sequence shifts one unambiguous rule — whichever reading
  (infix tail vs. block item) completes wins. A separate
  `_in_infix` preproc rule family was tried and removed: the two
  rules merged at the `#ifdef` and the deterministic merge
  systematically stole blocks meant for list items.

- **Statement fragments inside preproc blocks** — a block may end
  between `else` and its body (`} else #endif { }`) or carry a bare
  label (`#ifdef X fin: #endif stmt`): the `''`-suffixed block content
  accepts `dangling_else` (`if(cond) stmt else`, `else if` tails
  recurse) and `dangling_label` (`name:`) alongside `_top_level_item`.
  `_top_level_item` covers every `_block_item` member (`declaration`,
  all statement shapes via `_top_level_statement`) plus
  `macro_declaration`, so a `#ifdef` guard wrapping declarations or
  declaration macros parses at any nesting level. The matching
  `{ ... }` / statement after `#endif` parses as an ordinary
  block item.

- **A whole `#if`/`#ifdef` group as an unbraced statement body** —
  `if (x) #ifdef MAC a(); #else b(); #endif` (also `while`/`do`/`for`
  bodies, `else` consequences and labels). `_statement_or_preproc =
  choice($.statement, $.preproc_if, $.preproc_ifdef)` is the body slot
  for those positions. Deliberately NOT added to `_non_case_statement`:
  a `preproc_if` is already a direct `_block_item`, so wrapping it in
  `statement` too would make every block-level `#ifdef` ambiguous.

- **`NAME(args);` inside a function's `#ifdef` group is a call, not a
  `macro_declaration`** — `preprocIf('')` items are shared between file
  scope and function bodies, so `if (x) #ifdef MAC a(); #endif` used to
  give `a();` the file-scope macro read and the C adapter lost the call.
  `preprocIf('_in_statement', ...)` carries `_block_item` items (no
  `_macro_declaration_top` arm); `_block_item` and
  `_statement_or_preproc` alias it back to `preproc_if`/`preproc_ifdef`
  at use sites, so the node names are unchanged. File-scope guarded
  groups still use `_top_level_item` and keep the macro read.

- **Preproc inside expressions** — `a + #ifdef X b + #endif c`,
  `a && #ifdef X b #endif`, `x = #ifdef v1 #else v2 #endif`, and
  `( #ifdef expr #else expr #endif )`: `binary_expression` and
  `assignment_expression` gain rows that place
  `repeat1($._infix_continuation_preproc)` between operator and right
  operand (or where the right operand would be), and
  `parenthesized_expression` accepts block-led content. The
  `_in_argument_list`/`_in_initializer_list` block contents also accept
  trailing `expr op` fragments for the `b +` tails. Known gap:
  `TYPE MACRO(args) name` declarations (e.g. `unsigned char ALIGN(16)
  x[]`) stay unparseable — disambiguating the `NAME(args)` qualifier
  from a function declarator needs lexer help, not more LR rows.

- **Comments inside a `\`-continued `#define`/`#undef`/`#call` line**
  resume `preproc_arg` after the comment: the `value`/`argument` field
  is `repeat(choice($.preproc_arg, $.comment))` on `preproc_def`,
  `preproc_function_def`, and `preproc_call`.

- **`macro_argument_list` token soup** also accepts real expression
  rules (`$.pointer_expression`, `$.unary_expression`,
  `$.sizeof_expression`), the assignment/suffix operators `^=`, `<<=`,
  `>>=`, `%=`, `&=`, `|=`, `++`, `--`, `[`, `]`, `{`, `}` (brace-init
  args such as `M(name, T{})` / `M({a, b})`), and nested
  `$.macro_argument_list` — otherwise `*x`-shaped first arguments
  compete with the expression reading and land in `ERROR`.

- **Free-atom `enumerator_list`** — the body is now
  `repeat(choice($.enumerator, $.call_expression, '__extension__',
  preproc_if/ifdef aliases (comma and no_comma), $.preproc_def,
  $.preproc_call, ','))`, matching the `argument_list`/`initializer_list`
  stream model. `#define`-driven X-macro enum tables
  (`{ #define ET(E) E, GEN(ET) #undef ET }`) parse without comma
  bookkeeping, and `GEN(ET)` generator calls sit beside real
  enumerators.
- **`__extension__` inside preproc blocks** — the shared `preprocIf('')`
  block content accepts a bare `'__extension__'`
  (`#ifdef __GNUC__\n__extension__\n#endif`), and the
  `_in_enumerator_list`/`_in_enumerator_list_no_comma` block contents
  accept `$.enumerator`, `$.call_expression`, `'__extension__'` and
  `$.preproc_def`.
- **Type spellings as call arguments** — `_argument_list_real` gains
  `$._argument_list_type`, a strict `seq(repeat($.type_qualifier),
  choice($.primitive_type, $.sized_type_specifier, $.struct_specifier,
  $.union_specifier, $.enum_specifier), repeat('*'))`, so macro-style
  calls taking a type parse: `GET(uint8_t)`, `SWAP(void *, &a, &b)`,
  `SWAP(const char *, ...)`, `SWAP(unsigned int, ...)`,
  `SWAP(struct foo *, ...)`. `$.type_identifier` is deliberately not
  added: user-type spellings stay on the `identifier`-expression path
  with zero new ambiguity.

  - `_argument_list_operator` is a second new `_argument_list_real`
    member: the bare comparison tokens `==`, `!=`, `<=`, `>=`, `<`, `>`
    as items, for assertion macros that pass the operator as a token
    (`assert_int(0, ==, rv)` — munit/CMocka idiom). No expression can
    start with these tokens, so argument position stays unambiguous.
- **Trailing macros after a variable declarator** — `declaration`'s
  declarator seq and `init_declarator` carry a trailing
  `repeat(choice($.ms_call_modifier, $.macro_attribute,
  $.macro_qualifier))`, covering `} worker_id_map SEC(".maps");` and
  `int x PACKED = v;`. Same permissive model as the
  `function_definition` trailing-macro repeat.
- **`#` lines with no directive name** — new `preproc_null` rule is a member
  of every item list that already takes `preproc_call`. It covers the null
  directive (`#` alone or `#` + trailing blanks) and the pre-ANSI
  `# comment` banner idiom (`# /*`, `# *`, `# */`, `#---`, `#anything
  non-word`) that boost and BSD-derived sources use. The banner arm is one
  token that eats the whole line on purpose: a `preproc_arg` run there would
  let a trailing `*/` glue the *next* directive line into the argument
  stream (upstream's `\/[^*]` arg char-class matches `/\n`, which swallows
  the following `#define`/`#endif` and produces `MISSING` nodes — verified
  empirically on an optional-directive-name variant and rejected). A bare
  `#` goes through the `'#'` + immediate-newline arm so it never wins a
  lex-time tie against `#`-starting tokens (objc's `preproc_linemarker`
  showed why: same-length anonymous `'#'` stole the lex and the rule's
  continuation then errored — a token-level `prec(1)` was also tried and
  rejected: token precedence beats match length, so `#` shadowed every
  `#if`/`#define` in the file).
- **Operator tokens as preproc call arguments** — `preproc_argument_list`
  gains a `seq(choice('==','!=','<=','>=','<','>'),
  optional($._preproc_expression))` item, for macros that pass a bare
  comparison as an argument: `BOOST_WORKAROUND(BOOST_MSVC, < 1300)` and
  assertion-style `CHECK(x, ==, y)`. No ambiguity is possible: items are
  comma-delimited, so `a > b` still reads as one binary argument.
- **Directive terminators tolerate trailing blanks** —
  `preproc_include`/`preproc_def`/`preproc_function_def`/`preproc_call`
  end in `token.immediate(/[ \t]*\r?\n/)` (upstream:
  `token.immediate(/\r?\n/)`). A directive line ending in spaces or tabs
  (`#define KDDOCKWIDGETS_EXPORT ` inside a nested `#ifdef` —
  kddockwidgets_export.h) made the immediate token fail, the value
  position fell back to `preproc_arg`, the `\s` extras skipped to the
  next line and the next `#endif`/`#else` was eaten as the argument —
  cascading `MISSING` nodes and a whole-file ERROR. Absorbing the blanks
  inside the terminator token is safe: at `A B\n` the regex still fails
  on `B`, so argument lexing is unchanged; a `\<newline>` continuation
  still goes through `preproc_arg`'s `\\\r?\n` arm. Dropping
  `immediate` entirely is not an option — `\n` is itself an extra
  (`/\s|\\\r?\n/`) and is consumed as one before it can terminate the
  rule.
- **`concatenated_string` has no `call call` first pair** — the vendored
  grammar previously let `seq($.call_expression, $.call_expression)`
  open the concatenation. Adjacent `NAME(args)` lines on separate lines
  (consecutive `munit_void_test_decl(x)`/KDBINDINGS table rows,
  `DEBUG_OPTION_DECLARE_*` runs) then merged into a single
  `concatenated_string` at `prec.dynamic(2)`, outranking the
  `prec.dynamic(-2)` macro read — producing phantom call facts and
  violating the rule's own "one item must be a string_literal"
  invariant. The pair is removed; `call "lit"`, `"lit" call` and the
  `repeat` tail still cover stringification macros such as
  `XSTR(a) "." XSTR(b)` and `PRIu64 "x"` chains. Three new conflict
  entries cover enumerator/preproc reads that no longer had the
  call-call escape route.
