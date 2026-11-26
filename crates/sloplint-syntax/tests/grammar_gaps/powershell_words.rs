//! Argument boundaries retain protected text and validate interpolation.

use sloplint_core::facts::ArgShape;

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
        "Copy-Item $PSScriptRoot/\"file$.txt\" $dest\n",
        "Copy-Item $PSScriptRoot/\"#1.zip\" $dest\n",
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

#[test]
fn crlf_after_an_escape_inside_a_bare_argument_ends_the_command() {
    for indentation in ["", "    "] {
        let source = format!("Write-Output hello`\r\n{indentation}Get-Date\r\n");
        let analysis = run_clean(&source);
        let callees = analysis
            .facts
            .calls
            .iter()
            .map(|call| call.callee)
            .collect::<Vec<_>>();
        assert_eq!(callees, ["Write-Output", "Get-Date"], "{source:?}");
    }
}

#[test]
fn here_string_dollar_does_not_consume_the_closing_line() {
    for body in ["^\\d+$", "$ ", "$\t", "$\\", "$`n", "$`$"] {
        for ending in ["\n", "\r\n"] {
            let source =
                format!("$p = @\"{ending}{body}{ending}\"@{ending}Write-Output done{ending}");
            let analysis = run_clean(&source);
            assert_eq!(call(&analysis, "Write-Output").args.len(), 1, "{source:?}");
        }
    }
    for body in ["${name", "${}", "$(Get-Date"] {
        let source = format!("$p = @\"\n{body}\n\"@\n");
        assert!(run(&source).parse_error.is_some(), "{source:?}");
    }
}

#[test]
fn leading_pipes_require_an_adjacent_nonblank_line() {
    for ending in ["\n", "\r\n"] {
        for space in ["", " ", "\t"] {
            run_clean(&format!("Get-Date{ending}{space}| Write-Output{ending}"));
            let source = format!("Get-Date{ending}{space}{ending}| Write-Output{ending}");
            assert!(run(&source).parse_error.is_some(), "{source:?}");
        }
        run_clean(&format!(
            "if ($true) {{ 1 }}{ending}{ending}else {{ 2 }}{ending}"
        ));
    }
}

#[test]
fn here_string_newlines_preserve_interpolation_and_escape_openers() {
    for ending in ["\n", "\r\n"] {
        for second_line in [
            "${name}",
            "$name",
            "$(Get-Date)",
            "`$",
            "`\"@",
            "``````PowerShell",
            "# literal text",
            "\"literal quote",
        ] {
            let source = format!(
                "$p = @\"{ending}end${ending}{second_line}{ending}\"@{ending}Write-Output done{ending}"
            );
            let analysis = run_clean(&source);
            assert_eq!(call(&analysis, "Write-Output").args.len(), 1, "{source:?}");
        }
        for second_line in ["${}", "${name", "$(Get-Date"] {
            let source = format!("$p = @\"{ending}end${ending}{second_line}{ending}\"@{ending}");
            assert!(run(&source).parse_error.is_some(), "{source:?}");
        }
    }
}

#[test]
fn parenthesized_pipelines_reject_a_leading_pipe_after_a_blank_line() {
    for (prefix, suffix) in [
        ("(", ")"),
        ("if (", ") { 1 }"),
        ("if ($false) { 0 } elseif (", ") { 1 }"),
        ("while (", ") { break }"),
        ("do { 1 } until (", ")"),
        ("foreach ($item in ", ") { $item }"),
        ("switch (", ") { default { 1 } }"),
        ("for (", ";;) { break }"),
    ] {
        for ending in ["\n", "\r\n"] {
            for space in ["", " ", "\t"] {
                let adjacent =
                    format!("{prefix}Get-Date{ending}{space}| Write-Output{suffix}{ending}");
                run_clean(&adjacent);
                let blank = format!(
                    "{prefix}Get-Date{ending}{space}{ending}| Write-Output{suffix}{ending}"
                );
                assert!(run(&blank).parse_error.is_some(), "{blank:?}");
            }
        }
    }
}

#[test]
fn variable_path_literals_use_powershell_escape_rules() {
    for (source, searchable) in [
        (r#"Write-Host $r\bin\deploy\"$id""#, true),
        (r#"Write-Host $r\x\"$id""#, false),
        (r#"Write-Host "$r\bin\deploy\$id""#, true),
        (r#"Write-Host "$r\x\$id""#, false),
    ] {
        let analysis = run_clean(source);
        let arguments = &call(&analysis, "Write-Host").args;
        assert_eq!(arguments.len(), 1, "{source}");
        let ArgShape::Concat { longest_static } = arguments[0].shape else {
            panic!("expected an interpolated path: {source}");
        };
        assert_eq!(
            longest_static >= 8,
            searchable,
            "{source}: {longest_static}"
        );
    }
}

#[test]
fn grouped_pipelines_continue_across_comment_and_nbsp_lines() {
    // A comment-only line between the command and its leading pipe still
    // continues the pipeline, as does nonbreaking-space indentation. An
    // actual blank line ends it.
    run_clean("if (Get-Date\n# Filter below\n| Select-Object -First 1) {}\n");
    run_clean("(Get-Date\n\u{a0}| Write-Output)\n");
    assert!(run("(Get-Date\n\n| Write-Output)\n").parse_error.is_some());
}

#[test]
fn bare_words_discount_inline_variables() {
    // v$VERSION.tar.gz emits no interpolation child, but the word still
    // interpolates: only the literal runs count.
    let analysis = run_clean("Write-Host v$VERSION.tar.gz\n");
    let arguments = &call(&analysis, "Write-Host").args;
    assert_eq!(arguments.len(), 1);
    let ArgShape::Concat { longest_static } = arguments[0].shape else {
        panic!("expected interpolated word: {:?}", arguments[0].shape);
    };
    assert_eq!(longest_static, 7, ".tar.gz is the longest literal run");
    // No dollar, no interpolation: the whole word stays a plain literal.
    let analysis = run_clean("Write-Host hello\n");
    let ArgShape::Str { longest_static } = call(&analysis, "Write-Host").args[0].shape else {
        panic!("expected plain literal");
    };
    assert_eq!(longest_static, 5);
}
