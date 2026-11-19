# tree-sitter-cpp, vendored for sloplint

Date: 2026-10-20.

## Origin

- Upstream repository: https://github.com/tree-sitter/tree-sitter-cpp
- Upstream tag: v0.23.4 (commit f41e1a044c8a84ea9fa8577fdd2eab92ec96de02).
- Published crate: tree-sitter-cpp 0.23.4 (crates.io). Name and version are unchanged.
- This directory is the published crate plus the patch below. `README.md`,
  `bindings/rust/`, `queries/` and `Cargo.toml` are the published files.
  `LICENSE` is the upstream MIT license (the published crate files carry it
  only through `Cargo.toml`). `grammar.js` is new: the published crate omits
  it, so it is taken from the upstream tag.
- `sloplint-grammar.patch` is the diff of `grammar.js` and `src/scanner.c`
  against the upstream tag. It applies with `git apply` in a clone of tag
  v0.23.4. The scanner change adds the `_macro_braced_open` external token
  (see `macro_braced_statement` below).

## Why

The published grammar reports a parse error on valid C++. sloplint skips a
whole file on any error node. These patches remove the errors that were
reproduced on real code. Every repro below was first accepted by
`g++ -fsyntax-only -std=c++23` and `clang++ -fsyntax-only`.

## Regenerating src/

- Tool: tree-sitter-cli 0.24.7 (`cargo install tree-sitter-cli --version
  0.24.7 --locked`). 0.24.7 emits `LANGUAGE_VERSION` 14 and OLD-style
  `TSFieldMapSlice` headers; a newer CLI (0.27.x) emits ABI 15 and
  `TSMapSlice` headers — the generated `src/parser.c` and
  `src/tree_sitter/*.h` must come from the SAME CLI version or the crate
  fails to compile (`unknown type name 'TSFieldMapSlice'`). After
  regenerating, purge `$XDG_CACHE_HOME/tree-sitter` — a stale cached
  parser gives false greens.
- The C++ `grammar.js` starts with `require('tree-sitter-c/grammar')`. For
  regeneration it resolves to a clone of
  https://github.com/tree-sitter/tree-sitter-c at tag v0.23.1 (commit
  f4c21152f1952a99f4744e8c41d3ffb8038ae78c), the version pinned by the
  upstream `package-lock.json` (`tree-sitter-c 0.23.1`). Place it at
  `node_modules/tree-sitter-c` next to `grammar.js`. No file from it is
  copied here. The C++ worker and the C worker patch only their own grammar,
  with one exception: `guarded_linkage` lives in the C base, not in
  `grammar.js` here, because a rule added to this grammar never shares a
  parse state with the base's `preproc_if`/`preproc_ifdef` (the conflict
  declarations reported "unnecessary" and the rule never fired). Apply
  `sloplint-c-base.patch` with `git apply` inside that clone before
  `tree-sitter generate`; it carries every base change this grammar relies
  on (`guarded_linkage`/`_linkage_guard`, the `macro_qualifier` family,
  `_parameter_specifiers`/`_macro_qualifier_bare`, the MIDL/Win32 batch and
  the preproc/item members and conflicts they need). The same pair of conflicts is declared in this
  grammar's own `conflicts:` because the generator reads only the child's
  conflict list.
- Commands, run in this directory with that symlink in place:
  `tree-sitter generate`, then `tree-sitter test`.
- It rewrites `src/parser.c`, `src/grammar.json` and `src/node-types.json`.
  `src/scanner.c` is not generated and is untouched.
- Regenerating the unpatched grammar with 0.27.x moves the parser from
  ABI 14 to ABI 15; we stay on 0.24.7/ABI 14 for consistency. No node
  kind and no field is added or removed by
  the patch. `node-types.json` gains the `guarded_linkage` node kind
  (`name` and `value` fields, same as `linkage_specification`) and the
  `macro_attribute` node kind, and keeps every other type and field.

## Changes to grammar.js

- Preprocessor operands: `preproc_argument_list` also accepts
  `string_literal`, `system_lib_string` and a scoped name
  (`#if __has_include(<endian.h>)`, `__has_include("x")`, `__has_embed`,
  `__has_warning`, `__has_cpp_attribute(clang::fallthrough)`).
- Unnamed unscoped enum with a fixed base: `enum : unsigned short {A=2};`.
- Unnamed bit-fields: `struct S{unsigned:0; int x;};`.
- A bit-field may combine its width with a default member initializer:
  `struct S{int m:3{0};};`.
- A reference or pointer declarator with one identifier may take an argument
  list: `const std::string& s(v.get_str());`, `int *p(v.get());`. Two hidden
  rules do this and are aliased to `reference_declarator` and
  `pointer_declarator`, so the tree keeps its shape.
- Explicit instantiation of a class needs no declarator, with optional
  `extern`: `template class C<int>;`, `extern template class C<long>;`.
- Braced default argument: `void f(int x={}){}`.
- A namespace-scope `= delete` is a function definition, as inside a class:
  `bool operator==(A a,B b)=delete;`.
- A functional cast to a dependent type is a call:
  `g(typename S::First());`. The braced form is a compound literal:
  `typename S::First{}`.
- A pointer may appear in the allocated type of `new`: `new int*[n];`.
- `operator_name` is a valid member-access target: `x.operator==(y);`,
  `x->operator()(1);`, `this->Base::operator=(o);`. `->*` is a valid member
  access operator: `config->*field;`.
- New conflicts for the new ambiguities: `storage_class_specifier` with
  `template_instantiation`, `field_expression` and
  `qualified_field_identifier` each with `template_method`, `type_specifier`
  with `_class_name`, `_class_name` with `expression`, `_declarator` with
  each of the two new init declarator rules.
