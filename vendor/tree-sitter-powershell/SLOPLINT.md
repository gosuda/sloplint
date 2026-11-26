# tree-sitter-powershell, patched for sloplint

The local changes below were made in four rounds on the 0.26.4 crate.
The v0.26.6 update keeps the local fixes that sloplint still needs.

## Origin

- Upstream repo: https://github.com/airbus-cert/tree-sitter-powershell
- Upstream tag: `v0.26.6`, commit `65ea6919e816a7ac142acec7ee72757f89db8439`.
- The published `tree-sitter-powershell` 0.26.6 crate records that commit in
  `.cargo_vcs_info.json`. Its `grammar.js` and `src/scanner.c` match the tag.
- The preceding 0.26.4 crate was published from
  `d398441825243b00e317e87e1829b9d6a3e54ce0` (tag `v0.26.5`). The grammar and
  scanner were the same as at `v0.26.4` (`4ffe56d66aa926c28442befbb5b89d1437d7199c`).
- The tag supplies `test/corpus`, which the crate does not ship.

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
- Apply `sloplint-grammar.patch` to the pinned upstream tree with
  `git apply -p1`. Run `tree-sitter generate` there. From the sloplint root,
  `bash scripts/grammars.sh verify tree-sitter-powershell` checks the result
  against the vendored files. Run `tree-sitter test` in the vendor directory
  to include the sloplint corpus.
- The regenerated parser uses ABI 15 and has 2,017 states.

## Native Unicode argument separators

Command argument separators and the external bare-dash scanner use PowerShell's
whitespace set without CR and LF. The same 23 characters delimit command names
and bare arguments; quoted and backtick-escaped spaces remain inside one word.
U+200B, U+FEFF and U+2060 remain literal text after a dash prefix.

PowerShell 7.7.0 native argv captures cover each separator after a command name,
between arguments, and before and after single and double dashes. Rust adapter
regressions assert those argument counts and the protected-space controls.

## Native argument follow-up

The command word rules explicitly exclude unquoted U+00A0 from plain
characters, including attached-option tails. Quoted and backtick-escaped
segments still retain that character. Tabs and U+00A0 separate successive
native arguments, including after an end-of-options marker.

Comma-list elements accept parameter-shaped bare words only in list
context, so both spaced and unspaced dash-colon lists remain one array.
A dedicated hidden rule owns a parenthesized list after a double dash;
aliasing that symbol preserves one array node and its parenthesized child.

Regenerated grammar.json, node-types.json and parser.c with tree-sitter-cli
0.27.1 using the command above. All 240 corpus cases pass, including the
thirteen new boundary cases. Standalone parameters keep their existing node
kind. Variable-prefixed paths use the same protected word-tail segments,
and command names delimit unquoted U+00A0 while retaining quoted and escaped
segments. Rust adapter regressions assert the resulting argument counts.

## Expandable path and continuation boundaries

Variable-prefixed paths remain one `generic_token` argument. Double-quoted
segments share the ordinary expandable-string body: literal dollar signs
and hashes remain text, while a missing `)` inside `$(...)` is a syntax
error. Balanced and nested expressions, escaped dollar signs, verbatim
segments, and protected U+00A0 retain their argument boundaries.

Parameter and command-name escape pairs exclude line breaks, leaving
their LF and CRLF continuations to the line-continuation extra. Inside an
already-started bare argument, a backtick can escape CR. The following LF
then ends the command, matching the PowerShell 7.7.0 parser with either
indented or column-one text on the next line.

All 245 corpus cases pass with tree-sitter-cli 0.27.1. The five word-boundary
corpus cases and Rust adapter regressions cover quoted path literals,
interpolation, and the context-specific continuation behavior. Native
PowerShell 7.7.0 parsing separately confirms these accepted and rejected
forms; this comparison does not execute the commands.

## Literal dollars and blank-line boundaries

The external scanner accepts a literal dollar before whitespace, a backslash,
or a backtick. It leaves the following newline untouched so an expandable
here-string can still close on that line. A leading pipe continues the previous
statement only when no blank line intervenes. Comment-only lines and
non-breaking-space indentation do not count as blank: the lookahead skips
them while still rejecting an actual blank line. Blank lines before `else`
retain their existing behavior.

These scanner-only changes leave grammar.js and the generated parser unchanged.
The Rust regressions cover LF and CRLF, whitespace-only blank lines, valid
here-string endings, and malformed interpolation controls. CLI fixtures also
cover searchable and short interpolated Windows paths; the adapter uses
PowerShell's backtick escape character when measuring their literal runs.

## Literal accounting in interpolated words

The adapter excludes hidden variable prefixes and quote delimiters when measuring
literal runs in interpolated command words. Adjacent quoted and bare segments
share a run. Single-quoted backticks and dollar signs remain literal; expandable
and bare escapes end a run. Named subexpressions retain their AST boundaries.
Variable names follow PowerShell's UTF-16 letter and decimal-digit categories,
including the distinction between decimal digits and other numeric characters.

## Comment continuation and interpolated word escapes

