//! Literal word and comment ownership at PowerShell separators.

use super::powershell::run_clean;
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
