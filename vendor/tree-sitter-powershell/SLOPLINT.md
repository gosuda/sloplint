# tree-sitter-powershell, patched for sloplint

Date: 2026-10-20.

## Origin

- Upstream repo: https://github.com/airbus-cert/tree-sitter-powershell
- Upstream tag: v0.26.4.
- Published crate: tree-sitter-powershell 0.26.4. Name and version are unchanged.
- The published grammar.js and scanner.c are byte-identical to the tag.
- The tag keeps grammar.js at the repo root, so the crate layout is the same.
- test/corpus is not in the published crate. It was copied from the tag.
- scanner.c is not changed.

## Why

sloplint skips a whole file when the parse tree has one ERROR or MISSING node.
Valid PowerShell that the stock grammar rejects therefore reports `meta/parse-error`
and loses every other finding for that file.

## Regeneration

- tree-sitter-cli 0.27.1.
- Command, run in this directory: `tree-sitter generate`. No flags.
- Regenerating the unpatched grammar.js with this CLI gives grammar.json and
  node-types.json identical to the published crate. parser.c differs in one line,
  the pragma `#pragma GCC optimize ("O0", "jump-tables")`.
- To redo the patch: apply sloplint-grammar.patch to the upstream grammar.js
  (`patch -p1`), then run `tree-sitter generate` and `tree-sitter test`.
- sloplint-grammar.patch is the unified diff of grammar.js against tag v0.26.4.

## Grammar diff

One bullet per construct.

- `program` takes optional named blocks (`begin`, `process`, `end`, `dynamicparam`)
  after the param block. It also accepts an empty body, so a script that holds only
  a param block, only a comment, or nothing parses. Reused node: `named_block_list`.