A comment-only line does not end a pipeline before a leading pipe. The
statement-terminator lookahead skips line and block comments and recognizes
PowerShell's Unicode whitespace independently of the C locale. An actual
blank line still ends continuation. Block comments end at the first `#>`;
format marks such as U+200B are not whitespace in the PowerShell parser.

A valid backtick Unicode escape is one lexical unit, including its one to
six hexadecimal digits and closing brace. Its spelling cannot form a
searchable literal run. The adapter also detects variables inside anonymous
generic words such as `v$VERSION.tar.gz`, while retaining literal dollar
signs in escaped and verbatim text. A hidden `$()` expression is one dynamic boundary, including its complete
property chain. Non-interpolated generic words keep their existing fact shape.

The continuation and literal-run Rust tests cover the PowerShell 7.7.0
accepted and rejected forms. CLI fixtures pair short interpolated messages
with searchable prefixes and verbatim text. Regenerate the parser and the
upstream-relative patch with tree-sitter-cli 0.27.1 as described above.

## Multiline interpolation and parenthesized pipelines

An expandable here-string line break immediately before a dollar is a separate
external token. It leaves the interpolation opener intact. Closing delimiters
and line-start escapes retain their existing line-break tokens. Parenthesized pipelines use the same statement terminator
as top-level pipelines, including grouped expressions and control-flow clauses.
A blank line cannot reconnect either form to a leading pipe.

The focused Rust regressions cover both LF and CRLF. The vendored corpus covers
valid interpolation and escapes after a literal-dollar line, malformed braced
variables and subexpressions, and adjacent versus blank-separated pipes.

## What v0.26.6 replaces