- `extern "C"` inside preproc guards: `guarded_linkage` and `_linkage_guard`
  arrive through the patched C base (see "Regenerating src/"); this grammar
  only adds the matching conflicts `[$._linkage_guard, $.preproc_if]` and
  `[$._linkage_guard, $.preproc_ifdef]`. The rule covers the shared C/C++
  header idiom where `extern "C" {` opens inside one conditional block and
  `}` closes inside a later one.
- Operator-token macro arguments (`KDBINDINGS_DEFINE_BINARY_OP(==)`,
  `(^)`, `(&&)`, `X(...)X(...)X(...)` sequences): `macro_argument_list`
  gained `^`, `<<=`, `>>=`, `->*`, `.*`, `operator`. A missing token
  there killed the macro path mid-sequence and let error-tolerant
  `X & Y(...)` binary expressions win whole lines.
- `decltype(x){args}` functional-cast brace init (`auto i =
  decltype(n){0}`): `compound_literal_expression`'s type choice gained
  `decltype`.
- `decltype` with a comma expression (`decltype(expr, void())`, the
  canonical SFINAE well-formedness probe): the operand choice gained
  `comma_expression`.
- Parenthesized declarator parameters (`void f(const J& j, T (&arr)[N])`,
  reference/pointer-to-array — the nlohmann `from_json` signature): in a
  parameter list an identifier followed by `(` must read as type plus
  parenthesized declarator, but the pre-type `macro_qualifier` swallowed
  `(&arr)` as its argument list. The base gains `_parameter_specifiers`,
  identical to `_declaration_specifiers` except the pre-type qualifier is
  a bare identifier (aliased back to `macro_qualifier`) — and
  `parameter_declaration`, `optional_parameter_declaration` and
  `variadic_parameter_declaration` now use it. `MACRO(args) T p`
  parameters are the accepted loss; `f(args)` calls and in-body macros
  are unaffected.
- Unnamed pointer parameters with defaults (`explicit A(Core::Object *
  = nullptr)`): `optional_parameter_declaration`'s declarator choice
  gained `$._abstract_declarator`; upstream rejects the `* =` shape for
  qualified types entirely.
- Known upstream gap (queued, not fixed): comparison operators like
  `>=` inside template argument lists
  (`enable_if_t<sizeof...(Ts) >= 1>`) — the `>`-close token steals the
  lex; `(...)` around the expression works.
- Attribute-like declaration macros: the C base gains `macro_attribute`, a
  literal enumeration of widespread calling-convention, storage and
  compiler-attribute macros (`__IO`, `WINAPI`, `VKAPI_ATTR`/`VKAPI_CALL`,
  `NAPI_CDECL`, CMSIS, Linux sparse, glibc attribute names and frequent
  corpus `*_API` modifiers), usable in `_declaration_modifiers` and as a
  `type_specifier`. This grammar adds `$.macro_attribute` to its own
  `type_specifier` and declares `[$._declaration_modifiers,
  $.type_specifier]`. The `word` token intercepts every identifier-shaped
  input, so a regex token could never match an identifier subset; literals
  are the only mechanism. Trade-off: an identifier equal to a listed
  literal can no longer be a real type or declarator.
- Qt and code-generation class, all literal enumerations for the same
  `word`-token reason:
  - `macro_declaration`: bare or call-shaped declaration-expanding macros
    valid as a top-level item or a field-declaration-list item
    (`Q_OBJECT`, `Q_GADGET`, `Q_NAMESPACE`, `QT_BEGIN_NAMESPACE`,
    `GENERATED_BODY` and the Unreal body variants, `Q_PROPERTY`,
    `Q_ENUM`/`Q_FLAG` and their `_NS` forms, `Q_ENUMS`/`Q_FLAGS`,
    `Q_CLASSINFO`, `Q_INTERFACES`, `Q_PLUGIN_METADATA`,
    `Q_DISABLE_COPY`/`Q_DISABLE_MOVE`/`Q_DISABLE_COPY_MOVE`,
    `Q_GLOBAL_STATIC`(+`_WITH_ARGS`), `Q_COREAPP_STARTUP_FUNC`,
    `Q_IMPORT_PLUGIN`, the `Q_DECLARE_*` family, and the `QML_*` macros
    that expand to declarations). Optional arguments accept either a real
    `argument_list` or the new `macro_argument_list` token soup, which
    covers keyword-bag argument syntax like
    `Q_PROPERTY(int value READ value WRITE setValue NOTIFY changed)`.
  - `qt_section_label`: `signals`, `slots`, `Q_SIGNALS`, `Q_SLOTS`
    followed by `:` inside a class body, with an optional preceding
    `access_specifier`, covering `signals:`, `public slots:` and
    `private Q_SLOTS:`.
  - `emit_statement`: `emit` or `Q_EMIT` followed by an expression
    statement inside a compound statement, at `prec(-1)` so it is only a
    rescue. `emit` is a documented Qt-reserved word; as with every listed
    literal, an identifier spelled `emit` can no longer be an expression.
  - `qt_statement_macro`: Qt statement macros conventionally written
    without a trailing semicolon (`Q_UNUSED`, `Q_ASSERT`/`Q_ASSERT_X`,
    `Q_VERIFY`/`Q_VERIFY_X`, `Q_CHECK_PTR`, `Q_UNREACHABLE`,
    `Q_FALLTHROUGH`, `QT_REQUIRE_VERSION`), in
    `_non_case_statement` with `optional(';')`.
  - `qt_foreach_statement`: `Q_FOREACH`, `BOOST_FOREACH`, and `foreach`
    own the following body statement. A new iterator has a `declarator`
    field. An existing variable has a `qt_foreach_assignment` node with
    a `left` field, so the adapter records its implicit write.
    `qt_foreach_body` begins after the container expression. The adapter
    registers the new iterator shadow only inside that iteration scope.
  - The strict `_macro_declaration_top` variant is what use sites
    actually reference (aliased back to `macro_declaration`):
    `_top_level_item` and the base's `preprocIf('', ...)` item choice
    each alias it at `prec.dynamic(-2)`, and this grammar's
    `declaration_list` override repeats
    `choice($._block_item, alias(..._macro_declaration_top...))` so
    namespace-scope `KDBINDINGS_DEFINE_*` macros parse. `_block_item`
    itself carries no macro fallback: inside real function bodies
    `NAME(args)` must always be a `call_expression` — when the fallback
    lived there it both stole call sites and, while it shared
    `argument_list` with `call_expression`, collapsed the merged
    `(`-list states into ERROR on `NAME(CALL(args))`. All macro arg
    slots are `macro_argument_list` only, never `argument_list`.
    `_non_case_statement` (inherited from the base, so the cpp additions
    keep it via `original`) carries the strict `_macro_declaration_stmt`
    variant — `identifier + macro_argument_list` with no `;` — for
    `NAME(args)` items in statement position that no statement rule can
    accept, e.g. `FLATBUFFERS_GEN_TYPES(TD)` in a switch body.
    `NAME(args);` still resolves as a call unless a real macro item
    appeared earlier in the block (then the `;` lands as an empty
    `expression_statement`).
  - The Qt member-function modifiers (`Q_INVOKABLE`, `Q_SIGNAL`,
    `Q_SLOT`, `Q_REQUIRED_RESULT`, `Q_NORETURN`, `Q_CONSTEXPR`,
    `Q_ALWAYS_INLINE`, `Q_NEVER_INLINE`, `Q_PACKED`, `Q_DECL_EXPORT`,
    `Q_DECL_IMPORT`, `Q_DECL_OVERRIDE`, `Q_DECL_FINAL`,
    `Q_DECL_CONSTEXPR`, `Q_DECL_RELAXED_CONSTEXPR`, `Q_DECL_DEPRECATED`,
    `Q_DECL_NOTHROW`, `Q_DECL_UNUSED`, plus argument forms
    `Q_REVISION(n)` and `Q_DECL_DEPRECATED_X(x)`) join `macro_attribute`.
  - `_class_declaration_item` accepts `repeat(choice($.macro_attribute,
    $.macro_qualifier))` before the item, and the `enum_specifier` name
    slot accepts `repeat($.macro_qualifier)`, so `class FOO_API Bar` and
    `enum class MY_API E` parse with any export macro spelling.
