# tree-sitter-odin (vendored by sloplint)

## Origin

- Published crate: `tree-sitter-odin` 1.3.0 (crates.io). Name and version are unchanged.
- Upstream: https://github.com/amaanq/tree-sitter-odin, tag `v1.3.0` (commit `e8adc739`).
- License: MIT (`LICENSE` is the upstream v1.3.0 file, fetched verbatim).
- Not shipped in the crate: the upstream `test/corpus`. The only corpus that runs here is `test/corpus/sloplint_*.txt`.
- Files kept from the crate: `Cargo.toml` (normalized by cargo), `README.md`, `bindings/`, `grammar.js` (patched), `queries/`, `src/` (regenerated), `tree-sitter.json`, `LICENSE`.
- `Cargo.toml` has one addition: an empty `[workspace]` table after `[lib]`. Without it `cargo fmt --all` reports the vendored package as belonging to the parent workspace and fails. No other manifest change.
- Files dropped: `Cargo.toml.orig`, `.cargo_vcs_info.json`, `.cargo-ok`.

## Why

The published grammar reports a parse error on valid Odin. sloplint skips a whole file on any error node, so each error hides the whole file. Against the Odin corpus (~14,400 files across the corpora repos) the published grammar errors on about 78 files; the patched grammar errors on about 56.

## Regeneration

`src/` is regenerated with tree-sitter-cli 0.24.7 (`tree-sitter generate`). `grammar.js` is the only patched input; `src/grammar.json`, `src/node-types.json`, and `src/parser.c` are regenerated artifacts and are not part of `sloplint-grammar.patch`.

## Changes (`grammar.js`, see `sloplint-grammar.patch`)

Each change is an Odin construct the published grammar rejects.

1. **Newline/`;` before `else`/`else if`/`else when`.** `if a do g()\nelse do h()`, `if a do g(); else do h()`, and `else do pos = 1;` at the end of a chain are legal Odin. The external scanner already suppresses the newline token when the next word is `else` (or `where`), so the clause rules only need to absorb a `;`: `else_if_clause`, `else_when_clause`, and `else_clause` take a leading `optional(';')`, and `prec.left` keeps a `;` after a clause on the statement list instead of inside the clause. `when_statement`/`else_when_clause` also accept the `do` consequence form upstream lacks.
2. **For-loop `init; names in expr`.** `for it := iter_make(&x); vo, _ in iterate(&it)` iterates with an initializer. The alternative requires at least two names before `in` so `for a in b` (an `in_expression` condition) stays unambiguous.
3. **Anonymous container literal in call args.** `struct { a: int }{1}`, `union{..}{..}`, `enum{..}`, and `bit_field{..}` in expression position now head a `struct` literal via the type-prefix choice.
4. **`#soa` collection types.** `soa_type` covers `#soa[]T`, `#soa[N]T` in type position; matching conflicts keep `#soa` literals working.
5. **Struct member defaults.** `struct { retries: int = 3 }` — `struct_member` accepts `= expression`; `field` accepts `= expression` too so `Cfg :: struct { retries: int = 3 }` in declaration position parses.
6. **Trailing comma in `@(...)` attribute lists.** `@(private, objc_implement, name = value,)`.
7. **`label: #partial switch`.** `label_statement` accepts `partial_switch_statement` (and `when_statement`) as the labeled body.
8. **`auto_cast`/`no_alias`/`no_broadcast` on defaulted parameters.** `proc(auto_cast first_vertex := 0)`.
9. **`#force_inline call` operand.** `tagged_call_expression` lets a directive lead a call in expression position, e.g. `loc := #force_inline gl.GetAttribLocation(p, n)`.
10. **Matrix literal keyed elements.** `matrix[4,4]int{4 ..< 8 = 1}` — elements accept `expression = expression`.
11. **`shebang`.** `#!/usr/bin/env -S odin-run` on line 1 of scripts parses as a `shebang` token.
12. **`tag_call` at declaration scope.** `#assert(expr)`/`#config(name, default)` file-scope directives parse as calls on a tag rather than erroring.
13. **`field` vs `struct_member` prefix.** `struct { a, b: T }` shares a prefix with member lists; a declared conflict resolves it.
14. **Literal directive tokens.** `alias('#partial', $.tag)` etc. accepted any `#word` token, so `#force_inline gl.g()` was swallowed by the `#partial` struct-literal prefix. All `alias('#X', $.tag)` sites are literal `'#X'` tokens now.
15. **`union` members take `named_type`.** `union { a: int, b: f32 }{a = 1}` — members are `choice($.named_type, $.type)`.
16. **Directive tag as expression operand.** `x := #procedure + " expr"` — `tag` is an `_expression_no_tag` alternative so directive constants compose with operators. Declared conflicts cover the statement-tag prefix cases this creates.
17. **`union` tag operand.** `U :: union #align(16) {..}` — `union_declaration` repeats `seq($.tag, optional($._tag_operand))` so a tag may carry a parenthesized operand.

## Known remaining gaps

About 55 corpus files still error. Known shapes: `"""` multiline strings (one repo), nested call-style polymorphic types like `Maybe(MessageHandler(Box))`, map literals `map[K]V{...}` with keyed elements, `tag` continuing into operators or member callees inside call arguments (`f(#procedure + s)`, `f(#force_inline gl.g())` — statement level works), `#soa^T`, `proc(...) -> T where ... ---`, and assorted single-file constructs. Fixing these is follow-up work.
