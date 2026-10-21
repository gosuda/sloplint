# SLOPLINT.md: vendored tree-sitter-php

## Provenance

- Upstream: https://github.com/tree-sitter/tree-sitter-php, tag `v0.25.1`
  (commit `58d5643086ae60e93a3746cca648363ee97c8761`).
- Published crate: `tree-sitter-php` 0.25.1 (name and version unchanged).
- Regenerated with the pinned CLI `tree-sitter` 0.27.1
  (`/home/alpha/lint/complexity-linter/.outline/tools/tree-sitter-cli/bin/tree-sitter`).
- Regen command, run in each dialect directory after editing `common/`:

  ```
  tree-sitter generate   # in php/
  tree-sitter generate   # in php_only/
  ```

- PHP validator: `php:8.5.8-cli` Docker image (`php -l`, PHP 8.5.8). An early
  exploration run used a PHP 8.5.11 image; every acceptance check used 8.5.8.

## Files

- Copied verbatim from the published crate: `Cargo.toml`, `Cargo.toml.orig`,
  `LICENSE`, `README.md`, `bindings/`, `common/` (except the two edits below),
  `php/grammar.js`, `php_only/grammar.js`, `queries/`, `tree-sitter.json`,
  `php/src/tree_sitter/`, `php_only/src/` (except the regenerated files below),
  and `php/src/scanner.c`, `php_only/src/scanner.c`.
- Not copied: `Cargo.lock` (the workspace owns the lock), `.cargo_vcs_info.json`,
  `.cargo-ok`.
- Changed by hand (both dialects share these):
  - `common/define-grammar.js`
  - `common/scanner.h`
- Regenerated, both dialects:
  - `php/src/grammar.json`, `php/src/node-types.json`, `php/src/parser.c`
  - `php_only/src/grammar.json`, `php_only/src/node-types.json`, `php_only/src/parser.c`
  - `node-types.json` lists the same 305 (type, named) entries as the published
    grammar, so no node kinds were added or removed.
- Added: `test/corpus/` (upstream v0.25.1 corpus, verbatim) and
  `test/corpus/sloplint_*.txt` (six files, seven cases), `sloplint-grammar.patch`
  (unified diff of the two hand-edited files against the published crate), and
  this file.

## Fixed shapes

Each fix was checked against `php -l` 8.5.8. Corpus cases are in
`test/corpus/sloplint_*.txt`; Rust tests are in
`crates/sloplint-syntax/tests/grammar_gaps/php.rs`.

- P1 scope qualifiers that are cast words and valid class names: `Binary::`,
  `Integer::`, `Double::` (the three named in the brief), plus `Boolean::` and
  `Real::`, which `php -l` also accepts and which share the same rule. Implemented
  as an alias of those keywords to `name` in `_scope_resolution_qualifier`.
  The other cast words stay reserved.
- P2 `clone $x ??= ...`: `clone_expression` accepts `augmented_assignment_expression`.
- P3 chained assignment through `clone`: `clone_expression` accepts
  `assignment_expression`.
- P4 trailing comma in group `use`: `namespace_use_group` accepts an optional
  trailing comma.
- P5 raw NUL byte inside single-quoted strings. Byte inspection of the two
  corpus files shows the failing bytes are a literal `0x00` (for example
  `'<NUL>foo'` in `HttpClientTraitTest.php` and `RequestTest.php`). The
  generated lexer treats NUL as end of input, so an external token
  `SINGLE_QUOTED_NUL` consumes exactly one NUL byte inside `string`.
  `\xHH` escapes in single quotes were already accepted by the published grammar
  (single-quoted backslashes are literal), so they need no fix and no test
  that fails on the published grammar.
- P6 keyword property inside double-quoted interpolation
  (`"$item->class"`, `"$item->function"`): `_simple_string_member_access_expression`
  uses `reserved('nothing', $.name)`, the same rule `_member_name` uses.
- P7 heredoc body containing a line `<?php` and a multi-line `/* ... */` comment,
  followed by more lines. The body's last newline was a second, ambiguous path
  (`choice` of `body _new_line` and `body`). The generated parser kept an "ended
  body" version alive, and a comment on that path survived past the body until
  the version cap pruned the correct one. Fix: the newline that ends a body is an
  external token `HEREDOC_FINAL_NEWLINE`, emitted only when the next non-blank
  line is the closing identifier. The body rule is now
  `optional(seq(value, _heredoc_final_newline))`, and the `[$.heredoc_body]`
  conflict is no longer needed.