- Generic macro fallbacks for names no literal list can enumerate
  (`ASIO_DECL`, `FMT_API`, `MYLIB_EXPORT` and friends). The base C
  grammar carries two `prec.dynamic(-1)` rules this grammar inherits:
  `macro_qualifier` (a bare `identifier` with an optional argument list,
  usable only in the pre-type position of `_declaration_specifiers` and
  before a struct/union/enum/class name — post-type an identifier is a
  declarator, and allowing it there mis-parsed `int name : 1` and
  `bool f(T a, T* b)`) and `macro_declaration` (a bare `identifier` with
  an optional argument list and optional `;`, catching items no other
  rule accepts). In this grammar `macro_declaration` is a
  `prec.right(choice(...))`: the literal Qt/UE list above, or the
  `prec.dynamic(-1)` generic fallback. The shared `macro_argument_list`
  token soup also accepts the operator characters real macro arguments
  carry (`=`, `!`, `?`, `:`, `.`, `->`, `~`, arithmetic and comparison
  operators).
- Calling-convention and pointer-model macros inside declarators
  (`void* __cdecl memset(...)`, `VOID(NTAPI* FN)(...)`, `fd_set FAR*`,
  `int WINAPI WinMain(...)`, `_In_`-style SAL annotations in parameter
  lists). The shared base's declarator machinery gains:
  `repeat(choice($.ms_call_modifier, $.far_ptr_modifier,
  $.macro_attribute))` before `'*'`, a merged modifier repeat after
  `'*'` (literal-only in `abstract_pointer_declarator`, plus
  `macro_qualifier` in the concrete pointer rules), and
  `repeat(choice($.ms_call_modifier, $.macro_attribute))` before the inner
  declarator of the parenthesized-declarator rules and in the
  `declaration`/`function_definition` pre-declarator slots. Both slots are
  deliberately literal-only: a generic identifier pre-`'*'` kept the `[]`
  of `delete [] p` ambiguous against `lambda_capture_specifier` long
  enough to misparse the whole expression, and a generic identifier
  pre-declarator let `f(...)` fork a `macro_qualifier` GLR branch whose
  argument-list interpretations multiplied versions until the real
  `parameter_list` path was pruned — `void f(T<A,U<B>,C> x) {}` produced a
  `MISSING` declarator at `)`. A generic post-`'*'` identifier in
  `abstract_pointer_declarator` stole the declarator name of `(*cmp)`
  function pointers — `abstract_parenthesized_declarator` (prec 1)
  out-scores `parenthesized_declarator` (prec -10) — so the abstract
  rule alone keeps the post-`'*'` repeat literal-only while
  `*RESTRICTED_POINTER x` still parses via the concrete rules.
  WINAPI/NTAPI/APIENTRY still parse: they are `macro_attribute` literals. `far_ptr_modifier` is a new
  literal list (`FAR`, `NEAR`, `_far`, `_near`, `__far`, `__near`,
  `__huge`, `_huge`); WINAPI/NTAPI/APIENTRY already live in
  `macro_attribute`, so they are reused, not duplicated into
  `ms_call_modifier`.
