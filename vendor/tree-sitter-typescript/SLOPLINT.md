# sloplint vendored tree-sitter-typescript

This directory is the published crate tree-sitter-typescript 0.23.2 with a patched grammar.
The crate name and version are unchanged. It builds the typescript and tsx parsers.
The workspace uses it as a path dependency.
The grammar now extends the vendored `vendor/tree-sitter-javascript` (0.25.0 plus sloplint patches).
The published crate extended tree-sitter-javascript 0.23.1.

## Provenance

- Upstream repo: https://github.com/tree-sitter/tree-sitter-typescript
- Upstream tag: v0.23.2. Published crate version: 0.23.2.
- Date of the patch: 2026-10-20.
- Reference for the `using` work: upstream PR #357 (open draft, not merged).
  It bumps the JS grammar to 0.25. This change takes the same direction in fewer edits.
- Generator: tree-sitter-cli 0.27.1.
  Commands: `tree-sitter generate` in `vendor/tree-sitter-javascript`, then
  `tree-sitter generate` in `typescript/` and again in `tsx/`.
- Grammar files are ES modules (`import` and `export default`), converted from CommonJS.
  The CLI 0.27.1 loads them with Node v26.12.0 and with `--js-runtime native` (no Node).
  Both runtimes give a byte-identical `parser.c` for typescript.
  No older Node was tested.
- How `common/define-grammar.js` resolves the JS grammar:
  `import JavaScript from '../../tree-sitter-javascript/grammar.js'`.
  That is the sibling directory `vendor/tree-sitter-javascript`.
  The npm package `tree-sitter-javascript/grammar` is no longer used.
  No `node_modules` directory is needed.
- `src/` is regenerated for typescript and tsx. `scanner.c` and `common/scanner.h` are unchanged.
- `test/corpus/` holds the upstream corpus and the `sloplint_*.txt` guard files.
  `tsx/test/corpus/` holds guard files that only the tsx grammar can run (JSX).
- `sloplint-grammar.patch` is the unified diff of the three grammar files against the tag.

## Grammar changes

- `using` and `await using`: the hack `optional('using')` in `assignment_expression` is removed.
  Declarations now come from JS 0.25.0: `using_declaration` (field `kind`),
  `for (using r of xs)` and `for (await using r of xs)` (`for_in_statement`, field `kind`),
  and `for (using x = f();;)` (`for_statement.initializer`, a sloplint change in the JS grammar).
  The TypeScript `variable_declarator` is reused. `using f: Disposable = g();` parses.
  `using x = f();` is a declaration, not an assignment.
- Optional chaining: JS 0.25 reserves keywords everywhere except after `.` and `?.`.
  A literal `'?.'` in the TypeScript rules made tree-sitter keep `optional_chain` as a non-terminal.
  The state after `?.` then lost the "properties" reserved set, and `a?.delete(b)` failed.
  The three literal `'?.'` (call_expression and two type-query rules) are now
  `alias($.optional_chain, '?.')`. The tree is the same: an anonymous `?.` node.
- Variable named `of`: the TypeScript `variable_declarator` override lacked the JS
  `alias('of', $.identifier)`, so `for (var of;;) {}` failed. It is added back.
- `super` as a type-query object: `_type_query_member_expression` accepts `$.super`,
  as the JS `member_expression` does. `typeof super.foo` in a type parses again.
- One TypeScript adapter change in crates/sloplint-syntax/src/lang/typescript.rs:
  `exported_names` skips empty names. The grammar gives the anonymous keyword `default`
  no named node, so a bare `default` in an export list reads as an empty name and would
  otherwise poison the public fact of other declarations.
- Type queries: the property of `_type_query_member_expression` and of
  `_type_query_member_expression_in_type_annotation` uses `reserved('properties', ...)`.
  `import('x').default` and `typeof Map.prototype.delete` parse.
- Decorators: `decorator_parenthesized_expression` takes any expression (as PR #357 does).
  JS 0.25 reserves `super`, so the old identifier-only choice cannot parse `@(super.decorate)`.
  With forced rebuild (`tree-sitter test -r`) the unchanged upstream corpus gives 111 of 112 on
  JS 0.25.0. The upstream corpus case "Decorator with parenthesized expression" now expects `(super)`
  where it expected `(identifier)`. This is the only upstream corpus expectation that changed.
- Inherited from the vendored JS grammar: lenient escapes in templates, JSX reserved words,
  reserved words as export and import names, and the `await` precedence.

## Verified fixed

- `using x = f();`, `await using x = f();`, `using a = 1, b = 2;`, typed `using`.
- `for (using r of xs)`, `for (await using r of xs)`, `for await (await using r of xs)`.
- `for (using d1 = open(), d2 = null;;)`.
- Tagged templates with invalid escapes in typescript and tsx.
- Parity with the published grammar on keyword property names, type queries, JSX
  attributes and export names (all of them failed with plain JS 0.25.0 under TypeScript).
- Real corpora, parsed with `tree-sitter parse` using the published and the patched
  libraries, files with a parse error:
  50072 TypeScript files: 1986 before, 1890 after, 120 fixed, 24 new failures.
  All 24 are compiler test fixtures for invalid code or error recovery (tsc conformance
  fixtures that require an error, swc and oxc invalid-fixture files). No real project file regressed.
  1278 TSX files: 49 before, 49 after, 0 changes.
- Tree shape on 1160 sampled TypeScript files that parse in both: 0 differences.

## Known still failing

- `using` as a plain identifier at the start of a statement (`using = 1;`, `using(x);`).
  The published grammar fails on these too.
- Reserved words as type names, as in `class void {}` or `interface void {}`.
  They are invalid, but the published grammar accepted them.
- Flow annotations in `.js` files.

## Upstream `tree-sitter test` corpus

- All counts use `tree-sitter test -r` (forced rebuild; a cached library gave a false pass once).
- Before (published sources regenerated on JS 0.23.1 with CLI 0.27.1): 112 of 112 pass.
- On JS 0.25.0 with only the `using` hack removed: 111 of 112 pass (the decorator case fails).
- After: 124 of 124 pass in `typescript`. 112 upstream cases (one expectation changed, see above)
  and 12 sloplint cases.
- tsx: the upstream corpus run through the tsx grammar gives 48 of 48 before and after.
  The 3 sloplint tsx cases pass (run `tree-sitter test` inside `tsx/`).

## Regenerating

1. Regenerate `vendor/tree-sitter-javascript` if its `grammar.js` changed.
2. Run `tree-sitter generate` in `typescript/` and in `tsx/` with tree-sitter-cli 0.27.1.
3. Run `tree-sitter test` here and again inside `tsx/`.
