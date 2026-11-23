# SLOPLINT.md: vendored tree-sitter-php

## Provenance

- Upstream: https://github.com/tree-sitter/tree-sitter-php, tag `v0.25.2`
  (commit `c1e13490bd656b8869cd47e6f74997ed46750a08`, also the default-branch
  head; matches the published crate's `.cargo_vcs_info.json` sha1).
- Published crate: `tree-sitter-php` 0.25.2 (name and version unchanged).
- Regenerated with the pinned CLI `tree-sitter` 0.27.1.
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
- Added: `test/corpus/` (upstream v0.25.2 corpus, verbatim) and
  `test/corpus/sloplint_*.txt` (six files, seven cases), `sloplint-grammar.patch`
  (unified diff of the two hand-edited files against the pinned upstream tree), and
  this file.

## Fixed shapes

Each fix was checked against `php -l` 8.5.8. Corpus cases are in
`test/corpus/sloplint_*.txt`; Rust tests are in
`crates/sloplint-syntax/tests/grammar_gaps/php.rs`.

- P1-P3 are provided by upstream v0.25.2 and need no patch hunk here:
  upstream `cabf8bf` aliases the `CAST_TYPE_CLASS_NAMES` keywords
  (`binary`, `boolean`, `double`, `integer`, `real`) to `name` wherever a class
  name is expected (a superset of the old P1: it also covers argument position
  and typed const elements), and upstream `81ff3f1` lets `clone_expression`
  take a general `$.expression` (a superset of the old P2/P3). The other cast
  words stay reserved, in upstream as here. The old
  `test/corpus/sloplint_cast_keyword_class.txt` and
  `test/corpus/sloplint_clone_assignment.txt` cases are kept as regression
  guards; their trees are byte-identical to the ones the old patch produced.
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

- P8 cast words as a plain function name and as a constant subscript base
  (`int(1)`, `OBJECT[0]`): upstream `cabf8bf` aliases the
  `CAST_TYPE_CONSTANT_NAMES` spellings to `name` in `primary_expression`,
  but the function-name and subscript-base positions only accept `_name`,
  which covers just the class-name subset. The same alias is added to the
  `function_call_expression` function field and to
  `_dereferencable_expression`. Class declarations are untouched (no change
  to `_name` itself), and real casts keep their parentheses, so a
  `cast_expression` still parses as one (see the `cast_still_cast` gap test).

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
- Upstream corpus plus the seven sloplint files, pinned CLI regen: 172/172
  (`tree-sitter test`, no update flag). The upstream corpus grew by 15 cases
  since v0.25.1 (cast, clone, and match coverage); all pass.
- The 9 extracted sloplint inputs parse clean on both dialects (`php` and
  `php_only`), and the 7 carried-over parse trees are byte-identical to the
  ones the v0.25.1-based vendored tree produced, so no corpus rewrite was
  needed. The 2 new P8 inputs fail on the unpatched v0.25.2 tree.
- Rust tests (`grammar_gaps/php.rs`, 8 tests): green on the vendored grammar.
  P1-P3 now pass through upstream rules (kept as regression guards); P4-P8
  still fail on the unpatched v0.25.2 tree and pass with the reduced patch.
  The controls (invalid inputs that must keep reporting `parse_error`) run
  after each fix assertion, so they are only exercised on the vendored grammar.
- Full `grammar_gaps` suite (716 tests) and the whole fixture suite pass on
  the vendored grammar.
- Custom query migration: no node kind was added or removed, so existing
  custom queries still compile, but two tree shapes moved. A cast over an
  assignment was `(assignment_expression left: (cast_expression))` through
  the old `cast_variable` alias in `_variable`; it is now
  `(cast_expression value: (assignment_expression))`, because the cast value
  takes a general expression. And `(primary_expression (name))` now also
  matches the cast-word constant spellings (`int`, `string`, ...). Queries
  pinning either shape need the new nesting.
- The v0.25.1-base differential sweep over `.outline/corpora/laravel-framework`
  and `.outline/corpora/symfony` (14316 `.php` files: 14 error files published,
  3 vendored, 0 regressions, 11 fixed, `meta/parse-error` 11 to 0) is not
  re-run here — those corpora are not in this checkout. It is superseded by
  the checks above: the patch hunks kept here are byte-identical in
  behavior to the v0.25.1 re-vendor (same 7 trees, plus 2 new P8 inputs),
  and upstream's v0.25.1-to-v0.25.2 grammar diff touches none of the P4-P7
  rules; P8 is a new local hunk.

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

Outside the corpora, valid PHP that still errors:

- Reserved-word class names such as `foo(Int::FOO)`, `foo(String::class)`,
  `foo(Bool::FOO)`. `php -l` 8.5.8 accepts them, but upstream keeps
  `int`, `string`, `bool`, `float`, `object`, `array`, `void`, `mixed`,
  `iterable`, `never` and `unset` reserved as class names, in statement and
  argument position alike. No upstream issue covers them, and the analyzer
  never sees them as `parse-error` shapes distinct from the P1 words, so the
  patch does not extend the alias list.
