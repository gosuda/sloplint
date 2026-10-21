# tree-sitter-powershell, patched for sloplint

Date: 2026-10-20. Two rounds of fixes, both in this crate.

## Origin

- Upstream repo: https://github.com/airbus-cert/tree-sitter-powershell
- Upstream tag: v0.26.4.
- Published crate: tree-sitter-powershell 0.26.4. Name and version are unchanged.
- The published grammar.js and scanner.c are byte-identical to the tag.
- The tag keeps grammar.js at the repo root, so the crate layout is the same.
- test/corpus is not in the published crate. It was copied from the tag.
- scanner.c is changed in round 2 (see below). grammar.js is changed in both rounds.

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
- To redo the patch: apply sloplint-grammar.patch to the upstream tree with
  `patch -p1` (it holds the grammar.js and src/scanner.c diffs), then run
  `tree-sitter generate` and `tree-sitter test`. Regenerating from the committed
  grammar.js and scanner.c gives a byte-identical parser.c.
- Sizes: parser.c 5,629,881 bytes (published: 4,697,445), 2495 states (published: 2142).

## Round 1 grammar changes

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
- A switch clause condition can no longer start with a double quote unless it is a
  complete quoted word. `"^(a)$"` was cut at the first parenthesis. The hidden token
  is `_switch_clause_token`.
- Terminators between switch clauses are optional, so clauses may share a line.
- Number prefixes, exponents and multiplier suffixes are case-insensitive:
  `1MB`, `10Gb`, `0XFF`, `1E3`, `5D`. Before, `$n = 10MB` parsed as a command.
- A class method may chain a constructor: `Dog([string]$n) : base($n) {}`.

## Round 2 grammar and scanner changes

One bullet per cluster of the wave-2 sweep. The validator was `pwsh 7.6.7`
(`Parser.ParseFile` and `ParseInput`).

- PS-COMMA, comma lists of bare words, `Get-Module P, Pester | Remove-Module`:
  already handled by the round 1 list rule. Round 2 adds a regression guard for the
  Pester `param ([switch] $a, [switch] $b)` plus list shape.
- PS-VARPATH, `$var/path` arguments and `. $PSScriptRoot/x.ps1`: `/` is now a
  character of `path_command_name_token`, so a dot-sourced path with a slash parses.
  The argument form (`Import-Module $PSScriptRoot/bin/x.psd1`) was a round 1 fix.
- PS-NATIVE, `git -C $p log --format="%h %B" -n 1`, `& git -C $s apply -p2 ...`,
  `dotnet run -- ../x $y`:
  - A parameter name may hold digits after its first letter (`-p2`, `-m64`, `-O2`,
    `-Count2`). Before round 2 these were `generic_token` words. They are
    `command_parameter` now, as for `-Name`.
  - A command-line word may hold quoted segments that contain spaces, pipes or
    parentheses: `--format="%h %B"`, `--format='%H||%P'`. The segments are matched
    inside the word token. A double-quoted segment holds no nested quote.
    `-p:V="$($m.x)"` is a `-p` parameter, a `:` separator and then one word
    `V="$($m.x)"`, so the adapter sees two arguments, not one. pwsh passes one string
    to a native command; that shape is not modelled.
  - A lone `--` is a `command_parameter` after a separator, so `dotnet run -- x` parses.
  - A call in an argument may be followed by more member access and calls:
    `-Value $sbf.ToString().TrimEnd()`. The extra pieces are siblings of the argument,
    like the first `argument_list`.
- PS-EMPTYBLOCK, `{}` and `{ }`: a script block expression may be empty. This covers
  `Mock Remove-Item {}`, `@{ K = {} }`, and `[ScriptBlock] $MockWith = {}`.
- PS-PARAM, `param ([switch] $a, [switch] $b)` at file level: the program body is
  optional after the param block (round 1). In a function body the first statement
  may follow the param block on the same line: `function f { param($a) $a }`. The
  hidden rule `_param_then_body` is aliased to `script_block`. The tree has the same
  shape as a body with a line break after the param block: a `script_block` that holds
  a `param_block` and a `script_block_body`.
