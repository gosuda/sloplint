//! Valid PowerShell that stock tree-sitter-powershell 0.26.4 reports as a syntax error.
//!
//! The sources are slices of real scripts (named per constant) or the shape one of them
//! uses. The vendored grammar in `vendor/tree-sitter-powershell` must parse each one with
//! no error node, must still reject invalid text, and must keep the node shapes the
//! adapter reads.

use std::sync::LazyLock;

use sloplint_core::facts::{ArgShape, Call};
use sloplint_core::lang::Lang;
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

fn run_clean<'s>(source: &'s str) -> Analysis<'s> {
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

#[test]
fn invalid_powershell_still_reports_a_parse_error() {
    for source in [
        "function f {\n",
        "if ($x {\n    1\n}\n",
        "switch ($x) { \"^(a)$\" { 1 }\n",
        "Select-Object Name,Length )\n",
        "begin {\n",
        "foo -I=binary }\n",
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
