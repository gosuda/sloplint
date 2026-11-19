# tree-sitter-logos, vendored for sloplint

Date: 2026-11-18.

## Origin

- Base: `vendor/tree-sitter-go` in this repository, itself upstream
  tree-sitter-go v0.25.0 (commit 1547678a) plus the sloplint patches
  G1-G5 described in `vendor/tree-sitter-go/SLOPLINT.md`.
- This grammar renames the language to `logos` (`source.logos`, `.lgo`)
  and adds the LoGOs forms by extending the Go rules. Every Go rule the
  sloplint Go adapter reads is unchanged; see "Node compatibility".
- Syntax authority: `/home/alpha/repos-devtool/LoGOs/.outline/worktree/spira-toolchain-tip/src/cmd/compile/internal/syntax/`
  (files with `logos` in the name and the hooks in `parser.go`), then
  `logos-doc/SPEC.md` in that tree.
- `sloplint-grammar.patch` is the unified diff from the base above
  (`vendor/tree-sitter-go/grammar.js`) to this directory's `grammar.js`.
  Reproduce it with `diff -u --label=a/grammar.js --label=b/grammar.js`
  on the two files; apply it with `patch -p1` on the base file.

## How src/ is generated

- Tool: tree-sitter-cli 0.27.1 (`/tmp/tscli/package/tree-sitter`).
- Command, run in this directory: `tree-sitter generate`.
- `src/scanner.c` is hand-written: an external scanner for `$"..."`
  interpolated strings whose interior may nest quotes and braces; it
  skips whitespace itself because tree-sitter does not re-invoke an
  external scanner after internal extras.
- Regenerate after every `grammar.js` edit; `tree-sitter test` then must
  pass (`test/corpus/`).

## LoGOs forms in this grammar

Option types `T?`, `x?.f`, `a ?? b`; the `?` propagation operator
(`f()?`, `defer f()?`, `errdefer f()?`); tuple types `(A, B)` and tuple
literals `(a, b)`; struct type assertions `type S struct : I`; struct
field defaults (`Port int = 8080`); closed-union interface elements
(`~int | ~string`), property elements (`Name string`) and `const`
method/property elements; `const` view types (`const []int`); lambdas
`x => e`, `() => e`, `(a, b) => e`, `x => { ... }` lambdas; `switch`
expressions with `case v => e` value arms, `case T => e` type arms under
a type-switch guard `x.(type)`, `;`-separated arms, and `nil` or receive
bodies; named call arguments (`limit = 4`); `..expr` spread in
argument lists and composite literals; `errdefer`, `scope defer`,
`yield`; `go group (ctx, limit = N) { go { ... } }` and bare `go { }`
group children; `const if` / `const for` / `const switch`; `val.[f]`
field projections; `const func` declarations; `$"..."` interpolation.

Contextual words: `errdefer`, `yield`, `scope`, `group` stay ordinary
identifiers, so they also work as labels (`errdefer:`). The keyword
`map` is admitted as the name after `.` only (iterator chains,
`xs.map(f)`), matching `logosDotMap` in the parser source; other
keywords after `.` are errors, like Go.

`go group(args)` without a trailing block parses as an ordinary `go`
call of a function named `group`, the same degradation
`parser_group_logos.go` performs.

## Node compatibility

The sloplint LoGOs adapter (`crates/sloplint-syntax/src/lang/logos.rs`)
reuses the Go adapter's hooks and adds these node kinds to the role
table: `lambda_expression` (closure), `switch_expression`,
`const_switch_statement` (match), `const_if_statement` (if),
`const_for_statement` (loop), `yield_statement` (yield). Go rules keyed
on node names absent from LoGOs do not apply; see the LoGOs report.

Go rules that cannot apply to LoGOs, and why:

- `agent/catch-all-match`: returns early for Logos, as for Go; the Go
  hooks record no wildcard arms, and LoGOs type switches are exhaustive
  unions, not catch-alls.
- `agent/informative-assertions`: LoGOs has no `assert`; the arm exists
  only so `needs_message` keeps the Go (bool-literal) predicate.
- Lambda bodies carry no `sole_call`, so a sole-call lambda is not a
  `slop/passthrough-wrapper` candidate today.

## Corpora (tree-sitter-cli 0.27.1, this directory)

- `test/corpus`: 94 cases, 94 pass (the 80 vendored Go cases plus 14
  LoGOs cases in `test/corpus/logos_forms.txt`).
- LoGOs run corpus
  (`.../spira-toolchain-tip/test/logos/run/*.lgo`): 482 files, 482
  parse with no ERROR or MISSING node.
- spira (`git ls-files '*.lgo'`): 48 files, 48 parse clean; sloplint
  over them reports no `meta/parse-error`.

## Known limits

- The Go-side limits listed in `vendor/tree-sitter-go/SLOPLINT.md`
  ("Known still failing") carry over where the rules were not changed.
- The empty-body `switch x { }` tie prefers the statement form; a
  zero-arm switch expression in statement position is therefore parsed
  as `expression_switch_statement`.
- `queries/highlights.scm` and `queries/tags.scm` are editor queries
  only; sloplint loads no vendored query files.
