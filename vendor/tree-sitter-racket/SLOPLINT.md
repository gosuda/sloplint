# tree-sitter-racket, vendored for sloplint

Date: 2026-11-19.

## Origin

- Upstream repository: https://github.com/6cdh/tree-sitter-racket
- Upstream tag: `v0.25.0` (also HEAD `b3cbbde75ce2de3079e33c7fe90bd41b1d45be48`).
  `git log -1 --format='%H %ad' --date=iso` in this run:
  `b3cbbde75ce2de3079e33c7fe90bd41b1d45be48 2026-08-22 13:50:27 +0800`.
- Published packages: crates.io `tree-sitter-racket 0.25.0`
  (updated `2026-08-22T05:52:13Z`); npm `@6cdh/tree-sitter-racket`
  dist-tags `{'alpha': '0.24.7-1', 'latest': '0.25.0'}`,
  `0.25.0` published `2026-08-22T05:40:50.497Z`. The crates.io tarball
  was unreachable from this host, so this directory was copied from
  the GitHub tag `v0.25.0`, which is the published `0.25.0`
  (same version, same commit date). Name and version are unchanged.
- Licence: MIT (`LICENSE:1`: `MIT License`, `Copyright (c) 2022 6cdh`).
  Kept verbatim; no `NOTICE` file in the tree.
- `src/parser.c:17`: `#define LANGUAGE_VERSION 15`. The sloplint
  runtime (`tree-sitter 0.27.2`) loads ABI 15, so the committed
  `parser.c` is kept as generated. No regeneration was performed.
- External scanner: yes, C (`src/scanner.c`,
  `enum TokenType { HERE_STRING_BODY, ... }`,
  `void *tree_sitter_racket_external_scanner_create()`).
  Upstream README `News 0.3.0`: the external scanner was rewritten in C.
- `src/parser.c`: 404,149 bytes (`stat`).
- `grammar.js` has 37 rules; `src/node-types.json` has 59 entries,
  32 named. The grammar defines fields on no node kind (`fields: {}`).
- `tree-sitter.json`: grammar `racket`, scope `source.racket`,
  `file-types ["rkt"]`.
- `package.json` devDependencies: `tree-sitter-cli ^0.26.12`
  (current tree-sitter dependency 0.26.12, ABI 15, per README
  `Versioning`). Regeneration, if ever needed:
  `tree-sitter generate` with tree-sitter-cli 0.26.x or 0.27.x
  (both emit ABI 15), then `tree-sitter test` in this directory.

## Kept and dropped files

Kept from the tag: `LICENSE`, `README.md`, `grammar.js`,
`tree-sitter.json`, `Cargo.toml` (verbatim, no `[workspace]` table;
same as the bash/starlark vendors), `bindings/rust/{build.rs,lib.rs}`,
`queries/{highlights,locals,tags}.scm`,
`src/{parser.c,grammar.json,node-types.json,scanner.c,tree_sitter/}`,
`test/corpus/` (all twelve `.rkt` files plus `here_string/`).

Dropped (dev and multi-language packaging, not vendored by any
single-grammar crate here): `.clang-format`, `.clang-tidy`,
`.editorconfig`, `.gitattributes`, `.github/`, `.gitignore`,
`CMakeLists.txt`, `CONTRIBUTING.md`, `Cargo.lock`, `Makefile`,
`Package.swift`, `binding.gyp`, `bindings/node/`, `fuzztest/`,
`go.mod`, `nodes.md`, `package.json`, `package-lock.json`,
`pnpm-lock.yaml`, `pyproject.toml`, `setup.py`, `shell.nix`,
`test/scripts/`.

One local scanner change, recorded in `sloplint-grammar.patch`: here-string
lines compare with `memcmp` plus length (`strcmp` stopped at the embedded
NUL every `int32` element carries). The grammar is otherwise unchanged
(the committed parser already accepts every corpus case and every
sloplint fixture below). `bindings/rust/build.rs` is the upstream
file unchanged: it compiles `src/parser.c` and `src/scanner.c`
with `cc`, C11, and `-utf-8` on MSVC.

## Construct mapping (Racket forms onto linter concepts)

