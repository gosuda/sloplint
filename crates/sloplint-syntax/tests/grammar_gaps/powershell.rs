//! Valid PowerShell that stock tree-sitter-powershell 0.26.4 reports as a syntax error.
//!
//! The sources are slices of real scripts (named per constant) or the shape one of them
//! uses. The vendored grammar in `vendor/tree-sitter-powershell` must parse each one with
//! no error node, must still reject invalid text, and must keep the node shapes the
//! adapter reads.

use std::sync::LazyLock;

use sloplint_core::facts::{ArgShape, Call};
use sloplint_core::lang::Lang;
use sloplint_core::{CustomRuleSpec, Severity};
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "gap.ps1",
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_powershell::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("powershell language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let root = tree.root_node();
    assert!(
        !root.has_error() && !root.to_sexp().contains("MISSING"),
        "raw tree must be error-free, sexp: {}",
        root.to_sexp()
    );
}

fn run_clean<'s>(source: &'s str) -> Analysis<'s> {
    assert_raw_clean(source);
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_none(),
        "must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn call<'a, 's>(analysis: &'a Analysis<'s>, callee: &str) -> &'a Call<'s> {
    analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == callee)
        .unwrap_or_else(|| {
            panic!(
                "no call to {callee}; calls are {:?}",
                analysis
                    .facts
                    .calls
                    .iter()
                    .map(|c| c.callee)
                    .collect::<Vec<_>>()
            )
        })
}

fn str_len(arg: &sloplint_core::facts::Arg<'_>) -> Option<u32> {
    match arg.shape {
        ArgShape::Str { longest_static } => Some(longest_static),
        _ => None,
    }
}

fn len(text: &str) -> u32 {
    u32::try_from(text.chars().count()).expect("test literal fits in u32")
}

