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
  it, so it is taken from the upstream tag. `src/scanner.c` is unchanged.
- `sloplint-grammar.patch` is the diff of `grammar.js` against the upstream
  tag. It applies with `git apply` in a clone of tag v0.23.4. There are no
  scanner changes.

## Why

The published grammar reports a parse error on valid C++. sloplint skips a
whole file on any error node. These patches remove the errors that were
reproduced on real code. Every repro below was first accepted by
`g++ -fsyntax-only -std=c++23` and `clang++ -fsyntax-only`.

## Regenerating src/

- Tool: tree-sitter-cli 0.27.1.
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
  `tree-sitter generate`; it carries the `guarded_linkage` and
  `_linkage_guard` rules, their `_top_level_item`/`_block_item` members and
  the two conflicts. The same pair of conflicts is declared in this
  grammar's own `conflicts:` because the generator reads only the child's
  conflict list.
- Commands, run in this directory with that symlink in place:
  `tree-sitter generate`, then `tree-sitter test`.
- It rewrites `src/parser.c`, `src/grammar.json` and `src/node-types.json`.
  `src/scanner.c` is not generated and is untouched.
- Regenerating the unpatched grammar with 0.27.1 already moves the parser
  from ABI 14 to ABI 15. No node kind and no field is added or removed by
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
  - `macro_declaration` is also a member of `_block_item`, which is the
    repeated content of `preproc_if`/`preproc_ifdef` bodies at every
    level: without it every declaration macro inside a header guard or
    inside a function-level `#ifdef` dropped to an ERROR parsed as a
    `function_declarator` missing its `;`.
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
  $.macro_attribute))` before `'*'`, a merged literal-only modifier
  repeat after `'*'` (ms_pointer_modifier, type_qualifier,
  ms_call_modifier, far_ptr_modifier, macro_attribute), and
  `repeat(choice($.ms_call_modifier, $.macro_attribute))` before the inner
  declarator of the parenthesized-declarator rules and in the
  `declaration`/`function_definition` pre-declarator slots. Both slots are
  deliberately literal-only: a generic identifier pre-`'*'` kept the `[]`
  of `delete [] p` ambiguous against `lambda_capture_specifier` long
  enough to misparse the whole expression, and a generic identifier
  pre-declarator let `f(...)` fork a `macro_qualifier` GLR branch whose
  argument-list interpretations multiplied versions until the real
  `parameter_list` path was pruned — `void f(T<A,U<B>,C> x) {}` produced a
  `MISSING` declarator at `)`. A generic post-`'*'` identifier is always
  the declarator name (`(*cmp)` function pointers): letting
  `macro_qualifier` eat it produced abstract nameless declarators.
  WINAPI/NTAPI/APIENTRY still parse: they are `macro_attribute` literals. `far_ptr_modifier` is a new
  literal list (`FAR`, `NEAR`, `_far`, `_near`, `__far`, `__near`,
  `__huge`, `_huge`); WINAPI/NTAPI/APIENTRY already live in
  `macro_attribute`, so they are reused, not duplicated into
  `ms_call_modifier`. Known loss: `typedef CONST STRING* X` (unknown
  macro + unknown type) stays an error — genuinely ambiguous without a
  preprocessor.
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

## Constructs verified fixed

Each was a parse error with the published crate and is valid for g++ and
clang++ with `-std=c++23`.

- `#if __has_include(<endian.h>)`, with quotes, after `defined`, with `||`
  and `&&`, in `#elif`; `__has_attribute(x)`; `__has_cpp_attribute(ns::x)`;
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
- `tree-sitter test --debug-build` passes all 41 local cases, including
  five foreach regressions for body ownership and iterator forms.
- Total with the patch: 220 of 220.

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

`src/parser.c`: 35,394,654 bytes (published: 17,299,078). States: 9417
(published: 8637). Language version: 15 (published: 14). The literal
enumeration of `macro_attribute` costs most of the size growth.
