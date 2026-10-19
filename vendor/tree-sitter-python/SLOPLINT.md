# tree-sitter-python, vendored for sloplint

Date: 2026-10-20.

## Origin

- Upstream repository: https://github.com/tree-sitter/tree-sitter-python
- Upstream tag: v0.25.0
- Published crate: tree-sitter-python 0.25.0 (crates.io). Name and version are unchanged.
- This directory is the published crate plus the patch below. `README.md`, `LICENSE`,
  `queries/`, `bindings/rust/`, `tree-sitter.json` and `Cargo.toml` are the published files.
  Cargo's `.cargo_vcs_info.json`, `Cargo.lock` and `Cargo.toml.orig` are not kept.
- `sloplint-grammar.patch` is the diff of `grammar.js` and `src/scanner.c` against the
  upstream tag. It applies with `git apply` in a clone of tag v0.25.0.
- tree-sitter-starlark is built on this grammar. It is a separate crate and was not touched.

## Why

The published grammar reports a parse error on valid Python. sloplint skips a whole file on
any error node. These patches remove the errors that were reproduced on real files and on
parser test fixtures, and that CPython accepts.

CPython versions used to check the repros: Python 3.15.0 (final, 2026-10-09) accepts every
repro below. Python 3.14.7 rejects three of them: `lazy import`, `{**x for x in y}` and
`case +1:`. Those three are 3.15 syntax, 3.15.0 is released, so they count as valid.

## Regenerating src/

- Tool: tree-sitter-cli 0.27.1, JavaScript runtime node v26.12.0 (the default).
- Command, run in this directory: `tree-sitter generate`
- It rewrites `src/parser.c`, `src/grammar.json`, `src/node-types.json` and
  `src/tree_sitter/*.h`. `src/scanner.c` is edited by hand and is not generated.
- Regenerating the unpatched grammar with 0.27.1 alone changes `node-types.json`: the hidden
  supertypes `_simple_statement` and `_compound_statement` leave the child lists of `module`
  and `block`, and the named alias `as_pattern_target` appears. It adds or removes no other
  kind and no field. This patch adds the kind `default_type_parameter`, the anonymous
  token `lazy` and the fields `parameter` and `default` of `default_type_parameter`.
- Run `tree-sitter test` in an upstream clone with the patched `grammar.js` and `src/`
  copied in and the `test/corpus/sloplint_*.txt` files added. Upstream does not ship them.
  The `tree-sitter` CLI caches the compiled parser per language name in
  `~/.cache/tree-sitter/lib`. Set `TREE_SITTER_LIBDIR` to a private directory per grammar
  version, or an old build is reused.

## Changes to grammar.js

- PEP 696 type parameter defaults: `type_parameter` accepts the new `default_type_parameter`
  node, with fields `parameter` and `default`. Both fields are `type`. It covers `T = int`,
  `T: int = str`, `T: (int, str) = int`, `*Ts = *tuple[int]` and `**P = [int, str]`, in
  `type`, `def` and `class` headers. A subscript such as `list[T = int]` in an annotation
  also parses now, because `generic_type` shares `type_parameter`. CPython rejects that
  one, as it is an invalid use of a default.
- Starred annotation with a parenthesized operand: `splat_type` accepts `*` plus a
  `parenthesized_expression` (`*args: *(int)`).
- Starred operand in a comma list: the `expression_list` elements and the elements of an
  `expression_statement` tuple may now be a starred bracket display, literal, `await` or
  unary operand. A hidden rule `_star_primary_splat` is aliased to `list_splat`, so the
  node kind is the existing one (`_ = *[x],`, `a, *[x]`, `*await y,`, `return *(x), y`,
  `for i in *{1}, 2`). The operands are the primary expressions that `list_splat_pattern`
  cannot take: parenthesized expression, tuple, generator, list, set, dictionary and the
  three comprehensions, `await`, unary operator, string, number, `True`, `False`, `None`
  and `...`. A starred name, attribute or subscript still parses as `list_splat_pattern`
  as before. An earlier attempt that took any `expression` after `*` made `x = a, *b` a
  `pattern_list`. It was dropped for that reason.
- Starred subscript operand: `subscript` accepts `list_splat` as a subscript element
  (`data[*(x := y)]`, `data[*[a]]`, `x[a, *(b)]`).
- PEP 810 `lazy import` (3.15): `import_statement` and `import_from_statement` start with an
  optional `lazy`. `keyword_identifier` accepts `lazy`, so `lazy = 1`, `lazy(x)`,
  `lazy.y` and `import lazy` still parse as before. The `lazy` token is anonymous, so
  the adapter, which reads named children only, does not see it.
- PEP 798 dictionary unpacking in a comprehension (3.15): the `body` field of
  `dictionary_comprehension` accepts `dictionary_splat` (`{**x for x in y}`).
  `{*x for x in y}`, `[*x for x in y]` and `(*x for x in y)` already parsed.