- Win32 SDK headers (ntdll.h family, all through the shared C base):
  `_type_definition_type`'s pre-type qualifier slot takes a bare
  `_typedef_macro_qualifier` (identifier, no argument list) so
  `typedef CONST char *PCSZ` parses while `typedef VOID (*P)(T)` keeps
  `(*P)` out of the qualifier's argument list; `type_definition` gains a
  full no-declarator alternate for `typedef enum X {...};` under `#if`;
  `midl_attribute_list` leads `field_declaration` (in this grammar) and
  `parameter_declaration` (inherited) and stands alone in
  `_field_declaration_list_item` for the `#ifdef MIDL_PASS` bracket
  pattern; `struct_specifier`'s named branch accepts a `': public Base'`
  clause for `#ifdef __cplusplus` shared headers; `_argument_list_real`
  gains `$._argument_list_type` and `$._argument_list_operator` (dropping
  the now-redundant bare `$.primitive_type`), and the base's
  `_argument_list_type` carries `prec.dynamic(-1)` so `int (*F)(int)`
  keeps the declarator read over `call(*F, (int))`.
- `declaration`'s declarator seq gains a leading literal-only
  `repeat(choice($.ms_call_modifier, $.macro_attribute))` — the C base
  carries it inside `_declaration_declarator`, which this grammar
  replaces with plain `_declarator`, so `T __fastcall F(args);` (a
  calling-convention keyword on its own line between specifiers and
  name) parsed as ERROR. `_declarator` stays: the generic
  `macro_qualifier` must NOT join the leading repeat (an identifier
  there is the declarator).
- `_field_declaration_list_item` gains a dedicated `MACRO(TYPE)
  declarator;` alternate (aliased to `field_declaration`) for
  `WOW64_POINTER(PVOID) Reserved1[54]` members. This is a whole-item
  alternate, not a specifier variant: `macro_type_specifier` inside
  `_declaration_specifiers` regressed `= delete`/`= default` methods
  to MISSING identifiers when tried.
- `_macro_declaration_top` allows a bare identifier with no argument
  list and no `;` (at `prec.dynamic(-1)` so a real declaration still
  wins): namespace sentinels like `NLOHMANN_JSON_NAMESPACE_END` used to
  become an unterminated `expression_statement` that swallowed the
  following preproc block into ERRORs.
- `macro_namespace` pairs `NLOHMANN_JSON_NAMESPACE_BEGIN`/`END`,
  `QT_BEGIN/END_NAMESPACE`, `ABSL_NAMESPACE_BEGIN`/`END`,
  `PROTOBUF_NAMESPACE_OPEN`/`CLOSE` and `GTEST_NAMESPACE_BEGIN`/`END`
  at top level (dynamic +10), so the phantom-open braces inside these
  macros stop producing MISSING `}` errors on every real close brace
  downstream.
- `guarded_function` pairs a `#if`/`#else`-split function signature
  (specifiers + declarator, no `;`, inside a new
  `preproc_*_in_guarded_signature` block) with the shared
  `{` body that follows `#endif` — the nlohmann `operator<=>` vs
  `operator<` idiom. `preprocIf`/`preprocessor` are inlined at the
  bottom of this file because the node_modules C base exports no
  helpers.
- `guarded_class` does the same for a `#if`/`#else`-split class head
  (`_guarded_class_head`: class/struct/union keyword, attributes, macro
  qualifiers, name, optional `final`, optional `base_class_clause`,
  inside `preproc_*_in_guarded_class` blocks) paired with the shared
  `field_declaration_list` + `;` — the kddockwidgets
  `#ifdef PYTHON_BINDINGS class X : public A #else class X : public B
  #endif { }` idiom, where the two arms differ only in the base list.
  It is registered in both `_top_level_item` and `_block_item` so the
  form works inside namespace bodies (`declaration_list` items are
  `_block_item`s) as well as at translation-unit level.
- New conflicts for the Qt rules and the generic fallbacks:
  `argument_list`/`sized_type_specifier`/`expression`/
  `concatenated_string`/`_string`/`template_type`/`template_function`
  with `macro_argument_list`, `template_type` alone, and the verbatim
  conflict sets `tree-sitter generate` reported for the generic rules
  (`macro_declaration`/`macro_qualifier` against `_declarator`,
  `type_specifier`, `expression`, `_class_name`,
  `_top_level_expression_statement`, `_block_item`, `_non_case_statement`,
  `_field_declaration_list_item`, `parameter_list`,
  `_declaration_modifiers`, `statement`, `attributed_statement` and
  `preproc_*` in the combinations the generator asked for). Every set
  was added exactly as reported — a declared superset does not resolve a
  reported subset — and `prec.dynamic(-1)` on the generic rules keeps
  real reads preferred, so no clean-input parse changes.

- C-base drift picked up with the vendored `node_modules/tree-sitter-c`
  (and `sloplint-c-base.patch` regenerated): `__fp16`/`__bf16` out of
  `primitive_type` (user `struct __fp16` in ncnn headers), `'__packed'`
  in the CMSIS `macro_attribute` group, `__ASM` as a `gnu_asm_expression`
  keyword, the `_in_declaration_specifiers` preprocIf suite (MiG
  `#ifdef mig_external`/`extern` arms), and bare `NAME` marker arms via
  `_macro_declaration_top` with an optional tail.

## Constructs verified fixed

Each was a parse error with the published crate and is valid for g++ and
clang++ with `-std=c++23`.

- `#if __has_include(<endian.h>)`, with quotes, after `defined`, with `||`
  and `&&`, in `#elif`; `__has_attribute(x)`; `__has_cpp_attribute(ns::x)`;
- C header shapes shared with the C grammar: `__ASM volatile ("..." : : :
  "memory")`, `__packed struct S { ... } v;`, `#ifdef GUARD`/`NAME`/`#endif`
  marker arms, `#ifdef`-switched declaration specifiers
  (`mig_external`/`extern`), `struct __fp16 { ... };` and `__fp16 h;`.
  `__has_embed("x")`; `__has_warning("-Wx")`
