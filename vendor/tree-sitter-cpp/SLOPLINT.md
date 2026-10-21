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
  copied here. The C++ worker and the C worker patch only their own grammar.
- Commands, run in this directory with that symlink in place:
  `tree-sitter generate`, then `tree-sitter test`.
- It rewrites `src/parser.c`, `src/grammar.json` and `src/node-types.json`.
  `src/scanner.c` is not generated and is untouched.
- Regenerating the unpatched grammar with 0.27.1 already moves the parser
  from ABI 14 to ABI 15. No node kind and no field is added or removed by
  the patch. `node-types.json` keeps every type and every field.

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
- New cases in `test/corpus/sloplint_*.txt`: 21 cases (17 repros,
  4 controls). The published parser fails 17 of the 21 sources and passes
  the 4 controls. The patched grammar passes all 21.
- Total with the patch: 200 of 200.

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

`src/parser.c`: 17,722,475 bytes (published: 17,299,078). States: 8571
(published: 8637). Language version: 15 (published: 14).
