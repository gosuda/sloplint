# tree-sitter-go, patched for sloplint

Date: 2026-10-20.

## Origin

- Upstream repo: https://github.com/tree-sitter/tree-sitter-go
- Upstream tag: v0.25.0 (commit 1547678a9da59885853f5f5cc8a99cc203fa2e2c)
- Published crate: tree-sitter-go 0.25.0. Name and version are unchanged.
- Files copied unchanged from the published crate: Cargo.toml, LICENSE, README.md,
  tree-sitter.json, bindings/rust/lib.rs, bindings/rust/build.rs, queries/*.
- Changed: grammar.js, src/ (regenerated), test/corpus/literals.txt (one expectation).
- Added: test/corpus/sloplint_*.txt, sloplint-grammar.patch, this file.
- The upstream test/corpus is vendored so `tree-sitter test` runs in this directory.

## How src/ is generated

- Tool: tree-sitter-cli 0.27.1.
- Command, run in this directory: `tree-sitter generate`
- JS runtime: node v26.12.0 (the default). `--js-runtime native` gives byte-identical
  output. Regenerating twice gave identical files.
- Regenerating the unpatched upstream grammar with 0.27.1 does not reproduce the published
  src/ byte for byte. The header comment differs, array.h differs, and node-types.json
  drops the `_type` supertype entry (`_type` is both a supertype and inlined, and 0.27.1
  warns and ignores the supertype). These differences exist without any sloplint patch.
- parser.c is 1703326 bytes (published: 1572685 bytes). STATE_COUNT is 1553 (was 1442).

## grammar.js changes

`sloplint-grammar.patch` is the unified diff against the v0.25.0 grammar.js.
Apply it from this directory with `patch -p1 < sloplint-grammar.patch` on the upstream file.

- G1, `new` with a value: `special_argument_list` takes `choice(prec.dynamic(2, _type),
  _expression)` as its first element. `new(42)` and `new(int64(-64))` parse. A type still
  wins when both readings fit, so `new(int)`, `new(a.B)` and `make([]int, n)` keep their
  shape. A shadowed `new` called like a function parses the same way.
- G2, generic method declaration: `method_declaration` gets an optional `type_parameters`
  field after the name. One new conflict: `[type_parameter_declaration, _simple_type]`.
- G3, type literal as method-expression base: `selector_expression.operand` also accepts
  `struct_type`, `interface_type`, and a parenthesized type made only of struct types,
  interface types, pointers to them, and parentheses. The parenthesized form is aliased to
  `parenthesized_type` and the pointer form to `pointer_type`. Named types stay
  expressions: `(*T).M` is still a `parenthesized_expression`. New hidden rules:
  `_parenthesized_type_literal`, `_type_literal`, `_pointer_type_literal`. One new
  conflict: `[_simple_type, _type_literal]`.
- G4, empty initializer: `if`, expression `switch`, and type `switch` accept an empty
  initializer: `if ; x {}`, `switch ; {}`.
- G4, empty statement: `statement_list` is rebuilt. An `empty_statement` needs no
  terminator after it, so `{ ; f() }` and `a();; b()` parse. New hidden rules:
  `_non_empty_statement`, `_last_statement`. `_statement` is now
  `choice(_non_empty_statement, empty_statement)`.
- G4, nested labels: `empty_labeled_statement` may contain another one, so `L:` newline
  `L1:` newline `}` parses as nested `labeled_statement` nodes.
- G5, single-line const group: `const_declaration` uses `optionalTrailingSep(const_spec,
  terminator)`, so the last spec may be followed directly by `)`.

Node kinds and fields read by crates/sloplint-syntax/src/lang/go.rs are unchanged:
`function_declaration`, `method_declaration` (fields name, parameters, result, body),
`statement_list`, `call_expression`, `selector_expression` (fields operand, field),
`if_statement` (fields condition, consequence, alternative), `var_declaration`,
`type_spec`, `composite_literal`, `default_case`. node-types.json gains the
`method_declaration.type_parameters` field and lists the statement kinds under
`statement_list` one by one instead of through `_statement`.

## Constructs verified fixed

Each repro is accepted by `gofmt` and by the Go 1.27.2 compiler.
`crates/sloplint-syntax/tests/grammar_gaps/go.rs` and `test/corpus/sloplint_*.txt` guard them.

- G1: `var _ = new(42)`, `var _ = new(int64(-64))`, `new(x + 1)`, `new(T{1})`, `new(f())`.
- G2: `func (O[T]) G[U any](u U) U { return u }`.
- G3: `var _ = (*struct{ J }).M`, `var _ = interface{ m1(string) }.m1`.
- G4: `if ; x {}`, `switch ; {}`, `func f() { ; println("x") }`, `L:` newline `L1:` newline `}`.
- G5: `const ( F = 1 )`.

### G2 check against the Go toolchain

- Installed toolchain: go1.27.2 linux/amd64.
- A throwaway module outside target/ with `go 1.27` in go.mod, holding
  `type O[T any] struct{}` and `func (O[T]) G[U any](u U) U { return u }`, builds with
  `go build ./...` (exit 0). A call `o.G[int](3)` also builds.
- The same module with `go 1.26` fails: `generic method requires go1.27 or later (-lang
  was set to go1.26; check go.mod)`.
- So generic methods are valid in the latest released Go, and G2 is patched.

## Behaviour changes to know about

- The upstream corpus case "String literals" in test/corpus/literals.txt holds a
  deliberately broken string literal. Error recovery now yields a lone `const_spec` for
  `e` plus an `ERROR` for the string content, where it used to yield one `ERROR` that also
  held `e`. Both trees contain an `ERROR`. sloplint reports `meta/parse-error` on the
  corpus input before and after; the reported position moves from line 8 column 3 to
  line 8 column 5. The vendored literals.txt expects the new tree. With the unmodified
  upstream literals.txt the patched grammar passes 66 of 67 cases.
- `new(nil[A, B])` and `make(true[A, B], ...)` no longer parse. They used to parse as a
  generic type named `nil` or `true`. `new` and `make` now accept an expression first, so
  `nil` and `true` lex as keywords there. No real Go source uses these names as types.
- `new(true)`, `new(false)`, `new(nil)` and `new(iota)` now yield a `true`, `false`, `nil`
  or `iota` node. They used to yield a `type_identifier`. The Go adapter reads neither.
- `;;;` runs yield one `empty_statement` per extra `;`. A trailing comment after a final
  empty statement now sits after the `statement_list`, not inside it.
- Real sloplint binaries, built from the unpatched and the patched grammar, run with
  `--no-cache --format json --fail-on never --rule meta/parse-error`. Go files that report
  `meta/parse-error`: 164 to 121 under ~/go/pkg/mod (75625 files), 148 to 118 under
  ~/compiler, 0 to 0 under ~/.cargo/registry. Compared as sorted path sets over all
  languages, no file reports a parse error with the patched grammar that did not report
  one before, and 43 and 30 Go files stop reporting.
- The same two binaries over 40000 generated expressions, compared with `go/parser`: 16
  inputs parse with the unpatched grammar and fail with the patched one. All are of the
  `nil[..]` or `true[..]` kind above. Over 30000 generated statement lists and 8000
  generated const, type and function groups, none parses with the unpatched grammar and
  fails with the patched one while `go/parser` accepts it.

## Error visibility regression for some invalid Go

sloplint reports a parse error only when its cursor walk finds an `ERROR` node or a
`MISSING` node (`first_error_pos` in crates/sloplint-syntax/src/lib.rs). tree-sitter can
insert a `MISSING` terminator token that has no visible node. In that case the tree has
`has_error()` true, `analyze` returns empty facts, and `parse_error` is `None`: no
`meta/parse-error` finding, no other findings. This is not new. The unpatched grammar is
silent on `const ( A = 1 B = 2 )` (measured with the unpatched binary). With the patched
grammar, a file that ends in `const ( A = 1 )` with no trailing newline takes the same path:
`has_error()` is true, `analyze` returns `parse_error` None and no functions.

The patched grammar moves some invalid inputs the other way. Measured with the real
binaries over 23 realistic invalid snippets, the unpatched binary reports 22 and the
patched one reports 22: `func f() { go f()L1: }` is reported before and silent now, and
`const ( A = 1 B = 2 )` is silent before and reported now. For `func f() { go f()L1: }`
the linked patched grammar gives `has_error()` true with a `(MISSING
"source_file_token1")` node, and `analyze` gives `parse_error` None and no functions.

Over the generated inputs, `go/parser` rejects 9509 inputs that the unpatched binary reports
and the patched binary parses without a finding: 6536 of 30000 statement lists, 2956 of
8000 const, type and function groups, and 17 of 40000 expressions. Twelve sampled
statement-list inputs are token-glued gibberish such as `go f()L1: }`. The remaining inputs
were not inspected one by one.

The fix belongs in `first_error_pos` or `analyze`: report a position when `has_error()` is
true but the walk finds nothing. It does not belong in this grammar.

## Known still failing

These are valid Go, or constructs near the fixed rows, that this grammar still rejects.

- `S[T1, T2]{}.n[T3, T4]` and `x.y.z[A, B]` without a call: instantiating a generic
  method as a value. Calls such as `o.G[int](3)` parse. Seen in loGos test/genmeth1.go.
  Not part of G2, which covers declarations only.
- `(make)([]int, 0)`: a parenthesized builtin called with a type argument. `(new)(T)`
  parses. Seen in x/tools testdata (static.go, mapsloop.go).
- Type literals other than struct and interface as method-expression base, such as
  `[]int.f` or `(*[3]int).f`. `go/parser` accepts them, but no such method exists, so no
  compiling program contains them.
- `L:;stmt` with a label, a `;` and a statement on one line. `gofmt` output puts the `;`
  on its own line, and that form parses.
- A label before `default:` or `case`, as in `case 1: L: default:`. The grammar accepts it
  and Go rejects it. Upstream already accepted labels at the end of a case body.
- Not fixed and not in scope: the other `go/parser`-accepted compile-error fixtures
  in the Go test suite (`[...]int` outside a literal, `x.(type)` outside a switch,
  `const _ int`, `new(int...)`).

## Upstream corpus (`tree-sitter test`, tree-sitter-cli 0.27.1)

- Before, unpatched v0.25.0: 67 cases, 67 pass.
- After, this directory: 80 cases, 80 pass. That is the 67 upstream cases (one expectation
  updated, see above) and 13 sloplint cases in test/corpus/sloplint_*.txt.