- `enum : unsigned short {A=2};`, `enum : int {B};`
- `struct S{unsigned:0; int x;};`, `struct T{int a:3; int :5; int b:2;};`
- `struct S{int m:3{0};};`, `struct U{int a:4{1}; int b:2{}; int c:3 = 2;};`
- `const std::string& s(v.get_str());`, `int *p(v.get());` (also in blocks
  and after commas)
- `template class C<int>;`, `extern template class C<long>;`,
  `template struct C<char>;`
- `x.operator==(y);`, `x->operator()(1);`, `config->*field;`,
  `this->Base::operator=(o);`, `x.operator()<int>(3);`
- `void f(int x={}){}`, `void h(P p = {1, 2}, int y = {3}) {}`
- `new int*[n];`, `new int*[n]{};`, `new const char*[n];`
- `bool operator==(A a,B b)=delete;`, in-class `= delete` members, deleted
  and defaulted friends
- `g(typename S::First());`, `auto x = typename S::First();`,
  `auto x = typename S::First{};`
- `__IO uint32_t ISER[1];`, `__STATIC_INLINE int clip(int x) {...}`,
  `VKAPI_ATTR VkBool32 VKAPI_CALL cb(void *);`, `int WINAPI wWinMain(...);`,
  `NAPI_CDECL f(int);`, `CALLBACK int handler(void);`
- `class Counter : public QObject { Q_OBJECT ... };` with
  `Q_PROPERTY(int v READ v WRITE setV NOTIFY changed)`, `Q_ENUMS(P)`,
  `Q_DECLARE_METATYPE(T)` members; `signals:`/`public slots:`/
  `private Q_SLOTS:` sections; `Q_INVOKABLE`/`Q_REVISION(2)`/
  `Q_REQUIRED_RESULT` member modifiers; `emit sig(x);`/`Q_EMIT done();`;
  `QT_BEGIN_NAMESPACE`/`QT_END_NAMESPACE` at top level;
  `GENERATED_BODY()`, `Q_GLOBAL_STATIC(T, x)`, `Q_COREAPP_STARTUP_FUNC(f)`,
  `Q_PLUGIN_METADATA(IID "x" FILE "y")`, `Q_INTERFACES(I)`,
  `Q_DISABLE_COPY_MOVE(C)`.
- `Q_DECLARE_METATYPE(const C*)` inside `#ifndef` header guards and
  inside function-level `#ifdef` blocks (the `_block_item` fix).
- `Q_UNUSED(x)` without `;` and `Q_ASSERT(p)` inside function bodies.
- `Q_FOREACH` with new declarations or existing variables, braced and
  unbraced bodies, empty bodies, and an enclosing `if`/`else`.
- `T __fastcall F (args);` / `T __stdcall F(args);` (calling-convention
  keyword between specifiers and name); `WOW64_POINTER(PVOID) X[54];`
  members (`MACRO(TYPE) declarator`); `typedef CONST char *PCSZ;`;
  `typedef VOID(NTAPI *F)(IN PVOID X);`; MIDL `[size_is(N)]`/`[default]`/
  `[case(A,B)]` brackets on fields and parameters and alone inside
  `#ifdef MIDL_PASS` guards; a declaratorless `typedef enum X {...};`
  under `#if`; `struct X : public Y` under `#ifdef __cplusplus`. The
  22,396-line `ntdll.h` Win32 SDK header parses with zero errors.
- Member pointers in type position: `TypeMarker<Return (C::*)(Args...)>`,
  `int (A::B::*)(int)` as template arguments, `using P = int (X::*)(int);`.
  The scope-prefixed `*` lives only inside
  `abstract_parenthesized_declarator` — putting it on
  `_abstract_declarator` made `delete [] p` ambiguous with a lambda
  capture list. Named member-pointer declarators also parse:
  `int C::* p;` worked already via `qualified_identifier`, and
  `typedef void (C::*sb)(int);` works because `_type_declarator` gained
  `qualified_identifier` and `_scope_resolution` accepts a plain
  `_type_identifier` as its scope (needed because `dummy` inside the
  parens lexes as `type_identifier`, not `identifier`). Trailing
  qualifiers on the typedef also parse (`typedef void (C::*f)() const;`,
  `noexcept`, `&`): `function_type_declarator` is overridden to take the
  full `_function_declarator_seq` (parameters + cv/ref/noexcept/trailing
  return) — the C base's version ends at the parameter list. A parameter
  list may also take a parenthesized
  declarator (`T (&arr)[N]`, `T (*arr)[N]`) — the pre-type qualifier in
  a parameter is restricted to a bare identifier (`_parameter_specifiers`),
  so the `(` is never read as a macro argument list; `decltype` accepts a
  comma expression (`decltype(a, void())`).
- Relational `>=` inside template arguments: `enable_if_t<sizeof...(Ts) >= 1>`,
  `IC<int, sizeof(int) >= 4>`, `W<3 >= 2>`. The closing angle is
  `token(prec(1, '>'))`, which outranks `>=` at lex time and splits it into
  `>` `=`. `template_argument_list` gained a second alternative whose final
  argument is `_template_relational_expression` closed by a further `>`.
  `token.immediate('=')` confines the rejoin to a literally adjacent `>=`,
  so `W<3> b = c` keeps the close read, and a declared
  `_template_argument`/`_template_relational_argument` conflict lets GLR
  compare both readings. The seed `_template_relational_argument`
  (`expression`, `>` `=` rejoined as one `>=` operator leaf, operand)
  emits a `binary_expression` with real `left`/`operator`/`right` fields,
  and its operand is `_template_relational_operand` — unary-or-tighter
  expressions plus binary chains whose operators outrank relational
  (`* / % + - << >>`). Looser tails continue through the left-recursive
  `_template_relational_binary`/`_conditional`/`_assignment` variants, so
  `W<3 >= 2 && 0>` groups as `(3 >= 2) && 0` and `W<a >= b ? x : y>`
  emits a real `conditional_expression` — matching C++ grouping exactly.
  Residual: a `>=` argument only works in final position
  (`X<a>=b, c>` still errors), and a spaced `3 > 2` still reads as
  the close angle — matching C++, which rejects `W<3 > 2>` too.