- `generic_token` has two more forms.
  - An option with an attached value: `-I=binary`, `--go_out=binary/proto`,
    `-buildmode=c-shared`, `--gtest_filter=$test`, `--format=zip`. The name must be
    followed by a character that cannot continue a parameter name. A bare `-Name` and
    `-Name:` stay `command_parameter`. Commas belong to the value, because
    about_Parsing says a native command gets `A,B` as one string.
  - A word that starts with a variable and continues as a path:
    `$env:TEMP\x.ps1`, `$BunBin\bun.exe`, `${env:ProgramFiles(x86)}\Tool\t.exe`.
    The first character after the variable name must be `\` or `/`.
- Comma lists in argument position may hold bare words.
  `Select-Object Name,Length` and `Select-Object Name, Length` are one
  `array_literal_expression` argument. The new hidden rules are aliased to
  `array_literal_expression`, so the node kind is unchanged.
  Lists of expressions parse to the same tree as before.
- A switch clause condition can no longer start with a double quote unless it is a
  complete quoted word. `"^(a)$"` was cut at the first parenthesis and lexed as a
  word. Quoted words without parentheses, braces or spaces keep their old shape.
  The hidden token is `_switch_clause_token`.
- Terminators between switch clauses are optional. PowerShell allows
  `switch ($x) { 1 { "a" } default { "b" } }`. A clause that started with a bare word
  after a clause on the same line was an error.
- Number prefixes, exponents and multiplier suffixes are case-insensitive:
  `1MB`, `10Gb`, `0XFF`, `1E3`, `5D`, `0x10L`. Before, `$n = 10MB` parsed as a
  command named `10MB` with no error, and `foo 5MB` as a generic word.
- A class method may chain a constructor: `Dog([string]$n) : base($n) {}` and
  `Dog() : this("x") {}`. Reused node: `argument_list`. New anonymous nodes: `base`, `this`.

## node-types.json changes

No kind or field was renamed or removed. Every change loosens a constraint or adds a child.

- `program` children are no longer required (`required` goes from true to false), and
  may be `named_block_list`.
- `array_literal_expression` children may include `generic_token`.
- `class_method_definition` children may include `argument_list`.
- New anonymous nodes `base` and `this`.

## Verified fixed

Each construct below was checked on real script text. The stock 0.26.4 reports an error
on every one. The vendored grammar reports none.

| Construct | Real script |
| --- | --- |
| named blocks after a param block, brace on the next line | ps-parser 1.1.0 `script.ps1` |
| `$env:TEMP\Setup-DevEnv.ps1` as an argument | cilium/ebpf v0.22.0 `scripts/windows/Setup.ps1` |
| `protoc -I=binary --go_out=binary/proto ...` | google/osv-scalibr `build_protos.ps1` |
| `&$exe --gtest_filter=$test > $log 2>&1` | rust-librocksdb-sys `build_tools/run_ci_db_test.ps1` |
| `go build -buildmode=c-shared -o ...` | googleapis/enterprise-certificate-proxy `build/scripts/windows_amd64.ps1` |
| `& cmake ... --format=zip`, `& tar ... --strip-components=1`, `-Value $BunBin\bun.exe` | bun `sign-windows-artifacts.ps1`, `bootstrap.ps1`, `install.ps1` |
| `[math]::Round($totalSize / 1MB, 1)` | node `pgo.ps1` |
| `Select-Object Name, Length`, `Dog(...) : base($name) {}` | oak-powershell 0.0.11 `tests/lexer/basic.ps1` |
| `nvidia-smi --query-gpu=a,b --format=csv,noheader,nounits` (the `2>$null` on that real line still lexes as a word, see below) | btcrs `profile-windows.ps1` |
| `Install-TeXUserResource tex/latex/qsharp qsharp.sty, qsharp.pdf` (bare-word list with a space) | texlive-doc qsharp `Install.ps1` |
| `Install-TeXUserResource tex/latex/revquantum revquantum.sty, revquantum.pdf` | texlive-doc revquantum `Install.ps1` |

Two spec rows differ from how the spec words them.

- The spec repro `"^(a)$"` alone parses in the stock grammar. The failing context is a
  switch clause condition: `switch -regex ($x) { "^(a)$" { 1 } }`. That is what the
  corpus and the Rust test use. The real source named in the spec, Conda.psm1, is not
  on this host.
- The spec repro `$env:TEMP\x.ps1` as a whole statement is not valid PowerShell. The
  tokenizer is in expression mode there. The valid shape is an argument, which is what
  the real script uses and what is fixed.

## Known still failing

- `$env:TEMP\x.ps1` as a statement. Invalid PowerShell, so it stays an error.
- `Write-Output $a+2` and other words where a variable is followed by an operator
  character other than `\` or `/`. about_Parsing says these are one argument.
- `foo $a.b\c`: member access in a word, then more characters. about_Parsing says the
  rest starts a new argument.
- `foo "$a"\b`: a quoted string followed by more characters.
- `foo --x=$(Get-Date)`: a subexpression inside an option value.
- Arguments separated by a tab character. `command_argument_sep` only knows spaces.
- `foo --%` with no text after the token.
- `foo -a,-b`.
- `foo -x="a b"` and `foo -x='a b'` parse without an error, but the value splits at the
  space into two words. PowerShell passes one argument.
- A parameter name that holds a digit or a non-letter, such as `-Count2` or `-m64`, is now
  a `generic_token` word and not a `command_parameter`. The stock grammar reported an error.
- `ps_shape` in the sloplint adapter counts a `generic_token` as a literal of its full
  length. A word such as `$BunBin\bun.exe` therefore looks like static text of 15 characters.
  This was already true for words like `v$VERSION.tar.gz`.
- Trailing comma `foo a,b,`. about_Parsing says trailing commas are not supported.
- `2>$null` and `>$null` with no space after the operator lex as one `generic_token`
  word, not as a redirection. No error is reported, and the stock grammar does the same.
  `2>&1` and `> $null` with a space are redirections. This is not fixed here.
- Templates with a `.ps1` name that are not PowerShell: btcrs libbitcoin
  `templates/gsl.build.ps1` (GSL) and virtualenv `activation/powershell/activate.ps1`
  (`__BIN_NAME__` placeholders).

## Upstream corpus (`tree-sitter test`)

- Stock grammar.js regenerated with 0.27.1: 139 of 139 pass.
- Patched: 139 of 139 upstream cases pass, and 21 new cases pass. 160 of 160.
- The new cases are in `test/corpus/sloplint_*.txt`. Each one is named `sloplint: ...`.
- The nvidia-smi corpus case leaves out `2>$null` on purpose. A tree that pins the
  redirect as a word would record the defect above as correct. The real line,
  with `2>$null`, is in the Rust test and is checked for no parse error.

## Shape check

Trees of 112 PowerShell files found on this host were compared before and after, with
the stock parser regenerated by the same CLI. 15 files had an error before and 2 after
(the two templates above). Every file that parsed before has the same tree after.
88 expression-heavy snippets, 11 switch snippets and 69 command snippets were
compared the same way, counting only snippets that parsed cleanly before. The only
trees that changed are six number cases (`10MB`, `0XFF`, `1E3`, `5D`, `foo 5MB`,
`-Value 1KB`). Before, they parsed as a command named `10MB`, as a generic word, or
as a different literal.

## sloplint sweep

Run with the stock and patched binaries over three roots, filtering to `meta/parse-error`.
Only files that sloplint walks are counted. It skips hidden directories and `build/`
paths, so bun and node scripts under `~/compiler/bamTiScript/.references` and the
enterprise-certificate-proxy `build/scripts/windows_amd64.ps1` were checked with
`tree-sitter parse` only, not with sloplint.

| Root | PowerShell parse errors before | after |
| --- | --- | --- |
| `~/.cargo/registry/src/...` | 3 | 0 |
| `~/go/pkg/mod` | 2 | 0 |
| `~/compiler` | 0 (no PowerShell file walked) | 0 |

Every other extension has the same count before and after.

## Rust regression test

`crates/sloplint-syntax/tests/grammar_gaps/powershell.rs`.
