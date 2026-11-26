//! Argument boundaries retain protected text and validate interpolation.

use super::powershell::{call, run, run_clean};

#[test]
fn command_comma_lists_keep_parameter_words_and_parenthesized_items_together() {
    for source in [
        "Write-Output --:first,-second\n",
        "Write-Output --:first, -second\n",
    ] {
        let analysis = run_clean(source);
        assert_eq!(call(&analysis, "Write-Output").args.len(), 1, "{source}");
    }
    let analysis = run_clean("Write-Output --(Get-Date), (Get-Date)\n");
    assert_eq!(
        call(&analysis, "Write-Output").args.len(),
        2,
        "the end-of-options marker and one array argument"
    );
}

#[test]
fn nbsp_separates_native_words_but_preserves_quoted_and_escaped_segments() {
    for source in [
        "git add --\u{a0}first.txt\u{a0}second.txt\n",
        "git add --\u{a0}--format=zip\u{a0}second.txt\n",
        "git add --\u{a0}\"first\u{a0}part\"\u{a0}second.txt\n",
        "git add --\u{a0}first`\u{a0}part\u{a0}second.txt\n",
    ] {
        let analysis = run_clean(source);
        assert_eq!(call(&analysis, "git").args.len(), 4, "{source}");
    }
}

#[test]
fn variable_paths_keep_protected_nbsp_inside_one_argument() {
    for (source, count) in [
        ("git add -- $env:TEMP/\"first\u{a0}part\"\n", 3),
        ("git add -- $env:TEMP/first`\u{a0}part\n", 3),
        ("git add -- $env:TEMP/first\u{a0}part\n", 4),
    ] {
        let analysis = run_clean(source);
        assert_eq!(call(&analysis, "git").args.len(), count, "{source}");
    }
}

#[test]
fn nbsp_delimits_command_names_outside_protected_segments() {
    let analysis = run_clean("git\u{a0}status\n");
    assert_eq!(call(&analysis, "git").args.len(), 1);
    for source in ["git\"first\u{a0}part\"\n", "git`\u{a0}status\n"] {
        let analysis = run_clean(source);
        assert_eq!(analysis.facts.calls.len(), 1, "{source}");
        assert_eq!(analysis.facts.calls[0].args.len(), 0, "{source}");
    }
}

#[test]
fn variable_paths_parse_expandable_quoted_subexpressions() {
    for source in [
        "Copy-Item $PSScriptRoot/\"$(Get-Date)\" $dest\n",
        "Copy-Item $PSScriptRoot/\"`$(Get-Date\" $dest\n",
        "Copy-Item $PSScriptRoot/\"$(Get-Date -Format 'yyyy')\" $dest\n",
        "Copy-Item $PSScriptRoot/\"first\u{a0}$(Get-Date)\" $dest\n",
        "Copy-Item $PSScriptRoot/'$(Get-Date' $dest\n",
    ] {
        let analysis = run_clean(source);
        assert_eq!(call(&analysis, "Copy-Item").args.len(), 2, "{source}");
    }
    for source in [
        "Copy-Item $PSScriptRoot/\"$(Get-Date\" $dest\n",
        "Copy-Item $PSScriptRoot/\"first $(Get-Date\" $dest\n",
    ] {
        assert!(run(source).parse_error.is_some(), "{source}");
    }
}

#[test]
fn command_continuations_keep_pipelines_with_lf_and_crlf() {
    for ending in ["\n", "\r\n"] {
        let source = format!("Get-Date`{ending}| Write-Output{ending}");
        let analysis = run_clean(&source);
        assert!(
            analysis
                .facts
                .calls
                .iter()
                .any(|call| call.callee == "Get-Date")
        );
        assert!(
            analysis
                .facts
                .calls
                .iter()
                .any(|call| call.callee == "Write-Output")
        );
    }
}