Upstream [PR #65](https://github.com/airbus-cert/tree-sitter-powershell/pull/65)
(`b03e6b0e39692037a98e37a26f0a969b2c46fbc1`) replaces the local digit-in-parameter
regex and the bare `--` alias. Its scanner accepts both `--` and `-`. It skips
the space before a bare `--` rather than emitting a `command_argument_sep`.
The adapter already ignores that separator. The release also adds trailing
`&` as a `background_operator`.

Keep the local `generic_token` for attached option words. The adapter reads
it as a string argument, and the token keeps commas and quoted segments in
the value. Keep the local comma-list rules: they produce one
`array_literal_expression` instead of separate arguments for the items.
Both groups overlap PR #65. Open upstream issues
[#46](https://github.com/airbus-cert/tree-sitter-powershell/issues/46) and
[#59](https://github.com/airbus-cert/tree-sitter-powershell/issues/59) concern
related argument parsing.

The remaining local groups do not have an upstream fix in this release:

| Local grammar or scanner group | Upstream tracking |
| --- | --- |
| Ternary | [#7](https://github.com/airbus-cert/tree-sitter-powershell/issues/7), open |
| Argument words, variable paths and dots, quoted and escaped words, member-call tails, redirects | #46 and #59 overlap option arguments; [#57](https://github.com/airbus-cert/tree-sitter-powershell/issues/57) concerns leading backticks; no exact issue identified for the other forms |
| Named and empty blocks, switch clauses, constructors, attributes and declarations, labels, parameter bodies | No issue or PR identified for these changes |
| Case-insensitive numbers, cast operands, type names and generic ranks, bitwise and membership operators | No issue or PR identified for these changes |
| String text and here-string escapes, statement terminators, method-block scanner | No issue or PR identified for these changes |

The current patch has 2,355 lines and 49 diff hunks. It includes the
native argument corpus along with the grammar and scanner changes.

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
    `-Count2`). The upstream rule now provides this behavior.
  - A command-line word may hold quoted segments that contain spaces, pipes or
    parentheses: `--format="%h %B"`, `--format='%H||%P'`. The segments are matched
    inside the word token. A double-quoted segment holds no nested quote.
    `-p:V="$($m.x)"` is a `-p` parameter, a `:` separator and then one word
    `V="$($m.x)"`, so the adapter sees two arguments, not one. pwsh passes one string
    to a native command; that shape is not modelled.
  - A bare `--` or `-` is a `command_parameter` from the upstream scanner.
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
- `generic_token` has two more forms.
  - A bare escape sequence used as a word: `` `r`n `` in
    `Write-Host `r`n` — expandable-string content without quotes.
  - A word that interleaves a variable head with quoted or plain segments:
    `$x"cat"$y` is one `generic_token`. The variable head must be followed by
    a complete quoted segment, so `$x.Foo()` still parses as member access and
    `Write-Output $x"cat"; Write-Host "ok"` keeps both commands.
- A text run in a double-quoted string has lexical precedence. A run that started at
  white space or `#` after a variable or an escape was read as a comment
  (`"`n## x"`, `"$a ## x"`). No scanner code is needed for this.
- Here-strings: a line that starts with a backtick pair keeps the pair.
  ``` ``````PowerShell ``` inside `@" "@` failed, and `` `$x `` at the start of a
  line produced a phantom `variable` node.

## Round 3 grammar changes

- Attributes may stand before any statement, not only `class` and `enum`:
  `[SuppressMessageAttribute('x','')] $x = 1`. The `[Name(args)]` form is the
  hidden rule `_parenthesized_attribute`, aliased to `attribute`; a bare
  `[Name]` stays the `type_literal` attribute alternative. One conflict
  `[_statement, attribute]` is declared. An attribute before `function`,
  `filter` or `workflow` (`[Alias('x')] function f { }`) is not the statement
  form: it attaches inside `function_statement` via `optional(attribute_list)`,
  as `class` and `enum` already did, so the parse keeps the declaration rather
  than a command named `function`.
- `type_identifier` accepts a generic arity of more than one digit: `[Action``2]`.
  The backtick run is ``[`+]+``, not a single `` ` ``.
- A redirect may follow its operator with no separator, `'/r'>null`: the file
  name then lexes as `generic_token` and is a `redirected_file_name` alternative.
- A cast operand may no longer begin with `,`. Before, `param($T = [Exception],`
  read the comma as the comma-prefix unary inside the cast and `param($a = 1, $b)`
  worked only because `1` cannot head a cast. `script_parameter_default` keeps
  `_expression`, so `= 1,2` inside `param(...)` still errors (upstream behaviour
  for a number default: `,` is the parameter separator). The operand is the hidden
  `_cast_operand`, aliased to `unary_expression`, so node kinds are unchanged;
  two conflicts (`[_cast_operand, member_access, invokation_expression]`,
  `[_cast_operand, invokation_foreach_expression]`) cover the member-access
  ambiguity the shadow rule introduces.

## Round 4 grammar changes

- `generic_token` has two more forms.
  - A variable head followed by `..` or by a `.` that does not open a member
    name is one argument-mode word: `to $EndDate...`, `$v..$w`. `$v.foo`
    still parses as member access — neither alternative accepts a single `.`
    before an identifier char, and the post-dot char is limited to the word
    tail set, so `$v.` at end of line still lexes as upstream did (it errors;
    argument-mode `.` at EOL is a PS edge the token language cannot express
    without lookahead).
  - A word that starts with a quoted segment and keeps going: `"a"`n"b"`,
    `'a'`n'b'`. At least one continuation is required, so a bare `"a"` still
    lexes as `expandable_string`/`string_literal`.
- Round-4 residuals observed and rejected as broken-PS parity with upstream:
  backtick followed by trailing spaces before EOL (no line continuation in
  pwsh either) and Unicode-dash operator typos (`–replace` with U+2013).

## node-types.json changes

No node kind or field was renamed or removed in the v0.26.6 update. The
adapter does not need to change. Relative to plain v0.26.6, the local changes
add the kinds and children below.

- New named node: `ternary_expression`. It is a child wherever an expression is.
- New anonymous nodes: `base`, `this`, `?`, `-cin`, `-cnotin`, `-iin`, `-inotin`.
- `program` children are no longer required, and may be `named_block_list`.
- `array_literal_expression` children may include `generic_token`.
- `class_method_definition` children may include `argument_list`.
- `class_statement` and `enum_statement` may have an `attribute_list` child.
- `command_elements` may hold a `member_name` sibling after an argument list.
- `script_block_expression` children are no longer required: `{}` is valid.
- `label_expression` may hold a `simple_name`.

## Shape changes in code that already parsed

The historical shape check below compares the patch with the stock 0.26.4
crate. These changes can affect adapter facts.

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

These limits were recorded during the original sweeps. Each example is valid
PowerShell unless noted. The v0.26.6 update does not address these limits.

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
- `param()` blocks inside a function body give the adapter an empty parameter list.
  The adapter only reads the parameter list of `function f($a)`. This is the same
  on the stock grammar.
- Files that are not PowerShell: btcrs libbitcoin `templates/gsl.build.ps1` (GSL),
  virtualenv `activation/powershell/activate.ps1` (`__BIN_NAME__` placeholders),
  PSScriptAnalyzer `Tests/Engine/CSharp.ps1` (pwsh `ExpectedExpression`).
- Two residue files of the wave-2 sweep were not isolated, as agreed.

## Corpus (`tree-sitter test`)

- With CLI 0.27.1: 165 upstream cases and 54 sloplint cases pass (219 total).
- In `native_arguments.txt`, six expected trees use the retained local
  shapes: three attached-option cases and three comma-list cases. All six
  inputs are unchanged.
- The sloplint double-dash case in `sloplint_round2.txt` expects one fewer
  separator node. The other 53 sloplint expected trees are unchanged.
- The nvidia-smi corpus case leaves out `2>$null` on purpose. A tree that pins the
  redirect as a word would record the defect above as correct.

## Historical differential against the 0.26.4 crate

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

## Rust regression tests

The tests in `crates/sloplint-syntax/tests/grammar_gaps/powershell.rs` and
`powershell_syntax.rs` check both parse health and the adapter's facts. The
original 0.26.4 comparison had 29 failures out of 30 tests. The one test that
passed checked that invalid snippets still report a parse error.
