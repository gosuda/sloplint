//! A continuation escape cannot reach backward across an ordinary newline.

use super::powershell::{call, run, run_clean};

#[test]
fn backtick_only_lines_do_not_bridge_an_ordinary_newline_to_a_pipe() {
    for (name, source) in [
        (
            "pipe-lf-ordinary-then-backtick-top",
            "Get-Date\n`\n| Write-Output\n",
        ),
        (
            "pipe-lf-ordinary-then-backtick-grouped",
            "(Get-Date\n`\n| Write-Output)\n",
        ),
        (
            "pipe-lf-ordinary-then-indented-backtick-top",
            "Get-Date\n  `\n| Write-Output\n",
        ),
        (
            "pipe-lf-ordinary-then-indented-backtick-grouped",
            "(Get-Date\n  `\n| Write-Output)\n",
        ),
        (
            "pipe-lf-ordinary-comment-backtick-top",
            "Get-Date\n# note\n`\n| Write-Output\n",
        ),
        (
            "pipe-lf-ordinary-comment-backtick-grouped",
            "(Get-Date\n# note\n`\n| Write-Output)\n",
        ),
        (
            "pipe-lf-ordinary-block-comment-backtick-top",
            "Get-Date\n<# note #>\n`\n| Write-Output\n",
        ),
        (
            "pipe-lf-ordinary-block-comment-backtick-grouped",
            "(Get-Date\n<# note #>\n`\n| Write-Output)\n",
        ),
        (
            "pipe-lf-ordinary-two-backticks-top",
            "Get-Date\n`\n`\n| Write-Output\n",
        ),
        (
            "pipe-lf-ordinary-two-backticks-grouped",
            "(Get-Date\n`\n`\n| Write-Output)\n",
        ),
        ("pipe-lf-ordinary-blank-top", "Get-Date\n\n| Write-Output\n"),
        (
            "pipe-lf-ordinary-blank-grouped",
            "(Get-Date\n\n| Write-Output)\n",
        ),
        (
            "pipe-crlf-ordinary-then-backtick-top",
            "Get-Date\r\n`\r\n| Write-Output\r\n",
        ),
        (
            "pipe-crlf-ordinary-then-backtick-grouped",
            "(Get-Date\r\n`\r\n| Write-Output)\r\n",
        ),
        (
            "pipe-crlf-ordinary-then-indented-backtick-top",
            "Get-Date\r\n  `\r\n| Write-Output\r\n",
        ),
        (
            "pipe-crlf-ordinary-then-indented-backtick-grouped",
            "(Get-Date\r\n  `\r\n| Write-Output)\r\n",
        ),
        (
            "pipe-crlf-ordinary-comment-backtick-top",
            "Get-Date\r\n# note\r\n`\r\n| Write-Output\r\n",
        ),
        (
            "pipe-crlf-ordinary-comment-backtick-grouped",
            "(Get-Date\r\n# note\r\n`\r\n| Write-Output)\r\n",
        ),
        (
            "pipe-crlf-ordinary-block-comment-backtick-top",
            "Get-Date\r\n<# note #>\r\n`\r\n| Write-Output\r\n",
        ),
        (
            "pipe-crlf-ordinary-block-comment-backtick-grouped",
            "(Get-Date\r\n<# note #>\r\n`\r\n| Write-Output)\r\n",
        ),
        (
            "pipe-crlf-ordinary-two-backticks-top",
            "Get-Date\r\n`\r\n`\r\n| Write-Output\r\n",
        ),
        (
            "pipe-crlf-ordinary-two-backticks-grouped",
            "(Get-Date\r\n`\r\n`\r\n| Write-Output)\r\n",
        ),
        (
            "pipe-crlf-direct-two-backticks-top",
            "Get-Date`\r\n`\r\n| Write-Output\r\n",
        ),
        (
            "pipe-crlf-direct-two-backticks-grouped",
            "(Get-Date`\r\n`\r\n| Write-Output)\r\n",
        ),
        (
            "pipe-crlf-ordinary-blank-top",
            "Get-Date\r\n\r\n| Write-Output\r\n",
        ),
        (
            "pipe-crlf-ordinary-blank-grouped",
            "(Get-Date\r\n\r\n| Write-Output)\r\n",
        ),
    ] {
        assert!(run(source).parse_error.is_some(), "{name}: {source:?}");
    }
}