- Initializer value and the declaration's `;` carried inside preproc
  arms: `T x =\n#if A v1;\n#else v2;\n#endif`. `declaration` gained a
  second alternative ending in `_init_declarator_preproc` (aliased to
  `init_declarator`), whose value is `preproc_if`/`preproc_ifdef`
  `_in_init_declarator` — a new `preprocIf` suite whose arm items are
  `expression ';'`, `preproc_def`, `preproc_call`, `preproc_null`. The
  declarator is restricted to non-function forms (identifier,
  qualified, pointer, reference, array, parenthesized): a `= #if`
  initializer is never a function, and allowing function declarators
  collided with guarded signatures and `= delete`/`= default`.

## Constructs known still failing

None of the assigned C++ rows. These adjacent inputs stay errors, and both
compilers reject them too: `enum : int;` without a body, `__has_include`
outside a preprocessing directive, `new (int*)[n]` (array bound after a
parenthesized type id). `T &s(x);` still reads as a function declaration,
as before; only a member access, a call, or a non-type argument in the
parens steers it to an initialized declaration. 

## Corpus results

- Upstream `tree-sitter test` at tag v0.23.4, before: 179 of 179 pass. The
  tag `src/` is byte-identical to the published crate (sha256 of
  `parser.c`, `scanner.c`, `grammar.json`, `node-types.json` all match), so
  the tag corpus result is the published result.
- After the patch: 179 of 179 upstream cases pass.
- The original 36 cases in `test/corpus/sloplint_*.txt` contain 32 repros
  and 4 controls. The published parser fails those 32 repros and passes
  the 4 controls.
- This vendor dir ships only the `sloplint_*.txt` corpus files (upstream
  corpus is not vendored): 19 files, 92 cases, all passing with the patch
  (`tree-sitter test` reports 92/92). The latest additions cover the
  Win32 SDK cluster (typedef qualifier macros, function-pointer typedefs
  with SAL-annotated parameters, MIDL brackets, declaratorless typedefs
  inside `#if`, `#ifdef`-guarded constructs), bare macro sentinels before
  preproc blocks, `NS_BEGIN`/`NS_END` namespace-macro pairs, parenthesized
  declarator parameters, `decltype` comma expressions, member-pointer
  function types `T (C::*)(Args...)`, named member-pointer typedefs, and
  `#if`/`#else`-split class heads (`guarded_class`).

## Sweep result

Tree comparison between the published parser and this one on 31253 `.cc`,
`.cpp`, `.cxx`, `.hpp`, `.hh` and `.h` files from the cargo registry, the
Go module cache, a Python site-packages tree and `~/compiler`: files with
an error node 15539 before, 15220 after. 319 files changed from error to
clean. No file went from clean to error. The trees of 5 clean files
changed; each was a silent misparse that now matches the compiler:
`new HandleImpl*[n]{}` was a multiply with a lambda, `f(A<B> c = {})` was
a variable init instead of a defaulted parameter, and `D* p(D::Create())`
was a multiply instead of an initialized declaration. sloplint facts on
those 5 files gain the now-visible calls and bindings and lose nothing.

## Size

`src/parser.c`: 192,883,783 bytes (published: 17,299,078). States: 29043
(published: 8637). Language version: 14 (published: 14). The literal
enumeration of `macro_attribute` costs most of the size growth.

- **MSVC `__asm {}` / `_asm {}` blocks** inherit from the patched C base:
  statement-level `ms_asm_statement` → `ms_asm_block` token soup (no
  trailing `;`). See `../tree-sitter-c/SLOPLINT.md` for the rule shape
  and the `__asm int 3` single-instruction known gap.

- **Qt `SIGNAL(...)`/`SLOT(...)` string-based connect macros** parse as
  `qt_signature_call` (a `call_expression` alternative): inside them a
  `qt_signature` is `name(type, type, ...)` where each
  `qt_signature_type` is a specifier plus optional declarator, so
  bare-type arguments like `const char*` or `int` (not valid
  expressions) parse. Both `SIGNAL`/`SLOT` names are literal tokens,
  same trade-off as `qt_statement_macro` literals.