/// ps-parser 1.1.0 `script.ps1`, verbatim: a param block, then named blocks at file level.
const PS_PARSER_SCRIPT: &str = "[CmdletBinding(DefaultParameterSetName = \"Path\", HelpURI = \"https://go.microsoft.com/fwlink/?LinkId=517145\")]
param(
\t[Parameter(ParameterSetName=\"Path\", Position = 0)]
\t[System.String[]]
\t$Path


)

begin
{
\t# Construct the strongly-typed crypto object
}

process
{
\tWrite-output elo
}
";

/// cilium/ebpf v0.22.0 `scripts/windows/Setup.ps1`, first 14 lines.
const CILIUM_SETUP: &str = "# Configure a fresh installation of Windows via SSH.

param (
  [switch] $RunOnce = $false
)

if ($RunOnce) {
  # Visual Studio really doesn't seem to like being installed via SSH, so
  # we invoke from a RunOnce script.
  Invoke-WebRequest 'https://raw.githubusercontent.com/microsoft/ebpf-for-windows/main/scripts/Setup-DevEnv.ps1' -OutFile $env:TEMP\\Setup-DevEnv.ps1
  &\"$env:TEMP\\Setup-DevEnv.ps1\"

  # sshd needs to be restarted to pick up new environment variables.
  Restart-Service sshd
}
";

/// google/osv-scalibr `build_protos.ps1`, the protoc lines.
const OSV_PROTOS: &str = "protoc -I=binary --go_out=binary/proto binary/proto/*.proto
Move-Item binary/proto/github.com/google/osv-scalibr/binary/proto/* binary/proto/
Remove-Item -Recurse -Force binary/proto/github.com, binary/proto/vulnerability.proto
";

/// rust-librocksdb-sys `build_tools/run_ci_db_test.ps1`, the gtest runner block.
const ROCKSDB_RUNNER: &str = "# Invoke a test with a filter and redirect all output
$InvokeTestCase = {
    param($exe, $test, $log);
    &$exe --gtest_filter=$test > $log 2>&1
}
";

/// googleapis/enterprise-certificate-proxy `build/scripts/windows_amd64.ps1`.
const ECP_BUILD: &str =
    "go build -buildmode=c-shared -o .\\build\\bin\\windows_amd64\\libecp.dll .\\cshared\\main.go
go build -buildmode=c-archive -o .\\build\\bin\\windows_amd64\\libecp.lib .\\cshared\\main.go
";

/// bun `sign-windows-artifacts.ps1`, `bootstrap.ps1`, and `install.ps1` argument shapes.
const BUN_ARGS: &str = "function Sign($ZipName, $extractDir, $headersTar, $stage, $RegistryKey, $BunBin) {
    & cmake -E tar cfv $ZipName --format=zip $extractDir
    & tar -xzf $headersTar -C $stage --strip-components=1
    New-ItemProperty -Path $RegistryKey -Name \"DisplayIcon\" -Value $BunBin\\bun.exe -PropertyType String -Force | Out-Null
}
";

/// node `pgo.ps1`: multiplier suffix in capitals.
const NODE_PGO: &str = "$totalSize = ($profrawFiles | Measure-Object -Property Length -Sum).Sum
$totalSizeMB = [math]::Round($totalSize / 1MB, 1)
";

/// oak-powershell lexer fixture `basic.ps1`: bare-word list with a space and a chained constructor.
const OAK_BASIC: &str =
    "Get-ChildItem -Path . -Recurse -Filter \"*.ps1\" | Select-Object Name, Length

class Dog : Animal {
    Dog([string]$name) : base($name) {}

    [void] Speak() {
        Write-Host \"Woof\"
    }
}
";

/// btcrs `profile-windows.ps1`: option values that are comma lists.
const NVIDIA_SMI: &str = "$gpuLine = (nvidia-smi --query-gpu=utilization.gpu,utilization.memory,memory.used --format=csv,noheader,nounits 2>$null | Select-Object -First 1)
";

/// Switch over a quoted regex that ends in a group, the shape of a command dispatcher.
const REGEX_SWITCH: &str = "switch -regex ($Command) {
    \"^(activate|deactivate)$\" { Write-Output \"toggle\" }
    default { Write-Output \"other\" }
}
";

#[test]
fn named_blocks_follow_a_param_block_at_file_level() {
    let analysis = run_clean(PS_PARSER_SCRIPT);
    let write = call(&analysis, "Write-output");
    assert_eq!(
        write.args.len(),
        1,
        "one bare word argument: {:?}",
        write.args
    );
    assert_eq!(
        str_len(&write.args[0]),
        Some(len("elo")),
        "the word `elo` is a string arg"
    );
    let same_line = run_clean("begin { $x = 1 }\nprocess { Write-Output $_ }\nend { $x }\n");
    assert_eq!(
        call(&same_line, "Write-Output").args.len(),
        1,
        "a named block with the brace on the same line parses at file level too"
    );
}

#[test]
fn a_script_of_only_a_param_block_or_only_a_comment_is_valid() {
    run_clean("param([string] $Name)\n");
    run_clean("# Nothing to do here.\n");
}

#[test]
fn an_argument_may_start_with_a_variable_and_continue_as_a_path() {
    let analysis = run_clean(CILIUM_SETUP);
    let web = call(&analysis, "Invoke-WebRequest");
    let path = "$env:TEMP\\Setup-DevEnv.ps1";
    assert!(
        web.args.iter().any(|a| str_len(a) == Some(len(path))),
        "the variable path is one word argument of {} chars: {:?}",
        len(path),
        web.args
    );
    let restart = call(&analysis, "Restart-Service");
    assert_eq!(
        restart.args.iter().filter_map(str_len).collect::<Vec<_>>(),
        vec![len("sshd")],
        "the control argument keeps its word shape"
    );
    let bun = run_clean(BUN_ARGS);
    let item = call(&bun, "New-ItemProperty");
    assert!(
        item.args
            .iter()
            .any(|a| str_len(a) == Some(len("$BunBin\\bun.exe"))),
        "a root variable followed by a path is one word: {:?}",
        item.args
    );
}

#[test]
fn native_options_with_an_attached_value_are_single_words() {
    let analysis = run_clean(OSV_PROTOS);
    let protoc = call(&analysis, "protoc");
    assert_eq!(
        protoc.args.iter().filter_map(str_len).collect::<Vec<_>>(),
        vec![
            len("-I=binary"),
            len("--go_out=binary/proto"),
            len("binary/proto/*.proto")
        ],
        "each option and the glob is its own word: {:?}",
        protoc.args
    );
    let go = run_clean(ECP_BUILD);
    let build = call(&go, "go");
    assert!(
        build
            .args
            .iter()
            .any(|a| str_len(a) == Some(len("-buildmode=c-shared"))),
        "-buildmode=c-shared is one word: {:?}",
        build.args
    );
    let bun = run_clean(BUN_ARGS);
    assert!(
        call(&bun, "cmake")
            .args
            .iter()
            .any(|a| str_len(a) == Some(len("--format=zip"))),
        "--format=zip is one word after a named call operator command"
    );
}

#[test]
fn a_variable_command_takes_a_long_option_with_a_variable_value() {
    let analysis = run_clean(ROCKSDB_RUNNER);
    let invoke = call(&analysis, "$exe");
    assert!(
        invoke
            .args
            .iter()
            .any(|a| str_len(a) == Some(len("--gtest_filter=$test"))),
        "the long option is one word: {:?}",
        invoke.args
    );
}

#[test]
fn bare_word_lists_are_one_array_argument() {
    let analysis = run_clean(OAK_BASIC);
    let select = call(&analysis, "Select-Object");
    assert_eq!(
        select.args.len(),
        1,
        "one array argument: {:?}",
        select.args
    );
    assert_eq!(
        select.args[0].shape,
        ArgShape::Other,
        "an array is not a string"
    );
    let tight = run_clean("Select-Object Name,Length\n");
    assert_eq!(
        call(&tight, "Select-Object").args.len(),
        1,
        "no-space list is one argument"
    );
    let mixed = run_clean("foo a,\"b\",$c\n");
    assert_eq!(
        call(&mixed, "foo").args.len(),
        1,
        "mixed list is one argument"
    );
    let remove = run_clean(OSV_PROTOS);
    assert_eq!(
        call(&remove, "Remove-Item").args.len(),
        3,
        "the existing -Recurse -Force list control still yields three arguments"
    );
}

#[test]
fn an_option_value_may_be_a_comma_list() {
    let analysis = run_clean(NVIDIA_SMI);
    let smi = call(&analysis, "nvidia-smi");
    let words: Vec<u32> = smi.args.iter().filter_map(str_len).collect();
    let query = "--query-gpu=utilization.gpu,utilization.memory,memory.used";
    let format = "--format=csv,noheader,nounits";
    assert!(
        words.starts_with(&[len(query), len(format)]),
        "a native command gets each option, commas included, as one string (about_Parsing): {:?}",
        smi.args
    );
}

#[test]
fn a_switch_clause_may_be_a_quoted_regex_that_ends_in_a_group() {
    let analysis = run_clean(REGEX_SWITCH);
    assert_eq!(
        analysis.facts.wildcard_arms.len(),
        1,
        "the default arm is still the one wildcard arm: {:?}",
        analysis.facts.wildcard_arms
    );
    run_clean("switch ($x) { \"a b\" { 1 } }\n");
    run_clean("switch ($x) { \"ab\" { 1 } 'cd' { 2 } abc { 3 } default { 4 } }\n");
    run_clean(
        "switch ($x) {\n    \"ab\" { 1 }\n    'cd' { 2 }\n    abc { 3 }\n    default { 4 }\n}\n",
    );
    let one_line = run_clean("switch -regex ($x) { \"^(a)$\" { 1 } default { 2 } }\n");
    assert_eq!(
        one_line.facts.wildcard_arms.len(),
        1,
        "clauses that share a line keep the default arm: {:?}",
        one_line.facts.wildcard_arms
    );
}

#[test]
fn multiplier_suffixes_and_numeric_prefixes_ignore_case() {
    let analysis = run_clean(NODE_PGO);
    let round = call(&analysis, "[math]::Round");
    assert_eq!(
        round.args.len(),
        2,
        "both arguments of Round are kept: {:?}",
        round.args
    );
    run_clean("$mask = 0XFF\n$big = 1E3\n$limit = 10GB\n$small = 3Kb\n");
    let arg = run_clean("foo 5MB\n");
    assert_eq!(
        call(&arg, "foo").args.len(),
        1,
        "5MB is one number argument"
    );
}

#[test]
fn a_constructor_may_chain_to_the_base_class() {
    let analysis = run_clean(OAK_BASIC);
    let names: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert!(
        names.contains(&"Dog"),
        "the constructor is a function: {names:?}"
    );
    assert!(
        names.contains(&"Speak"),
        "the method after it is a function: {names:?}"
    );
    run_clean("class Dog : Animal {\n    Dog() : this(\"rex\") {}\n}\n");
}

/// Pester `tst/Pester.Mock.RSpec.ts.ps1` lines 1 to 3: a typed switch list, then a bare-word list.
const PESTER_TEST_HEAD: &str = "param ([switch] $PassThru, [switch] $NoBuild)

Get-Module Pester.Runtime, Pester.Utility, P, Pester, Axiom, Stack | Remove-Module
";

/// Pester `test.ps1` line 59, `PowerShell/test/tools/NamedPipeConnection/build.ps1` line 49.
const VARIABLE_SLASH: &str = "Import-Module $PSScriptRoot/bin/Pester.psd1 -ErrorAction Stop
. $PSScriptRoot/doBuild.ps1
";

/// Pester `src/csharp/Sync-WithProfiler.ps1` line 12 and `DiffPlex/Update-VendoredDiffPlex.ps1` line 166.
const GIT_NATIVE: &str = "$commit = git -C $profilerPath log --format=\"%h %B\" -n 1
& git -C $staging apply -p2 --whitespace=nowarn $patchFile
";

/// PowerShell `build.psm1` line 3268: a native command after a double dash.
const DOTNET_DASHES: &str = "function Build-TypeCatalog {
    try {
        Start-NativeExecution { dotnet run -- ../System.Management.Automation/CoreCLR/CorePsTypeCatalog.cs $IncFileName }
    } finally {
        Pop-Location
    }
}
";

/// Pester `build.ps1` line 93: native options whose values hold quotes and subexpressions.
const DOTNET_BUILD: &str = "dotnet build \"$PSScriptRoot/src/csharp/Pester.sln\" --no-restore --configuration $Configuration -p:VersionPrefix=\"$($manifest.ModuleVersion)\" -p:VersionSuffix=\"$($manifest.PrivateData.PSData.Prerelease)\"
";

/// Pester `tst/functions/TestDrive.Tests.ps1` line 68, `tst/TypeClass.Tests.ps1` line 90-91,
/// `src/Pester.Runtime.ps1` line 1197, `src/functions/Mock.ps1` line 12.
const EMPTY_BLOCKS: &str = "Mock Remove-Item {}
$cases = @(
    @{ Value = {} },
    @{ Value = { abc } }
)
$RootBlock.ScriptBlock = {}
function Invoke-Mock {
    param(
        [Parameter(Mandatory)]
        $ContextInfo,
        [ScriptBlock] $MockWith = { },
        [Switch] $Verifiable
    )
}
";

/// Pester `tst/Pester.CustomAssertion.ts.ps1` line 136 and `src/functions/Pester.SessionState.Mock.ps1`.
const PARAM_SAME_LINE: &str =
    "function Get-Thing { [CmdletBinding()] param ([string] $Name) 'real' }
function Read-Path { param($Path) Get-Content -Path $Path }
";

/// PowerShell `tools/UpdateDotnetRuntime.ps1` line 70 and `tools/packaging/packaging.psm1` line 2763.
const GENERIC_TYPES: &str = "function Get-Packages {
    $packages = [System.Collections.Generic.Dictionary[[string], [PkgVer[]] ]]::new()
}
function New-Package {
    param(
        [parameter(Mandatory = $false)]
        [tuple[ [tuple[string, string]], [tuple[string, string]] ] []] $Dependencies
    )
}
";

/// PSReadLine `test/Get-KeyInfoData.ps1` lines 125 to 130.
const FLAGS_ENUM: &str = "[Flags()]
enum ConsoleModeInputFlags
{
    ENABLE_PROCESSED_INPUT = 0x0001
    ENABLE_LINE_INPUT = 0x0002
}
";

/// PSScriptAnalyzer `Tests/Rules/DSCResourceModule/DSCResources/MyDscResource/MyDscResource.psm1`.
const DSC_CLASS: &str = "[DscResource()]
class FileResource
{
    [DscProperty(Key)]
    [string] $Path

    [void] Set()
    {
        Write-Host $this.Path
    }
}
";

/// PSReadLine `PSReadLine/SamplePSReadLineProfile.ps1` line 194 and Pester
/// `tst/Pester.RSpec.Parallel.ts.ps1` line 1319.
const METHOD_BLOCK: &str = "if ($line[0..$cursor].Where{$_ -eq $quote}.Count % 2 -eq 1) {
    Insert($quote)
}
$actual = (($normalized -split \"`r`n|`r|`n\").ForEach({ $_.TrimEnd() }) -join \"`n\").Trim()
";

/// Humanizer `tools/docs/api-xml-input.ps1` lines 15 and 16.
const CIN_OPERATOR: &str = "if ($record.Kind -ceq \"Method\" -and
    $record.Name -cin @(\"op_Implicit\", \"op_Explicit\", \"op_CheckedExplicit\")) {
    Write-Host $record.Name
}
";

/// PowerShell `tools/wix/wix.psm1` line 31.
const TERNARY: &str = "function Get-Root {
    param($arm64 = $false)

    $targetRoot = $arm64 ? \"${env:ProgramFiles(x86)}\\Arm Support WiX Toolset xcopy\" : \"${env:ProgramFiles(x86)}\\WiX Toolset xcopy\"
    Write-Host $targetRoot
}
";

/// PSScriptAnalyzer `Tests/Engine/Settings.tests.ps1` lines 383 to 385.
const BOR_ARGUMENT: &str = "$findSettingsMode = ($settingsTypeName -as [type]).GetMethod(
    'FindSettingsMode',
    [System.Reflection.BindingFlags]::NonPublic -bor [System.Reflection.BindingFlags]::Static)
";

/// Pester `tst/Pester.RSpec.ts.ps1` line 3601.
const LOOP_LABEL: &str = "function Invoke-StrayBreak { break outerLoop }
foreach ($i in 1..3) { if ($i -eq 2) { continue top } }
";

/// Pester `src/functions/assertions/HaveCount.ps1` lines 15 to 20.
const ELSE_AFTER_ASSIGNMENT: &str = "$count = if ($null -eq $ActualValue) {
    0
}
else {
    $ActualValue.Count
}
Write-Host $count
";

/// Pester `src/Format.ps1` lines 133 to 147: a backtick continuation, then a blank line.
const BACKTICK_CONTINUATION: &str = "function Get-ShortType ($Value) {
    if ($null -ne $value) {
        $type = Format-Type $Value.GetType()

        $type `
            -replace \"^System\\.\" `
            -replace \"^Management\\.Automation\\.PSCustomObject$\", \"PSObject\" `
            -replace \"^Object\\[\\]$\", \"collection\" `

    }
    else {
        Format-Type $null
    }
}
";

/// PowerShell `tools/ci.psm1` line 1168.
const HASH_IN_STRING: &str = "$summaryContent += \"`n## ❌ Conflicts Detected`n`n\"
";

/// PSScriptAnalyzer `Tests/Rules/AvoidSemicolonsAsLineTerminators.tests.ps1` lines 72 to 85, as an
/// argument. Lines that start with an escaped dollar come after a blank line.
const HERE_STRING_ESCAPES: &str = "Write-Output @\"
`$calcCode = `@\"
public class Calc{
    public int Add(int x,int y){
        return x+y;
    }
}
\"`@

Add-Type -TypeDefinition `$calcCode -Language CSharp

`$c = New-Object Calc
`$c.Add(1,2)
\"@
";

/// PSScriptAnalyzer `Utils/RuleMaker.psm1` line 208: a line that starts with a backtick pair.
const HERE_STRING_BACKTICKS: &str = "$ruleDocTemplate = @\"
# {0}
**Severity Level: {1}**

## Example
### Wrong:
``````PowerShell

``````
\"@
";

/// Cluster PS-COMMA: comma-separated bare words as command arguments.
#[test]
fn a_typed_switch_list_and_bare_word_lists_parse_in_a_script() {
    let analysis = run_clean(PESTER_TEST_HEAD);
    let get = call(&analysis, "Get-Module");
    assert_eq!(get.args.len(), 1, "one array argument: {:?}", get.args);
    assert_eq!(get.args[0].shape, ArgShape::Other, "a list is not a string");
    assert_eq!(
        call(&analysis, "Remove-Module").args.len(),
        1,
        "the piped command takes the previous stage as its one argument"
    );
}

/// Cluster PS-VARPATH: `$var/path` as a command argument.
#[test]
fn an_argument_may_be_a_variable_followed_by_a_slash_path() {
    let analysis = run_clean(VARIABLE_SLASH);
    let import = call(&analysis, "Import-Module");
    let path = "$PSScriptRoot/bin/Pester.psd1";
    assert_eq!(
        import.args.iter().filter_map(str_len).next(),
        Some(len(path)),
        "the variable and its path are one word: {:?}",
        import.args
    );
    let dot = call(&analysis, "$PSScriptRoot/doBuild.ps1");
    assert_eq!(
        dot.args.len(),
        1,
        "a dot-sourced path is the callee: {dot:?}"
    );
}

/// Cluster PS-NATIVE: `git -C ...` dash-arguments, with and without `&`.
#[test]
fn native_git_options_keep_a_quoted_value_with_a_space_in_one_word() {
    let analysis = run_clean(GIT_NATIVE);
    let git = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.callee == "git"
                && c.args
                    .iter()
                    .any(|a| str_len(a) == Some(len("--format=\"%h %B\"")))
        })
        .unwrap_or_else(|| {
            panic!(
                "no git call with the quoted option: {:?}",
                analysis.facts.calls
            )
        });
    assert_eq!(
        git.args.len(),
        6,
        "-C, the variable, log, the option, -n and 1 are six arguments: {:?}",
        git.args
    );
    let apply = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.callee == "git"
                && c.args
                    .iter()
                    .any(|a| str_len(a) == Some(len("--whitespace=nowarn")))
        })
        .unwrap_or_else(|| panic!("no git apply call: {:?}", analysis.facts.calls));
    assert_eq!(
        apply.args[3].shape,
        ArgShape::Other,
        "-p2 is a parameter with a digit, not a word: {:?}",
        apply.args
    );
}

#[test]
fn a_double_dash_ends_the_options_of_a_native_command() {
    let analysis = run_clean(DOTNET_DASHES);
    let dotnet = call(&analysis, "dotnet");
    assert_eq!(
        dotnet.args.len(),
        4,
        "run, --, the path and the variable: {:?}",
        dotnet.args
    );
    assert_eq!(dotnet.args[1].shape, ArgShape::Other, "-- is a parameter");
    let build = run_clean(DOTNET_BUILD);
    let dotnet = call(&build, "dotnet");
    let word = "VersionSuffix=\"$($manifest.PrivateData.PSData.Prerelease)\"";
    assert_eq!(dotnet.args.len(), 9, "nine arguments: {:?}", dotnet.args);
    assert_eq!(
        str_len(&dotnet.args[8]),
        Some(len(word)),
        "the value after -p: is one word with its quotes and subexpression: {:?}",
        dotnet.args
    );
}

/// Cluster PS-EMPTYBLOCK: `{}` as a command argument, a default and a hashtable value.
#[test]
fn an_empty_block_is_a_valid_command_argument_default_and_hashtable_value() {
    let analysis = run_clean(EMPTY_BLOCKS);
    let mock = call(&analysis, "Mock");
    assert_eq!(mock.args.len(), 2, "name and block: {:?}", mock.args);
    assert_eq!(
        mock.args[1].shape,
        ArgShape::Other,
        "the block is not a word"
    );
    let names: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        ["Invoke-Mock"],
        "the function after the blocks is found"
    );
}

/// Cluster PS-PARAM: a multi-typed `param(...)` block with its body on the same line.
#[test]
fn a_function_body_may_continue_on_the_line_of_its_param_block() {
    let analysis = run_clean(PARAM_SAME_LINE);
    let names: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        ["Get-Thing", "Read-Path"],
        "both functions are found"
    );
    assert_eq!(
        call(&analysis, "Get-Content").args.len(),
        2,
        "the body statement after the param block is parsed as a call"
    );
}

/// Cluster PS-GENERIC: `Dictionary[[string],[int]]` and jagged array types.
#[test]
fn generic_type_arguments_may_be_bracketed_and_arrays_may_be_jagged() {
    let analysis = run_clean(GENERIC_TYPES);
    let new = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.callee
                .starts_with("System.Collections.Generic.Dictionary")
        })
        .unwrap_or_else(|| panic!("no constructor call: {:?}", analysis.facts.calls));
    assert_eq!(
        new.callee,
        "System.Collections.Generic.Dictionary[[string], [PkgVer[]] ]"
    );
    let names: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert_eq!(names, ["Get-Packages", "New-Package"]);
}

/// Clusters PS-FLAGS (`[Flags()] enum`) and PS-UNKNOWNATTR (`[DscResource()] class`).
#[test]
fn an_attribute_with_arguments_may_precede_an_enum_or_a_class() {
    let flags = run_clean(FLAGS_ENUM);
    let types: Vec<&str> = flags.facts.types.iter().map(|t| t.name).collect();
    assert_eq!(types, ["ConsoleModeInputFlags"], "the enum is a type");
    let dsc = run_clean(DSC_CLASS);
    let types: Vec<&str> = dsc.facts.types.iter().map(|t| t.name).collect();
    assert_eq!(types, ["FileResource"], "the class is a type");
    let names: Vec<&str> = dsc.facts.functions.iter().map(|f| f.name).collect();
    assert_eq!(names, ["Set"], "its method is a function");
    run_clean("[Foo()]\nclass Bar {\n}\n");
}

/// Cluster PS-WHERE: `.Where{...}` with no parentheses.
#[test]
fn a_method_may_take_a_script_block_with_no_parentheses() {
    let analysis = run_clean(METHOD_BLOCK);
    assert_eq!(
        call(&analysis, "Insert").args.len(),
        1,
        "the statement inside the if keeps its call"
    );
    let for_each = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee.ends_with(".ForEach"))
        .unwrap_or_else(|| panic!("no ForEach call: {:?}", analysis.facts.calls));
    assert_eq!(
        for_each.args.len(),
        1,
        "ForEach({{ }}) keeps its one argument"
    );
    run_clean("$commands.ForEach{ $_.Name }\n$x = $a::Create{ 1 }\n");
}

/// Cluster PS-CIN: `-cin @(...)` and the other case-sensitive membership operators.
#[test]
fn the_case_sensitive_membership_operators_parse() {
    let analysis = run_clean(CIN_OPERATOR);
    assert_eq!(call(&analysis, "Write-Host").args.len(), 1);
    run_clean("$x -cnotin 1\n$y -iin $z\n$w -inotin $z\n");
}

/// Cluster PS-TERNARY: `$a ? b : c`.
#[test]
fn a_ternary_expression_parses_with_a_quoted_path_in_each_branch() {
    let analysis = run_clean(TERNARY);
    assert_eq!(call(&analysis, "Write-Host").args.len(), 1);
    run_clean("$u = $a ? 1 : $b ? 2 : 3\n");
}

#[test]
fn a_bitwise_or_may_appear_inside_method_arguments() {
    let analysis = run_clean(BOR_ARGUMENT);
    assert!(
        analysis
            .facts
            .calls
            .iter()
            .any(|c| c.callee.ends_with(".GetMethod") && c.args.len() == 2),
        "GetMethod keeps its two arguments: {:?}",
        analysis.facts.calls
    );
}

#[test]
fn a_loop_label_may_be_a_bare_name() {
    let analysis = run_clean(LOOP_LABEL);
    let names: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert_eq!(names, ["Invoke-StrayBreak"]);
}

#[test]
fn an_else_on_the_line_after_an_if_in_an_assignment_is_part_of_the_if() {
    let analysis = run_clean(ELSE_AFTER_ASSIGNMENT);
    assert!(
        analysis.facts.calls.iter().all(|c| c.callee != "else"),
        "`else` is a keyword, not a command: {:?}",
        analysis.facts.calls
    );
    assert_eq!(call(&analysis, "Write-Host").args.len(), 1);
}

#[test]
fn a_backtick_continuation_may_end_the_line_before_a_closing_brace() {
    let analysis = run_clean(BACKTICK_CONTINUATION);
    let names: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert_eq!(names, ["Get-ShortType"]);
    assert!(
        analysis.facts.calls.iter().all(|c| c.callee != "else"),
        "the else branch after the continuation is not a command"
    );
}

#[test]
fn a_hash_sign_after_an_escape_or_a_variable_in_a_string_is_text() {
    run_clean(HASH_IN_STRING);
    run_clean("$msg = \"x $a ## y\"\n");
}

/// Runs a tree-sitter query over `source` and returns the number of `@match` captures.
fn query_hits(source: &str, query: &str) -> usize {
    let spec = CustomRuleSpec {
        id: "custom/gap-probe".to_owned(),
        language: "powershell".to_owned(),
        severity: Severity::Warn,
        message: "probe".to_owned(),
        why: "probe".to_owned(),
        fix: "probe".to_owned(),
        query: query.to_owned(),
        include: Vec::new(),
        exclude: Vec::new(),
    };
    let rules = CustomRules::compile(&[spec]).expect("probe query compiles");
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "gap.ps1",
        source,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &rules);
    assert!(
        analysis.parse_error.is_none(),
        "must parse cleanly: {:?}",
        analysis.parse_error
    );
    analysis.custom_hits.len()
}

#[test]
fn a_here_string_line_may_start_with_an_escape_pair() {
    let analysis = run_clean(HERE_STRING_ESCAPES);
    assert_eq!(call(&analysis, "Write-Output").args.len(), 1);
    // The escaped dollars at the start of lines are text, not variables. The stock grammar
    // swallowed the backtick into the line break and produced two `variable` nodes here.
    assert_eq!(
        query_hits(
            HERE_STRING_ESCAPES,
            "(expandable_here_string_literal (variable) @match)"
        ),
        0,
        "no variable node inside the here-string"
    );
    // Control: a real variable in a here-string is still a variable node.
    assert_eq!(
        query_hits(
            "Write-Output @\"\nvalue $x\n\"@\n",
            "(expandable_here_string_literal (variable) @match)"
        ),
        1,
        "an unescaped variable keeps its node"
    );
    run_clean(HERE_STRING_BACKTICKS);
}

#[test]
fn invalid_powershell_still_reports_a_parse_error() {
    for source in [
        "function f {\n",
        "if ($x {\n    1\n}\n",
        "switch ($x) { \"^(a)$\" { 1 }\n",
        "Select-Object Name,Length )\n",
        "begin {\n",
        "foo -I=binary }\n",
        "$a.Foo {1}\n",
        "[Flags()] function f { }\n",
        "$t = $a ? 1\n",
        "function f { param($a) 1\n",
        "$a.Where{ 1\n",
        "if ($v -cin) { 1 }\n",
        "enum E { A\n",
        "foo -- (\n",
        "$h = @{ K = { }\n",
        "$x = [Dictionary[[string], [int]]\n",
        "$a.Where{ 1 } }\n",
    ] {
        let analysis = run(source);
        assert!(
            analysis.parse_error.is_some(),
            "invalid source must not parse cleanly:\n{source}"
        );
        assert!(
            analysis.facts.functions.is_empty() && analysis.facts.calls.is_empty(),
            "a failed parse yields no facts for:\n{source}"
        );
    }
}