Racket with the default readtable is homoiconic: almost every form is
a generic `list` node, so the sloplint adapter classifies a `list` by
its head symbol through a data table (head text to concept), not by
node kind. One node kind carries one walker role, so `list` maps to
`Role::Function` and every other list-shaped construct is classified
by the table but realized only where its role is available. A head
with no table entry, and any definition-shaped list inside quoted or
commented datum (see "Non-code positions"), is transparent: the
walker still descends into it.

| Racket form | Linter concept | Realized as |
|---|---|---|
| `(define (f args ...) body ...)` | function-like unit | `Function` role; `function()` head with name `f` and `Param` rows |
| `(define f (lambda (args ...) body ...))`, same with `λ` | function-like unit | `Function` role; name `f`, params from the lambda list |
| `(define/contract (f args ...) ...)` and `(define/contract f (lambda ...))` | function-like unit | same as `define` |
| `(lambda (args ...) body ...)`, `(λ ...)` | closure | `Function` role; `function()` head with empty name, params from the list |
| `(let loop ([i 0]) body ...)` (named `let`) | loop / function-like unit | `Function` role; name is the loop label, no params (bindings are not parameters) |
| parameter: `x`, `[x default]`, `#:k k`, `(x . rest)` | parameter | `Param` rows: `has_default` for `[x default]` (`bool_default` when the default is `#t`/`#f`/`#true`/`#false`), `keyword_only` for `#:k`-marked names, `is_variadic` past the dot |
| `if`, `cond`, `case`, `match` arms, `when`, `unless` | branch | classified, unmapped (no `If`/`Match` role is available for `list`) |
| `for` family, `do` | loop | classified, unmapped (no `Loop` role is available for `list`) |
| recursion | self-reference through a `symbol`, not a node | unmapped |
| `list`, `vector`, `structure` bodies | block | transparent; scopes come only from function frames |
| `;`, `#\| … \|#`, `#;` | comment | `Comment` role on `comment`, `block_comment`, `sexp_comment` |
| `string`, `byte_string`, `here_string`, `regex`, `character`, numbers, `boolean`, `keyword`, `symbol` | string | leaves; no role (the walker has no string role) |
| `(f args ...)` calls | call | unmapped (the `list` role serves `Function`; `call()` returns `None`) |
| `require`, `provide`, `module`, `module+` | import | classified, unmapped (no `Import` role is available for `list`) |

Non-code positions: a definition-shaped list is ignored when any
ancestor is `quote`, `quasiquote`, `syntax`, `quasisyntax`,
`unquote`, `unquote_splicing`, `unsyntax`, `unsyntax_splicing`,
`sexp_comment`, `box` or `graph` (quoted, quasiquoted or
commented-out datum is not code). `vector`, `structure` and `hash`
stay transparent: a definition-shaped list inside them still reads
as a function (documented over-approximation; macro-shaped code in
containers keeps resolving).

Not in scope by grammar design (upstream README: "This grammar only
implements the Racket language with the default readtable" and
"There are no plans to add support for new language currently";
status: "complete and compatible with Racket 9.2"): `#lang` dialects
and reader extensions beyond the default readtable. `#lang` lines
themselves parse as `extension`/`lang_name` and stay transparent.

## Corpora (tree-sitter-cli 0.27.1, this directory)

- `tree-sitter test`: 24 cases, 24 pass, 0 fail (the twelve `.rkt`
  files plus `here_string/`, counted per assertion).
- sloplint fixtures (`crates/sloplint/tests/fixtures/*__racket/`):
  every fixture source parses with `root.has_error() == false` and
  no `MISSING`/`ERROR` in `to_sexp`; the broken-syntax negative
  control reports `meta/parse-error` as required.

## Known limits

- Branch and loop depth is not counted: `if`/`cond`/`match`/`for`
  lists are transparent to the complexity scorer, which only sees
  function nesting. A deeply nested `cond` scores like a flat one.
- No call facts: call-driven rules (log literals, dynamic lookups,
  assertions, sole-call wrappers) stay silent on Racket.
- No import facts: `require`/`provide` targets are not recorded, so
  import-driven rules stay silent.
- No type facts: `struct` shapes are not lowered to `TypeDef`.
- `#lang` dialects and reader extensions parse as generic
  `list`/`symbol` soup or `ERROR`, which bounds what the linter can
  claim about non-default readtables.