- PS-GENERIC, `Dictionary[[string], [int]]`: a generic argument may be wrapped in
  its own brackets, and array ranks may repeat (`int[][]`, `tuple[...] []`).
  `type_identifier` accepts nested-type `+` and generic-arity backtick parts
  (`System.Environment+SpecialFolder`, ``System.Func`1``).
- PS-FLAGS, `[Flags()] enum E {}`, and PS-UNKNOWNATTR, `[DscResource()] class C {}`,
  `[Foo()] class C {}`: `class_statement` and `enum_statement` take an optional
  `attribute_list`. Decision: pwsh's only complaint about `[Foo()] class Bar {}` is
  `CustomAttributeTypeNotFound` ("Cannot find the type for custom attribute"), which
  is semantic, so this is fixed. pwsh's complaint about a `[DscResource()]` class that
  has no `Set` method is `DscResourceMissingSetMethod`, also semantic (it needs the
  DSC module); a class with the three methods is clean in pwsh and parses here.
  The grammar is looser than pwsh in one way: a bare `[Flags]` before `enum` or
  `class` also parses, where pwsh reports `TypeNotAllowedBeforeStatement`.
- PS-WHERE, `$a.Where{ $_ }.Count`, `$a.ForEach{ ... }`, `$a::M{ ... }`: the external
  scanner lexes `.Name` or `::Name` as one token only when `{` follows the name at
  once. The node keeps the kind `invokation_foreach_expression` with a
  `script_block_expression` child, as the upstream `.foreach` rule had. This
  replaces the upstream `.foreach` token rule, whose longest match also swallowed
  the name in `$a.ForEach({ $_ })` (an error in the stock grammar).
- PS-CIN, `-cin`: `-cin`, `-iin`, `-cnotin` and `-inotin` complete the `-in` family
  of comparison operators.
- PS-TERNARY, `$a ? "x" : "y"`: new node `ternary_expression`. The condition is a
  `logical_expression`, each branch an expression or a nested ternary.

More valid code that the same sweep files turned up, fixed because it blocked files:

- `-bor`, `-band` and `-bxor` inside method arguments. Upstream repeated `-and`,
  `-or` and `-xor` in the bitwise argument rule.
- `break outer` and `continue top` take a bare name as a label.
- ForEach-Object, Do-Something and the other names that start with a statement
  keyword and a hyphen now lex as command names.
- Scanner: the end-of-statement token no longer ends a statement when the next line
  starts with `else` or `elseif`. Scanner: a backtick that ends a line is white space
  for the end-of-statement test.
- A text run in a double-quoted string has lexical precedence. A run that started at
  white space or `#` after a variable or an escape was read as a comment
  (`"`n## x"`, `"$a ## x"`). No scanner code is needed for this.
- Here-strings: a line that starts with a backtick pair keeps the pair.
  ``` ``````PowerShell ``` inside `@" "@` failed, and `` `$x `` at the start of a
  line produced a phantom `variable` node.

## node-types.json changes

No kind or field was renamed or removed. Every change loosens a constraint or adds a child.

- New named node: `ternary_expression`. It is a child wherever an expression is.
- New anonymous nodes: `base`, `this`, `?`, `-cin`, `-cnotin`, `-iin`, `-inotin`.
- `program` children are no longer required, and may be `named_block_list`.
- `array_literal_expression` children may include `generic_token`.
- `class_method_definition` children may include `argument_list`.
- `class_statement` and `enum_statement` may have an `attribute_list` child.
- `command_elements` may hold a `member_name` sibling after an argument list.
- `script_block_expression` children are no longer required: `{}` is valid.
- `label_expression` may hold a `simple_name`.
- `redirected_file_name` may hold a `command_parameter`.

## Shape changes in code that already parsed

The shape check below found trees that change for files the stock parser accepted.
Each one is a bug fix. They change facts, so they are listed.

- `else` or `elseif` on the line after `}` of an `if` that is the value of an
  assignment (`$count = if (...) { } NEWLINE else { }`, also inside `@{ }`). The stock
  grammar accepted it as a separate command named `else`; it is an `else_clause`
  now. This is most of the 22 changed trees that `grammar-diff` reports below
  (Pester `src/` and `tst/`, PSScriptAnalyzer `build.psm1`, `ModuleHelp.Tests.ps1`).
- File-level `end { }`, `BEGIN { }` and `process { }` after a param block. They
  parsed as commands with a script-block argument; they are `named_block_list` now.
- `$n = 10MB`, `0XFF`, `1E3`: a command or a generic word before, a number now.
- `-p2`, `-m64`: `generic_token` before, `command_parameter` now.
- A word with a quoted segment that holds a space (`--format="%h %B"`): two or three
  words before, one now.
- A here-string line that starts with `` `$ ``: a `variable` node before, none now.

## Verified fixed

Each construct was checked on real script text. The stock 0.26.4 reports an error on
the first six rows of round 1; round 2 rows are listed after.

| Construct | Real script |
| --- | --- |
| named blocks after a param block, brace on the next line | ps-parser 1.1.0 `script.ps1` |
| `$env:TEMP\Setup-DevEnv.ps1` as an argument | cilium/ebpf v0.22.0 `scripts/windows/Setup.ps1` |
| `protoc -I=binary --go_out=binary/proto ...` | google/osv-scalibr `build_protos.ps1` |
| `&$exe --gtest_filter=$test > $log 2>&1` | rust-librocksdb-sys `build_tools/run_ci_db_test.ps1` |
| `go build -buildmode=c-shared -o ...` | googleapis/enterprise-certificate-proxy `build/scripts/windows_amd64.ps1` |
| `& cmake ... --format=zip`, `-Value $BunBin\bun.exe` | bun `sign-windows-artifacts.ps1`, `install.ps1` |
| `[math]::Round($totalSize / 1MB, 1)` | node `pgo.ps1` |
| `Select-Object Name, Length`, `Dog(...) : base($name) {}` | oak-powershell 0.0.11 `tests/lexer/basic.ps1` |
| `Install-TeXUserResource tex/latex/qsharp qsharp.sty, qsharp.pdf` | texlive-doc qsharp `Install.ps1` |
| `Get-Module Pester.Runtime, Pester | Remove-Module`, `param ([switch] $PassThru, ...)` | Pester `tst/Pester.Mock.RSpec.ts.ps1` |
| `. $PSScriptRoot/doBuild.ps1` | PowerShell `test/tools/NamedPipeConnection/build.ps1` |
| `git -C $p log --format="%h %B" -n 1`, `-p2` | Pester `src/csharp/Sync-WithProfiler.ps1`, `Update-VendoredDiffPlex.ps1` |
| `dotnet run -- ../x $y` | PowerShell `build.psm1` |
| `Mock Remove-Item {}`, `@{ Value = {} }`, `$RootBlock.ScriptBlock = {}` | Pester `tst/`, `src/Pester.Runtime.ps1` |
| `function f { [CmdletBinding()] param ([string] $Name) 'real' }` | Pester `tst/Pester.CustomAssertion.ts.ps1` |
| `Dictionary[[string], [PkgVer[]] ]`, `tuple[ [tuple[...]] [] ]` | PowerShell `tools/UpdateDotnetRuntime.ps1`, `packaging.psm1` |
| `[Flags()] enum` | PSReadLine `test/Get-KeyInfoData.ps1` |
| `[DscResource()] class` | PSScriptAnalyzer `MyDscResource.psm1` |
| `.Where{$_ -eq $quote}.Count` | PSReadLine `SamplePSReadLineProfile.ps1` |
| `$r.Name -cin @(...)` | Humanizer `tools/docs/api-xml-input.ps1` |
| `$arm64 ? "a" : "b"` | PowerShell `tools/wix/wix.psm1` |
| `[A]::B -bor [A]::C` in method arguments | PSScriptAnalyzer `Tests/Engine/Settings.tests.ps1` |
| `$count = if (...) { } NEWLINE else { }` | Pester `src/functions/assertions/HaveCount.ps1` |
| backtick, blank line, `}` | Pester `src/Format.ps1` |
| `"`n## ... Conflicts Detected`n`n"` | PowerShell `tools/ci.psm1` |
| here-string with a line that starts with `` ` `` | PSScriptAnalyzer `Utils/RuleMaker.psm1` |

Spec rows that read differently from how the spec words them (round 1):

- `"^(a)$"` alone parses in the stock grammar. The failing context is a switch clause
  condition: `switch -regex ($x) { "^(a)$" { 1 } }`.
- `$env:TEMP\x.ps1` as a whole statement is not valid PowerShell. The valid shape is
  an argument, which is what is fixed.

## Known still failing

Round 1 and round 2 together. Each is valid PowerShell unless noted.

- `$env:TEMP\x.ps1` as a statement. Invalid PowerShell.
- A smart double quote closes a string in PowerShell: `"globalVar“`
  (PSScriptAnalyzer `AvoidGlobalOrUnitializedVars.ps1`). Not handled.
- `using namespace X` followed by an attribute and a `param()` block at file level
  (PSScriptAnalyzer `AvoidUsingArrayList.tests.ps1`). The `using` line parses as a
  command, and a param block cannot follow a statement.
- A backtick followed by a space and a line break as an argument
  (PSScriptAnalyzer `MisleadingBacktick.ps1`).
- `Write-Output $a+2`, `foo $a.b\c`, `foo "$a"\b`, `foo --x=$(Get-Date)`, a tab
  between arguments, bare `foo --%`, `foo -a,-b`.
- For contrast, these parse: `foo -x="a b"` and `foo -x='a b'` are one word, and
  `foo -Name:"x"` is a parameter and an argument.
- A double-quoted segment that holds a nested quote inside a command-line word
  (`-p:V="$($a.Replace("x","y"))"`).
- `-p:Name=value` and `-Name:value` options: the parser sees a parameter, a separator
  and a word, so the adapter counts two arguments where a native command gets one.
- `$a.Foo {1}` (white space before the brace) is invalid and stays an error.
- A bare `[Flags]` before `enum` or `class` parses here; pwsh rejects it.
- Trailing comma `foo a,b,`. about_Parsing says trailing commas are not supported.
- `2>$null` and `>$null` with no space after the operator lex as one `generic_token`
  word, not as a redirection. No error is reported, and the stock grammar does the
  same. `2>&1` and `> $null` with a space are redirections.
- `ps_shape` in the sloplint adapter counts a `generic_token` as a literal of its full
  length. A word such as `$BunBin\bun.exe` therefore looks like static text. This was
  already true for words like `v$VERSION.tar.gz`.
- `param()` blocks inside a function body give the adapter an empty parameter list.
  The adapter only reads the parameter list of `function f($a)`. This is the same
  on the stock grammar.
- Files that are not PowerShell: btcrs libbitcoin `templates/gsl.build.ps1` (GSL),
  virtualenv `activation/powershell/activate.ps1` (`__BIN_NAME__` placeholders),
  PSScriptAnalyzer `Tests/Engine/CSharp.ps1` (pwsh `ExpectedExpression`).
- Two residue files of the wave-2 sweep were not isolated, as agreed.

## Upstream corpus (`tree-sitter test`)

- Stock grammar.js regenerated with 0.27.1: 139 of 139 pass.
- Patched: 139 of 139 upstream cases pass, plus 47 `sloplint:` cases. 186 of 186.
- The new cases are in `test/corpus/sloplint_*.txt`. Each one is named `sloplint: ...`.
- Round 2 trees were produced with `tree-sitter test -u -i sloplint` and then read: no
  `ERROR` or `MISSING` node in any file, and the `sloplint_*` snapshots of round 1 did
  not change.
- The nvidia-smi corpus case leaves out `2>$null` on purpose. A tree that pins the
  redirect as a word would record the defect above as correct.

## Differential against the published crate

`grammar-diff` (staged published 0.26.4 with renamed C globals against this crate),
660 files: the four wave-2 corpora (PowerShell, Pester, PSReadLine, PSScriptAnalyzer)
plus the 112 PowerShell files found on this host, 3 skipped as non-UTF-8, 657 compared:

- 457 identical s-expression.
- 22 clean in both, different s-expression: the shape changes listed above.
- 172 error to clean.
- 0 clean to error.
- 4 error to error, same first error position; 2 error to error, different position.

`pwsh 7.6.7` `ParseFile` on the same four corpora (548 files, 545 after dropping the
BOM and 3 non-UTF-8 files for tree-sitter): 4 files have parse errors, all semantic or
non-PowerShell. Files that pwsh accepts and tree-sitter rejects: 160 with the stock
grammar, 63 after round 1, 3 after round 2 (the smart quote, `using namespace` and
backtick-space files above).

sloplint `meta/parse-error` on `.ps1` and `.psm1` files, stock binary against this
crate, `--no-cache --rule meta/parse-error`:

| Corpus | files | before | after |
| --- | --- | --- | --- |
| PowerShell | 87 | 19 | 0 |
| Pester | 335 | 59 | 0 |
| PSReadLine | 95 | 6 | 0 |
| PSScriptAnalyzer | 435 | 79 | 4 |

The four remaining PSScriptAnalyzer files are the smart quote, `using namespace`,
backtick-space and `CSharp.ps1` (not PowerShell) files above.

## sloplint sweep (round 1)

Run with the stock and patched binaries over three roots, filtering to `meta/parse-error`.
Only files that sloplint walks are counted. It skips hidden directories and `build/`
paths, so scripts under `~/compiler/bamTiScript/.references` and
`enterprise-certificate-proxy/build/scripts/windows_amd64.ps1` were checked with
`tree-sitter parse` and `grammar-diff`, not with sloplint.

| Root | PowerShell parse errors before | after |
| --- | --- | --- |
| `~/.cargo/registry/src/...` | 3 | 0 |
| `~/go/pkg/mod` | 2 | 0 |
| `~/compiler` | 0 (no PowerShell file walked) | 0 |

## Rust regression test

`crates/sloplint-syntax/tests/grammar_gaps/powershell.rs`. 28 tests. Against the stock
crate 27 of them fail; the one that passes is the test that invalid snippets still
report a parse error.