- **Preproc blocks splitting comma lists** (`parameter_list`,
  `field_initializer_list`, and the base's `initializer_list`) model
  each list item as `choice(real, real·blocks·[real], blocks·[real])`
  inside `commaSep1`, where a real item may follow a block comma-free
  because the separator comma often hides inside the block. Between two
  real items commas stay strict (`T<A, U<B>, C>` stays type-argument).
  The base's `argument_list` is instead comma-required between real
  items (see the C base notes): `"a" "b"` is forced through
  `concatenated_string` instead of splitting into two arguments.
  `parameter_list`/`field_initializer_list` use handwritten
  `preproc_*_in_*_list` rules (the C base's `preprocIf` factory is
  module-local, so variants are spelled out with literal
  `alias(token(/#[ \t]*ifdef/), '#ifdef')` directives);
  `initializer_list` inherits the base's `preprocIf('_in_*')` rules.
  Earlier designs that put a second permissive shape beside the list,
  or `optional(',')` between items, were reverted: they split
  multi-token parameters and flipped template arguments to identifiers.

- **Commas are loose atoms inside the blocks themselves**: the
  `_in_*_list_item` content is `repeat(choice(item, ','))`, so a `,`
  never demands a following item before `#endif`. Earlier per-item
  `optional(',')` edges and strict separators committed the `,` to a
  required continuation and produced `MISSING identifier`.

- **Infix continuations** (`a | b #ifdef | c #endif`):
  `_in_argument_list_item` and the base's `_in_initializer_list`
  content accept `seq(op, expression)` items covering the C++ operator
  spellings (`.*`, `->*`, `<=>`, `or`, `and`, `bitor`, `xor`,
  `bitand`, `not_eq`, and the shared set). The overridden
  `binary_expression` gains the same `seq(left,
  repeat1($._infix_continuation_preproc))` fallback row as the C
  grammar, reusing the argument-list preproc rules via the same alias —
  separate `_in_infix` rules were tried and removed because the
  deterministic `#ifdef` merge stole list blocks.

- **`concatenated_string`** accepts `identifier` (macro names) at
  either edge of the literal chain — `"lit" MACRO`, `MACRO "lit"`, and
  interleaved forms — plus `raw_string_literal` anywhere in the chain.
  The generic `call_expression + call_expression` arm was removed:
  with it present, three or more consecutive `NAME(ident)` lines at
  namespace/file scope (the nghttp2/kdbindings/7zip declaration-macro
  run idiom — 61+ corpus files) always read as a string concat plus a
  MISSING `;` instead of a run of `macro_declaration`s. The trade-off
  is that adjacent call-with-args concatenation (`XSTR(A) XSTR(B)`)
  no longer parses; the parse-rescue pipeline still analyses the
  surrounding code. Restricting the arm to known stringify-macro names
  was tried and reverted: a literal `'XSTR'` lexes as a keyword and
  shadows the identifier in every expression position.

- **Statement fragments inside preproc blocks** — inherited from the
  base: `dangling_else` (cpp override carries `constexpr` +
  `condition_clause`) and `dangling_label` cover `} else #endif { }`
  and `#ifdef X fin: #endif stmt`.

- **A whole `#if`/`#ifdef` group as an unbraced statement body** —
  `if (x) #ifdef MAC a(); #else b(); #endif` (also `while`/`do`/`for`/
  `for_range_loop` bodies, `else` consequences and labels). The base
  defines `_statement_or_preproc = choice($.statement, $.preproc_if,
  $.preproc_ifdef)`; the cpp overrides (`if_statement`, `dangling_else`,
  `while_statement`, `for_range_loop`) point their body slots at it.
  Deliberately NOT added to `_non_case_statement`: a `preproc_if` is
  already a direct `_block_item`, so adding it inside `statement` would
  make every block-level `#ifdef` ambiguous (block-item read vs
  statement-wrapped read). The body slots are unambiguous positions.

- **Preproc inside expressions** — the base's mid-split rows on
  `binary_expression` are mirrored for the cpp-only operators; the
  overridden `assignment_expression` and `_in_argument_list_item` carry
  the same `repeat1($._infix_continuation_preproc)` / trailing
  `expr op` shapes. Known gap: `TYPE MACRO(args) name` declarations
  stay unparseable (declarator/qualifier ambiguity needs lexer help).

- **Inherited from `sloplint-c-base.patch`**: comments inside a
  `\`-continued `#define`/`#undef`/`#call` resume `preproc_arg` (the
  `value`/`argument` field is `repeat(choice($.preproc_arg,
  $.comment))`), and `macro_argument_list` accepts real expressions
  (`$.pointer_expression`, `$.unary_expression`, `$.sizeof_expression`),
  assignment/suffix operators, `[`/`]`, `{`/`}` (brace-init args such
  as `M(name, T{})` / `M({a, b})` — the KDBindings `make_node` idiom),
  and nested `$.macro_argument_list`. The same brace atoms are added
  to `vendor/tree-sitter-c`'s own `macro_argument_list`.

- **Access-modifier macros** (`JSON_PRIVATE_UNLESS_TESTED:`-style names
  in `private:`/`protected:` position) parse as `macro_access_label` in
  `_class_member`, ahead of the bitfield reading.

- **Nested calls inside macro arguments** (`M(to_string())`) parse: a
  `macro_argument_list` may recursively contain another
  `macro_argument_list` after a `(`.

- **Free-atom `enumerator_list`** — inherited from the base patch:
  `repeat(choice($.enumerator, $.call_expression, '__extension__',
  preproc_if/ifdef aliases, $.preproc_def, $.preproc_call, ','))`.
  X-macro enum tables (`{ #define ET(E) E, GEN(ET) #undef ET }`) parse.
