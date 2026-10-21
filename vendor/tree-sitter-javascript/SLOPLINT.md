# sloplint vendored tree-sitter-javascript

This directory is the published crate tree-sitter-javascript 0.25.0 with a patched grammar.
The crate name and version are unchanged. The workspace uses it as a path dependency.

## Provenance

- Upstream repo: https://github.com/tree-sitter/tree-sitter-javascript
- Upstream tag: v0.25.0. Published crate version: 0.25.0.
- Date of the patch: 2026-10-20.
- Generator: tree-sitter-cli 0.27.1. Command, run in this directory: `tree-sitter generate`.
- `grammar.js` is now an ES module: `export default grammar({...})`. The CLI 0.27.1 loads it.
- `src/` is regenerated from `grammar.js`. `src/scanner.c` is unchanged.
- `test/corpus/` holds the upstream corpus and the `sloplint_*.txt` guard files.
- `sloplint-grammar.patch` is the unified diff of `grammar.js` against the upstream tag.

## Grammar changes

- Tagged templates: `template_string` uses the new token `_template_escape_sequence`.
  It is aliased to `escape_sequence`, so the tree shape is unchanged.
  The token accepts invalid escapes: `\x`, `\x4`, `\u`, `\u{`, `\u{12`, `\u12`.
  Valid escapes lex as before. Strings keep the strict `escape_sequence` token.
  The scanner is unchanged: template bodies still go through `_template_chars` first.
- JSX names: `_jsx_identifier` and the property of `nested_identifier` use
  `reserved('properties', ...)`. `<div class="x" for="y" />` and `<Foo.default />` parse.
  Version 0.25.0 failed on them.
- Export and import names: `_module_export_name` uses `reserved('properties', ...)`.
  `export { a as null }` and `export { x as function }` parse.
- `for (using x = f();;)`: `for_statement.initializer` accepts `using_declaration`.
- `await`: `await_expression` has `prec.dynamic(1, ...)`.
  Without it, `await (a ?? b)(c)` and `await (await fetch(u)).json()` parsed as calls
  of an identifier named `await`.
- `using_declaration` and the `using` for-heads already exist in 0.25.0. They are unchanged.

## Verified fixed

- ``String.raw`[0]\x` ``, ``String.raw`.\undefined` ``, and `\u` or `\u{` without a valid body.
- The same sources inside the TypeScript and TSX grammars (they share the rule).
- JSX attribute and member names that are reserved words.
- `await (expr)(args)` and `await (expr).method()` build an `await_expression`.
- Real corpora, published 0.25.0 against this grammar, parse errors:
  109003 JS files from compiler, go/pkg/mod, cargo registry and bun cache,
  23609 failing before and 23510 failing after. 0 new failures and 99 files fixed.

## Known still failing

- Flow type annotations in `.js` files. They are not JavaScript.
- `using` as a plain identifier at the start of a statement: `using = 1;`, `using(x);`,
  `using.y = 2;`, `using;`. Upstream 0.25.0 has the same gap.
- A call to a function named `await` in non-async code, as in `x = await(y)`.
  It now parses as an await expression, as it did before 0.25.0.
- Untagged templates also accept invalid escapes. The grammar cannot tell them apart.

## Upstream `tree-sitter test` corpus

- Before: 116 of 116 parses pass (published sources regenerated with CLI 0.27.1).
- After: 122 of 122 parses pass. 116 upstream cases and 6 sloplint cases.

## Regenerating

1. Edit `grammar.js`.
2. Run `tree-sitter generate` here with tree-sitter-cli 0.27.1.
3. Run `tree-sitter test`.
4. Rebuild `vendor/tree-sitter-typescript` (see its SLOPLINT.md). It reads this `grammar.js`.
