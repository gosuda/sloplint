//! Literal word and comment ownership at PowerShell separators.

use super::powershell::{call, run_clean};
use sloplint_core::facts::ArgShape;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_core::{Confidence, CustomRuleSpec, Severity};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn captures(source: &str, query: &str) -> Vec<(u32, String)> {
    let spec = CustomRuleSpec {
        id: "custom/separator-ownership".to_owned(),
        language: "powershell".to_owned(),
        severity: Severity::Info,
        confidence: Confidence::High,
        message: "Captured {match}".to_owned(),
        why: "Preserve literal words and public comment ownership.".to_owned(),
        fix: "Inspect the captured source.".to_owned(),
        query: query.to_owned(),
        include: Vec::new(),
        exclude: Vec::new(),
    };
    let rules = CustomRules::compile(&[spec]).expect("ownership query compiles");
    let input = FileInput { lang: Lang::PowerShell, path: "ownership.ps1", source, role: CodeRole::Library };
    let analysis = analyze(&mut Parsers::new(), &input, &rules);
    assert!(analysis.parse_error.is_none() && !analysis.partial && !analysis.timed_out, "{source:?}");
    analysis.custom_hits.iter().map(|hit| {
        let text = hit.captures.iter().find(|(name, _)| *name == "match").expect("match capture").1;
        (hit.pos.byte, text.to_owned())
    }).collect()
}

#[test]
fn literal_format_marks_stay_in_redirect_words() {
    for mark in ['\u{200b}', '\u{feff}', '\u{2060}'] {
        for dashes in ["-", "--"] {
            for ending in ["\n", "\r\n"] {
                let target = format!("{mark}{dashes}");
                let source = format!("Write-Output hi > {target}{ending}");
                run_clean(&source);
                assert_eq!(
                    captures(&source, "(redirected_file_name (generic_token) @match)"),
                    vec![(18, target)],
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn inline_comment_before_named_parameter_keeps_its_parent() {
    let source = "Write-Output hi <# note #> -NoEnumerate\n";
    run_clean(source);
    assert_eq!(
        captures(source, "(command_elements (comment) @match)"),
        vec![(16, "<# note #>".to_owned())]
    );
    for (_, text) in captures(source, "(command_elements (command_argument_sep) @match)") {
        assert!(!text.contains("<#"), "{text:?}");
    }
}


#[test]
fn block_comments_preserve_adjacent_word_boundary() {
    let source = "Write-Output <# note #>hello\n";
    run_clean(source);
    assert_eq!(
        captures(source, "(generic_token) @match"),
        vec![(23, "hello".to_owned())]
    );
}

#[test]
fn commented_redirect_keeps_its_numeric_target() {
    let source = "Write-Output hello > <# note #> 1\n";
    run_clean(source);
    assert_eq!(
        captures(source, "(redirection) @match"),
        vec![(19, "> <# note #> 1".to_owned())]
    );
}


#[test]
fn comments_keep_array_arguments_and_protected_commas() {
    for (mark_name, mark) in [("zwsp", '\u{200b}'), ("bom", '\u{feff}'), ("wj", '\u{2060}')] {
        for dashes in ["-", "--"] {
            for (mode, suffix) in [("array", "first,second"), ("quoted", "\"first,second\""), ("escaped", "first`,second")] {
                let word = format!("{dashes}{mark}{suffix}");
                let source = format!("Capture-Native hi <# note #> {word}\n");
                let analysis = run_clean(&source);
                let command = call(&analysis, "Capture-Native");
                assert_eq!(command.args.len(), 2, "{source:?}");
                assert_eq!(
                    captures(&source, "(command_elements (comment) @match)"),
                    vec![(18, "<# note #>".to_owned())],
                    "{source:?}"
                );
                let arrays = captures(&source, "(array_literal_expression) @match");
                if mode == "array" {
                    assert!(matches!(command.args[1].shape, ArgShape::Other), "{source:?}");
                    assert_eq!(arrays, vec![(29, word)], "{source:?}");
                } else {
                    assert!(matches!(command.args[1].shape, ArgShape::Str { .. }), "{source:?}");
                    assert!(arrays.is_empty(), "{source:?}");
                }
                println!("GUARD_NATIVE_ARG_CASE comment-guard-{mark_name}-{}-{mode} 2", dashes.len());
            }
        }
    }
}


#[test]
fn complete_dash_prefix_keeps_the_comment_boundary_before_an_escape() {
    for ending in ["\n", "\r\n"] {
        let source = format!("Capture-Native hi <# note #> ---`{ending} next{ending}");
        run_clean(&source);
        assert_eq!(
            captures(&source, "(command_elements (comment) @match)"),
            vec![(18, "<# note #>".to_owned())],
            "{source:?}"
        );
    }
}

#[test]
fn comments_before_attached_redirect_words_stay_outside_separators() {
    for comments in ["<# note #>", "<# one #><# two #>", "<# one\n two #>"] {
        let source = format!("Write-Output hi > {comments} -output.log\n");
        run_clean(&source);
        assert!(
            captures(&source, "(command_argument_sep (comment) @match)").is_empty(),
            "{source:?}"
        );
    }
}

#[test]
fn separated_comment_groups_keep_their_original_public_parents() {
    for ending in ["\n", "\r\n"] {
        let source = format!("Write-Output hi <# one #> \t<# two #> -NoEnumerate{ending}");
        run_clean(&source);
        assert_eq!(
            captures(&source, "(command_elements (comment) @match)"),
            vec![(27, "<# two #>".to_owned())],
            "{source:?}"
        );
        assert_eq!(
            captures(&source, "(command_argument_sep (comment) @match)"),
            vec![(16, "<# one #>".to_owned())],
            "{source:?}"
        );
        assert_eq!(
            captures(&source, "(command_elements (command_argument_sep) @match)"),
            vec![(12, " ".to_owned()), (15, " <# one #> \t".to_owned())],
            "{source:?}"
        );
    }
}