- **Type spellings as call arguments** — `_argument_list_real` gains
  `$._argument_list_type` (the base's strict type sequence) and
  `$._argument_list_operator`, so macro-style calls taking a type or a
  bare comparison token parse as real `call_expression` trees:
  `GET(uint8_t)`, `SWAP(unsigned int, *a, *b)`, `assert_int(0, ==, rv)`.
  `$.type_identifier` stays out to keep `f(x)` unambiguous.

  - `_argument_list_operator` is a second new `_argument_list_real`
    member: the bare comparison tokens `==`, `!=`, `<=`, `>=`, `<`, `>`
    as items, for assertion macros that pass the operator as a token
    (`assert_int(0, ==, rv)` — munit/CMocka idiom). No expression can
    start with these tokens, so argument position stays unambiguous.
- **`macro_qualifier`/`macro_attribute` in `_function_postfix`** —
  `repeat1(choice($.virtual_specifier, $.macro_attribute,
  $._macro_qualifier_bare))`. The qualifier slot takes the base
  grammar's bare-identifier form, not the arg-carrying
  `macro_qualifier`: with args allowed, `M(f3) M(f4) M(f5)` read as
  one function declarator followed by qualifier macros, starving the
  macro-declaration read. The base's `_macro_qualifier_bare` carries
  `prec.dynamic(-1)` and must not be redefined here without it — a
  zero-precedence version wins against `type + identifier` reads
  inside `_parameter_specifiers` (`Foo bar` params degrade to
  `macro_qualifier + type_identifier`).
- **`macro_declaration` args are `macro_argument_list` only** —
  `optional($.macro_argument_list)` in both the literal-name and
  generic fallback forms (and in `qt_statement_macro`). Sharing
  `argument_list` with `call_expression` made `identifier + arglist`
  token-identical to a call and collapsed merged `(`-list parse
  states, ERRORing every in-body `NAME(CALL(args))`; the permissive
  soup covers the same inputs without sharing states.
- **`type_qualifier` no longer inherits C's `noreturn`** — C++ has no
  `noreturn` keyword (`[[noreturn]]` is an attribute), so the rule is a
  full replacement list without `'noreturn'`; `auto noreturn = ...`
  parses and `noreturn void die();` still parses as a two-specifier
  declaration.
- **Trailing macros after a variable declarator** — `declaration`'s
  declarator seq gains `repeat(choice($.ms_call_modifier,
  $.macro_attribute, $.macro_qualifier))` (inherited `init_declarator`
  carries the same repeat), covering `} var SEC(".maps");`.
- **`__extension__` inside preproc blocks** — inherited: the
  `preprocIf('')` block content and the `_in_enumerator_list*` contents
  accept a bare `'__extension__'` atom.
- **Known remaining gaps** — `T* f(T (&arr)[N])` inside a template
  still loses to the `init_declarator` reinterpretation
  (`f = T(T(&arr))[N]`); `return x\n#endif\n? a : b;` expressions
  split across `#if` branches still error; a handful of generated
  headers carry a lone stray-brace ERROR that parse-rescue excises;
  `XSTR(A) XSTR(B)`-style adjacent call concatenation is a documented
  loss (see `concatenated_string` above); bare `>=`/`>` inside
  template arguments (`enable_if_t<sizeof(T) >= 1>`) stays
  unparseable — `token(prec(1, '>'))` splits the operator at lex time
  and a context-free fix needs an external scanner.

## Packed generated parser

The generated `src/parser.c` (~193MB) exceeds GitHub's 100MB blob limit,
so the committed artifact is `src/parser.c.gz` (`gzip -9`, ~13MB) and
`src/parser.c` is gitignored. `bindings/rust/build.rs` compiles
`src/parser.c` when present (a fresh `tree-sitter generate` output) and
otherwise decompresses `src/parser.c.gz` into `OUT_DIR` with the `flate2`
build-dependency. After every regen, refresh the packed copy:

```
gzip -9 -c src/parser.c > src/parser.c.gz
```

A `src/parser.c` left behind by an earlier `tree-sitter generate` run is
gitignored, so pulling a regen does not remove it, and the build still
compiles the leftover instead of the packed copy. The leftover keeps its
old language version, so it reproduces the mismatch the regen fixed.
After pulling a regen you did not run yourself, delete the leftover
first with `rm -f src/parser.c`.

- **`#` lines with no directive name** — inherited from the C base via
  `sloplint-c-base.patch`: `preproc_null` covers the null directive and the
  `# comment` banner idiom (heavy in boost `.hpp`/`.ipp` headers — the
  largest single parse-error class in the corpus recount). Same rule and
  rationale as in the C grammar's SLOPLINT.md.
- **Operator tokens as preproc call arguments** — `preproc_argument_list`
  gains the same comparison-operator item arm as the C grammar:
  `BOOST_WORKAROUND(BOOST_MSVC, < 1300)` and `CHECK(x, ==, y)`. The boost
  `.hpp` headers were the largest remaining parse-error class in the
  corpus recount.
- **Directive terminators tolerate trailing blanks** — inherited from the
  C base via `sloplint-c-base.patch`: the `token.immediate(/\r?\n/)` at
  the end of `preproc_include`/`preproc_def`/`preproc_function_def`/
  `preproc_call` widens to `/[ \t]*\r?\n/`, so a directive line ending in
  blanks no longer eats the following `#endif` as `preproc_arg`. Same
  rule and rationale as in the C grammar's SLOPLINT.md (the
  kddockwidgets_export.h whole-file ERROR class).
- **`number_literal` drops its leading sign** — same fix as the C grammar:
  `f(1)-1`, `f(1) -1` and `f(1)+1` lexed `-1`/`+1` as a signed literal in
  states shared with `macro_argument_list`, leaving the binary operator
  absent. This grammar defines its own `number_literal` (not inherited
  from the C base), so the change is applied here directly; the
  `sloplint-c-base.patch` copy carries it too for consistency. The
  `const sign = /[-\+]/` helper stays for the exponent tails. Signed
  operands parse through `unary_expression`.
- **GNU extended asm with fused `::` sections** — `asm(code ::: "memory")`
  fuses the operand-section separators into one `::` token, which the base
  grammar reads as three colons and fails on. `gnu_asm_expression` now also
  accepts `_gnu_asm_fused_sections`: `::` plus the four-section operand
  lists. `asm(s : :: : x)` still parses as classic sections (the fused arm
  requires `::` immediately, so the second `:` can't reach it). Repro:
  atmosphere's `kern_init_loader_board_default_setup.arch.arm64.cpp` and
  friends — verified with `g++ -fsyntax-only`.
- **`macro_braced_statement`** — `NAME { stmts };` statement-block macros
  (`ON_SCOPE_EXIT { VerifyAndClearTpidrEl1(...); };` in atmosphere, Qt
  `Q_FOREACH`-style bodies, `LOG { ... };`). The ambiguity against braced
  temporaries (`OStream{&buf}`, `Foo{a,b}`, `erase_fixture{}.m()`) is
  resolved by the `_macro_braced_open` external token in `src/scanner.c`:
  it claims `{` only when a `;` occurs inside the brace region — braced
  initializer lists never contain `;` — so `Type{args}` can never open a
  macro block. The name position accepts `identifier` and C++'s
  `type_identifier` (all-caps macro names like `ON_SCOPE_EXIT` lex as
  type tokens). Usable at top level and as an unbraced `if`/`for`
  consequence. `;` after the closing `}` is optional.