#[test]
fn adjacent_and_explicit_continuation_pipes_remain_valid() {
    for source in [
        "Get-Date`\n| Write-Output\n",
        "(Get-Date`\n| Write-Output)\n",
        "Get-Date`\n`\n| Write-Output\n",
        "(Get-Date`\n`\n| Write-Output)\n",
        "Get-Date\n| Write-Output\n",
        "(Get-Date\n| Write-Output)\n",
        "Get-Date\n# note\n| Write-Output\n",
        "(Get-Date\n# note\n| Write-Output)\n",
        "Get-Date`\r\n| Write-Output\r\n",
        "(Get-Date`\r\n| Write-Output)\r\n",
        "Get-Date\r\n| Write-Output\r\n",
        "(Get-Date\r\n| Write-Output)\r\n",
        "Get-Date\r\n# note\r\n| Write-Output\r\n",
        "(Get-Date\r\n# note\r\n| Write-Output)\r\n",
    ] {
        run_clean(source);
    }
}

#[test]
fn attached_and_separated_mixed_newline_escapes_match_native_boundaries() {
    for (name, source, accepted) in [
        (
            "lf-lf-attached-none-top",
            "Get-Date`\n`\n| Write-Output\n",
            true,
        ),
        (
            "lf-lf-attached-none-group",
            "(Get-Date`\n`\n| Write-Output)\n",
            true,
        ),
        (
            "lf-lf-attached-space-top",
            "Get-Date`\n `\n| Write-Output\n",
            true,
        ),
        (
            "lf-lf-attached-space-group",
            "(Get-Date`\n `\n| Write-Output)\n",
            true,
        ),
        (
            "lf-lf-spaced-none-top",
            "Get-Date `\n`\n| Write-Output\n",
            true,
        ),
        (
            "lf-lf-spaced-none-group",
            "(Get-Date `\n`\n| Write-Output)\n",
            true,
        ),
        (
            "lf-lf-spaced-space-top",
            "Get-Date `\n `\n| Write-Output\n",
            true,
        ),
        (
            "lf-lf-spaced-space-group",
            "(Get-Date `\n `\n| Write-Output)\n",
            true,
        ),
        (
            "lf-crlf-attached-none-top",
            "Get-Date`\n`\r\n| Write-Output\r\n",
            true,
        ),
        (
            "lf-crlf-attached-none-group",
            "(Get-Date`\n`\r\n| Write-Output)\r\n",
            true,
        ),
        (
            "lf-crlf-attached-space-top",
            "Get-Date`\n `\r\n| Write-Output\r\n",
            true,
        ),
        (
            "lf-crlf-attached-space-group",
            "(Get-Date`\n `\r\n| Write-Output)\r\n",
            true,
        ),
        (
            "lf-crlf-spaced-none-top",
            "Get-Date `\n`\r\n| Write-Output\r\n",
            true,
        ),
        (
            "lf-crlf-spaced-none-group",
            "(Get-Date `\n`\r\n| Write-Output)\r\n",
            true,
        ),
        (
            "lf-crlf-spaced-space-top",
            "Get-Date `\n `\r\n| Write-Output\r\n",
            true,
        ),
        (
            "lf-crlf-spaced-space-group",
            "(Get-Date `\n `\r\n| Write-Output)\r\n",
            true,
        ),
        (
            "crlf-lf-attached-none-top",
            "Get-Date`\r\n`\n| Write-Output\n",
            false,
        ),
        (
            "crlf-lf-attached-none-group",
            "(Get-Date`\r\n`\n| Write-Output)\n",
            false,
        ),
        (
            "crlf-lf-attached-space-top",
            "Get-Date`\r\n `\n| Write-Output\n",
            false,
        ),
        (
            "crlf-lf-attached-space-group",
            "(Get-Date`\r\n `\n| Write-Output)\n",
            false,
        ),
        (
            "crlf-lf-spaced-none-top",
            "Get-Date `\r\n`\n| Write-Output\n",
            true,
        ),
        (
            "crlf-lf-spaced-none-group",
            "(Get-Date `\r\n`\n| Write-Output)\n",
            true,
        ),
        (
            "crlf-lf-spaced-space-top",
            "Get-Date `\r\n `\n| Write-Output\n",
            true,
        ),
        (
            "crlf-lf-spaced-space-group",
            "(Get-Date `\r\n `\n| Write-Output)\n",
            true,
        ),
        (
            "crlf-crlf-attached-none-top",
            "Get-Date`\r\n`\r\n| Write-Output\r\n",
            false,
        ),
        (
            "crlf-crlf-attached-none-group",
            "(Get-Date`\r\n`\r\n| Write-Output)\r\n",
            false,
        ),
        (
            "crlf-crlf-attached-space-top",
            "Get-Date`\r\n `\r\n| Write-Output\r\n",
            false,
        ),
        (
            "crlf-crlf-attached-space-group",
            "(Get-Date`\r\n `\r\n| Write-Output)\r\n",
            false,
        ),
        (
            "crlf-crlf-spaced-none-top",
            "Get-Date `\r\n`\r\n| Write-Output\r\n",
            true,
        ),
        (
            "crlf-crlf-spaced-none-group",
            "(Get-Date `\r\n`\r\n| Write-Output)\r\n",
            true,
        ),
        (
            "crlf-crlf-spaced-space-top",
            "Get-Date `\r\n `\r\n| Write-Output\r\n",
            true,
        ),
        (
            "crlf-crlf-spaced-space-group",
            "(Get-Date `\r\n `\r\n| Write-Output)\r\n",
            true,
        ),
        ("ordinary-lf-lf-top", "Get-Date\n`\n| Write-Output\n", false),
        (
            "ordinary-lf-lf-group",
            "(Get-Date\n`\n| Write-Output)\n",
            false,
        ),
        (
            "ordinary-lf-crlf-top",
            "Get-Date\n`\r\n| Write-Output\r\n",
            false,
        ),
        (
            "ordinary-lf-crlf-group",
            "(Get-Date\n`\r\n| Write-Output)\r\n",
            false,
        ),
        (
            "ordinary-crlf-lf-top",
            "Get-Date\r\n`\n| Write-Output\n",
            false,
        ),
        (
            "ordinary-crlf-lf-group",
            "(Get-Date\r\n`\n| Write-Output)\n",
            false,
        ),
        (
            "ordinary-crlf-crlf-top",
            "Get-Date\r\n`\r\n| Write-Output\r\n",
            false,
        ),
        (
            "ordinary-crlf-crlf-group",
            "(Get-Date\r\n`\r\n| Write-Output)\r\n",
            false,
        ),
    ] {
        if accepted {
            run_clean(source);
        } else {
            assert!(run(source).parse_error.is_some(), "{name}: {source:?}");
        }
    }
}