- Unary plus in a case pattern (3.15): the sign of a number in `_simple_pattern` and
  `complex_pattern` accepts `+` as well as `-` (`case +1:`, `case +1 + 2j:`,
  `case [+1, -2]:`, `case {+1: a}:`).

## Changes to src/scanner.c

- Backslash continuation at the start of a block line: CPython takes the indentation of a
  logical line from the whitespace before its first backslash. The scanner added the
  whitespace of the lines that the backslash joins in. The scanner now remembers the
  indentation seen before the first backslash on a line and uses it when that value is
  not 0 (`if True:` newline `    \` newline `        1` newline `    \` newline `2`
  newline `else:` newline `    3`). A backslash at column 0 keeps the old behavior.

## Constructs verified fixed

Each was a parse error with the published crate and is valid for CPython 3.15.0.

- `type X[T = int] = int`, `def f[T = int](x: T): ...`, `class C[T = int]: ...`
- `def f[*Ts = *tuple[int]](): ...`, `def f[**P = [int, str]](): ...`,
  `def f[T: int = str, U: (int, str) = int](): ...`
- `def foo(*args: *(int)): ...`
- `_ = *[x],` and `_ = a, *[x]` and `_ = *(x), y` and `for a in *[x], y: pass`
- `data[*(x := y)]`, `data[*[a]]`
- `async def g():` newline `    *await y,`
- `if True:` newline `    \` newline `        1` newline `    \` newline `2` newline
  `else:` newline `    3`
- `lazy import foo`, `lazy from foo import bar`, `lazy from . import x`
- `{**x for x in y}`
- `match foo:` newline `    case +1: ...`

## Constructs known still failing

These are valid for the CPython parser. They are not in the gap list, so they were not
widened.

- A starred bracket, literal or unary operand that goes on with an operator, a call or a
  subscript: `_ = *(a) + b,`, `_ = *-x + y,`, `_ = *(a)(x),`. The operand is taken as one
  atom. `_ = *[x][0],` and `_ = *[x].a,` still parse, through `list_splat_pattern`.
- A starred operand in parentheses as an assignment target: `*(a), b = c`.
- `*f(x)` in a comma list parses without error, but as a `call` whose `function` is a
  `list_splat` of `f`, and not as a `list_splat` of the call. This is unchanged from the
  published grammar. The adapter reads the callee text `*f` for it.

## Inputs that are not valid Python and so stay errors

- A single starred expression that is not a name, as a statement or as an assignment
  right-hand side: `*(a + 1)`, `_ = *[42]`, `_ = *{42}`, `_ = *list()`, `_ = *(p + q)`.
  CPython's parser accepts them and its compiler rejects them ("can't use starred
  expression here"). This grammar reports a parse error for them, as the published grammar
  does. With a comma they are a tuple display and parse: `*(a + 1),`, `_ = *[42],`.
- `f((a)=1`, `def f(:`, `x =`, `class`.
- `[**x for x in y]`, `{**x, 1: 2 for x in y}`.
- Python 2 syntax and the SageMath files `FR.<x> = GFR[]` in the Go module cache.

Inputs that CPython rejects only after parsing (a later pass reports them) parse without
an error here: a type parameter default that comes before a parameter without one,
duplicate type parameter names, `lazy import` inside a function, class or `try`,
`lazy from x import *`. `def f[T = *x]()` and `x: list[T = int]` also parse without an
error, although CPython rejects them while parsing: the type parameter default shares the
`type` rule, which already took a starred name. The published grammar accepted
`_ = *x` and `*x` as well.

## Corpus results

- Upstream `tree-sitter test` at tag v0.25.0, before: 117 of 117 pass.
- After the patch: 117 of 117 upstream cases pass.
- New cases in `test/corpus/sloplint_*.txt`: 23 cases (17 repros, 6 controls).
  The unpatched grammar fails 17 of the 23. The patched grammar passes all 23.
- Total with the patch: 140 of 140.

## Sweep result

Tree comparison between the published parser and this one on 5645 `.py` and `.pyi` files
from the cargo registry (1926), the Go module cache (46), the miniconda site-packages
(2730) and `~/compiler` (943): files with an error node 831 before, 751 after.

- Error to clean: 80 files. All are in ruff parser fixture crates (ruff_python_parser
  0.0.18, rustpython-ruff_python_parser 0.15.8 and 0.16.10: 28 distinct fixture names, 3
  versions each).
- Clean to error: 0 files.
- Clean to clean with a different tree: 0 files.
- Error to error with a different error tree: 40 files (error recovery shapes only).
- The 751 files that still fail: 2 are SageMath files in the Go module cache, 749 are in
  ruff parser fixture crates. Of those 749, CPython 3.15 `ast.parse` accepts 6 files (2
  distinct names, 3 versions each): `inline/err/assign_stmt_starred_expr_value.py` and
  `valid/expressions/starred.py`. Both hold the single starred expression statements
  listed above, which the compiler rejects.

## Size

`src/parser.c`: 3,612,962 bytes (published: 3,439,646). States: 2901 (published: 2786).