## Behavior changes on previously clean files

Four published-clean files change tree shape. Every other clean file has the same
tree (see Verification). The four changes:

- `symfony/src/Symfony/Component/AssetMapper/Tests/Compiler/JavaScriptImportPathCompilerTest.php`:
  5 `comment` nodes inside heredoc bodies become 7 `string_content` nodes. Text such as
  `//# sourceMappingURL=...` is PHP string text, not a comment. Loses 5 comment facts.
- `symfony/src/Symfony/Component/AssetMapper/Tests/Compiler/SourceMappingUrlsCompilerTest.php`:
  8 `comment` nodes inside heredoc bodies become 8 `string_content` nodes. Same cause.
  Loses 8 comment facts.
- `symfony/src/Symfony/Component/String/Resources/WcswidthDataGenerator.php`:
  the `/* ... */` block inside the `<<<EOT` heredoc was one `comment` node. It becomes
  `string_content` nodes, and the interpolated `$version` and `$date` become
  `variable_name` nodes. PHP interpolates both in an unquoted heredoc, so the
  adapter now sees two variable reads in the function that returns the heredoc.
  Loses 1 comment fact and gains 2 variable-read facts (the adapter reads
  `variable_name` for reads at `crates/sloplint-syntax/src/lang/php.rs`
  lines 242, 314, 464, 479).
- `symfony/src/Symfony/Component/Yaml/Tests/ParserTest.php`: one `comment` node inside a
  heredoc body becomes one `string_content` node. Loses 1 comment fact.

Comment facts are therefore reduced by 15 across these four files, and two variable
read facts are added. No other clean file changes tree shape.

## Verification

- Upstream corpus, pinned CLI regen: 148/148 on the unmodified v0.25.1 grammar
  (regenerated with the same CLI) and on the patched grammar. The five upstream
  cases that the stale shipped `parser.c` in the clone failed are not a patch
  effect and are not claimed as fixes.
- Corpus with the six sloplint files: 155/155 (`tree-sitter test`, no update
  flag). Every new expected tree was checked for `ERROR` and `MISSING` nodes.
- Differential sweep over `.outline/corpora/laravel-framework` and
  `.outline/corpora/symfony` (14316 `.php` files), tree has any ERROR or MISSING node:
  - published grammar: 14 error files; vendored grammar: 3 error files.
  - clean to error (regressions): **0**.
  - error to clean (fixed): **11**, the files sloplint reports `meta/parse-error` on.
  - sloplint built from this tree reports `meta/parse-error` on those 11 files:
    **11 to 0**.
- Tree shape over every file: the 14302 files clean on the published grammar were
  compared by S-expression hash. Each has a matching hash except the four files
  listed above. Those four are the complete set of tree-shape changes.
- Rust tests (`grammar_gaps/php.rs`, 7 tests): green on the vendored grammar,
  red on the published crate (fail-proof 7/7). The controls (invalid inputs that
  must keep reporting `parse_error`) run after each fix assertion, so they are
  only exercised on the vendored grammar.
- Full `sloplint-syntax` test suite: all targets pass.

## Still failing

Remaining errors in the two corpora (3 files):

- `laravel-framework/tests/Foundation/Fixtures/bad-syntax-strategy.php`: invalid
  PHP (`php -l`: unmatched `)` on line 3). Intentionally bad fixture; skipped.
- `symfony/src/Symfony/Component/Config/Tests/Fixtures/ParseError.php`: invalid
  PHP (`php -l`: unclosed `{`). Intentionally bad fixture; skipped.
- `symfony/src/Symfony/Component/Cache/Traits/ValueWrapper.php`: valid PHP
  (`php -l` accepts it). The class name contains a raw non-UTF-8 byte (0xA9).
  Tree-sitter rejects it. sloplint skips non-UTF-8 files before parsing, so the
  analyzer never reaches the grammar for this file and it is not one of the 11
  `meta/parse-error` findings. Still a grammar gap, with no analyzer effect.

Outside the corpora, valid PHP that still errors (not in the brief's P1 list):

- Reserved-word class names in argument position: `foo(Int::FOO)`,
  `foo(String::class)`, `foo(Bool::FOO)`. `php -l` 8.5.8 accepts them. Statement
  level (`$x = Int::FOO;`) already parses. Only the five cast words above were added to P1.