#[test]
fn an_attached_cr_escape_remains_in_the_native_command_name() {
    for source in [
        "Get-Date`\r\n| Write-Output\r\n",
        "(Get-Date`\r\n| Write-Output)\r\n",
    ] {
        let analysis = run_clean(source);
        assert_eq!(call(&analysis, "Get-Date`\r").args.len(), 0);
        call(&analysis, "Write-Output");
        assert!(
            analysis
                .facts
                .calls
                .iter()
                .all(|call| call.callee != "Get-Date")
        );
    }
}

#[test]
fn escaped_lines_preserve_native_else_and_elseif_structure() {
    let language: tree_sitter::Language = tree_sitter_powershell::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("powershell language loads");
    for ending in ["\n", "\r\n"] {
        for gap in [
            format!("{ending}`{ending}"),
            format!("{ending}# note{ending}`{ending}"),
            format!("{ending}  `{ending}"),
            format!("{ending}`{ending}`{ending}"),
        ] {
            for (tail, elseifs) in [
                ("else { Write-Output no }", 0),
                (
                    "elseif ($false) { Write-Output maybe } else { Write-Output no }",
                    1,
                ),
            ] {
                let source = format!("if ($true) {{ Write-Output yes }}{gap}{tail}{ending}");
                run_clean(&source);
                let tree = parser.parse(&source, None).expect("parser returns a tree");
                let sexp = tree.root_node().to_sexp();
                assert_eq!(
                    sexp.matches("(elseif_clause ").count(),
                    elseifs,
                    "{source:?}"
                );
                assert_eq!(sexp.matches("(else_clause ").count(), 1, "{source:?}");
            }
        }
    }
}
