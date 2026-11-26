//! Native command continuation and branch-context boundaries.

use super::powershell::{call, run_clean};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_core::{Confidence, CustomRuleSpec, Severity};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

#[test]
fn indented_bare_dashes_follow_separated_line_continuations() {
    for ending in ["\n", "\r\n"] {
        for indent in [
            "",
            " ",
            "  ",
            "\t",
            "\u{a0}",
            "\u{2003}",
            " \t\u{a0}\u{2003}",
        ] {
            for dashes in ["-", "--"] {
                let source =
                    format!("Capture-Native before \x60{ending}{indent}{dashes} after{ending}");
                let analysis = run_clean(&source);
                assert_eq!(analysis.facts.calls.len(), 1, "{source:?}");
                assert_eq!(
                    call(&analysis, "Capture-Native").args.len(),
                    3,
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn keyword_like_executable_after_an_escaped_line_is_a_separate_call() {
    for ending in ["\n", "\r\n"] {
        for name in ["else.exe", "elseif.exe"] {
            let source =
                format!("Capture-Native hello{ending}\x60{ending}<# note #> {name} tail{ending}");
            let analysis = run_clean(&source);
            assert_eq!(analysis.facts.calls.len(), 2, "{source:?}");
            assert_eq!(
                call(&analysis, "Capture-Native").args.len(),
                1,
                "{source:?}"
            );
            assert_eq!(call(&analysis, name).args.len(), 1, "{source:?}");
        }
    }
}

#[test]
fn spaced_dash_redirection_retains_its_legacy_separator_capture() {
    let spec = CustomRuleSpec {
        id: "custom/redirect-space".to_owned(),
        language: "powershell".to_owned(),
        severity: Severity::Info,
        confidence: Confidence::High,
        message: "Captured {match}".to_owned(),
        why: "Preserve the existing custom-query contract.".to_owned(),
        fix: "Inspect the captured source.".to_owned(),
        query: "(redirected_file_name (command_argument_sep \" \" @match))".to_owned(),
        include: Vec::new(),
        exclude: Vec::new(),
    };
    let rules = CustomRules::compile(&[spec]).expect("legacy redirect query compiles");
    let source = "Capture-Native hello > -output.log\n";
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "redirect.ps1",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &rules);
    assert!(analysis.parse_error.is_none());
    assert_eq!(analysis.custom_hits.len(), 1);
    assert_eq!(analysis.custom_hits[0].pos.byte, 22);
    assert_eq!(analysis.custom_hits[0].captures[0].1, " ");
}

#[test]
fn observe_public_redirection_queries() {
    for (query_name, query) in [
        (
            "separator",
            "(redirected_file_name (command_argument_sep) @match)",
        ),
        (
            "space",
            "(redirected_file_name (command_argument_sep \" \" @match))",
        ),
        (
            "tab",
            "(redirected_file_name (command_argument_sep \"\\t\" @match))",
        ),
        (
            "nbsp",
            "(redirected_file_name (command_argument_sep \"\u{a0}\" @match))",
        ),
        ("target", "(redirected_file_name (generic_token) @match)"),
    ] {
        let spec = CustomRuleSpec {
            id: "custom/redirect-observation".to_owned(),
            language: "powershell".to_owned(),
            severity: Severity::Info,
            confidence: Confidence::High,
            message: "Captured {match}".to_owned(),
            why: "Observe the original public-query contract.".to_owned(),
            fix: "Inspect the captured source.".to_owned(),
            query: query.to_owned(),
            include: Vec::new(),
            exclude: Vec::new(),
        };
        let rules = match CustomRules::compile(&[spec]) {
            Ok(rules) => rules,
            Err(error) => {
                println!("PUBLIC_QUERY_COMPILE {query_name} ERROR {error}");
                continue;
            }
        };
        println!("PUBLIC_QUERY_COMPILE {query_name} OK");
        for (id, source) in [
            (
                "redirect-out-space-dash",
                "Capture-Native hello > -output.log\n",
            ),
            (
                "redirect-out-space-double-dash",
                "Capture-Native hello > --output.log\n",
            ),
            (
                "redirect-out-space-plain",
                "Capture-Native hello > output.log\n",
            ),
            (
                "redirect-out-space-quoted",
                "Capture-Native hello > \"output file.log\"\n",
            ),
            (
                "redirect-out-spaces-dash",
                "Capture-Native hello >  -output.log\n",
            ),
            (
                "redirect-out-spaces-double-dash",
                "Capture-Native hello >  --output.log\n",
            ),
            (
                "redirect-out-spaces-plain",
                "Capture-Native hello >  output.log\n",
            ),
            (
                "redirect-out-spaces-quoted",
                "Capture-Native hello >  \"output file.log\"\n",
            ),
            (
                "redirect-out-tab-dash",
                "Capture-Native hello >\t-output.log\n",
            ),
            (
                "redirect-out-tab-double-dash",
                "Capture-Native hello >\t--output.log\n",
            ),
            (
                "redirect-out-tab-plain",
                "Capture-Native hello >\toutput.log\n",
            ),
            (
                "redirect-out-tab-quoted",
                "Capture-Native hello >\t\"output file.log\"\n",
            ),
            (
                "redirect-out-nbsp-dash",
                "Capture-Native hello > -output.log\n",
            ),
            (
                "redirect-out-nbsp-double-dash",
                "Capture-Native hello > --output.log\n",
            ),
            (
                "redirect-out-nbsp-plain",
                "Capture-Native hello > output.log\n",
            ),
            (
                "redirect-out-nbsp-quoted",
                "Capture-Native hello > \"output file.log\"\n",
            ),
            (
                "redirect-out-mixed-dash",
                "Capture-Native hello > \t  -output.log\n",
            ),
            (
                "redirect-out-mixed-double-dash",
                "Capture-Native hello > \t  --output.log\n",
            ),
            (
                "redirect-out-mixed-plain",
                "Capture-Native hello > \t  output.log\n",
            ),
            (
                "redirect-out-mixed-quoted",
                "Capture-Native hello > \t  \"output file.log\"\n",
            ),
            (
                "redirect-out-em-dash",
                "Capture-Native hello > -output.log\n",
            ),
            (
                "redirect-out-em-double-dash",
                "Capture-Native hello > --output.log\n",
            ),
            (
                "redirect-out-em-plain",
                "Capture-Native hello > output.log\n",
            ),
            (
                "redirect-out-em-quoted",
                "Capture-Native hello > \"output file.log\"\n",
            ),
            (
                "redirect-append-space-dash",
                "Capture-Native hello >> -output.log\n",
            ),
            (
                "redirect-append-space-double-dash",
                "Capture-Native hello >> --output.log\n",
            ),
            (
                "redirect-append-space-plain",
                "Capture-Native hello >> output.log\n",
            ),
            (
                "redirect-append-space-quoted",
                "Capture-Native hello >> \"output file.log\"\n",
            ),
            (
                "redirect-append-spaces-dash",
                "Capture-Native hello >>  -output.log\n",
            ),
            (
                "redirect-append-spaces-double-dash",
                "Capture-Native hello >>  --output.log\n",
            ),
            (
                "redirect-append-spaces-plain",
                "Capture-Native hello >>  output.log\n",
            ),
            (
                "redirect-append-spaces-quoted",
                "Capture-Native hello >>  \"output file.log\"\n",
            ),
            (
                "redirect-append-tab-dash",
                "Capture-Native hello >>\t-output.log\n",
            ),
            (
                "redirect-append-tab-double-dash",
                "Capture-Native hello >>\t--output.log\n",
            ),
            (
                "redirect-append-tab-plain",
                "Capture-Native hello >>\toutput.log\n",
            ),
            (
                "redirect-append-tab-quoted",
                "Capture-Native hello >>\t\"output file.log\"\n",
            ),
            (
                "redirect-append-nbsp-dash",
                "Capture-Native hello >> -output.log\n",
            ),
            (
                "redirect-append-nbsp-double-dash",
                "Capture-Native hello >> --output.log\n",
            ),
            (
                "redirect-append-nbsp-plain",
                "Capture-Native hello >> output.log\n",
            ),
            (
                "redirect-append-nbsp-quoted",
                "Capture-Native hello >> \"output file.log\"\n",
            ),
            (
                "redirect-append-mixed-dash",
                "Capture-Native hello >> \t  -output.log\n",
            ),
            (
                "redirect-append-mixed-double-dash",
                "Capture-Native hello >> \t  --output.log\n",
            ),
            (
                "redirect-append-mixed-plain",
                "Capture-Native hello >> \t  output.log\n",
            ),
            (
                "redirect-append-mixed-quoted",
                "Capture-Native hello >> \t  \"output file.log\"\n",
            ),
            (
                "redirect-append-em-dash",
                "Capture-Native hello >> -output.log\n",
            ),
            (
                "redirect-append-em-double-dash",
                "Capture-Native hello >> --output.log\n",
            ),
            (
                "redirect-append-em-plain",
                "Capture-Native hello >> output.log\n",
            ),
            (
                "redirect-append-em-quoted",
                "Capture-Native hello >> \"output file.log\"\n",
            ),
            (
                "redirect-error-space-dash",
                "Capture-Native hello 2> -output.log\n",
            ),
            (
                "redirect-error-space-double-dash",
                "Capture-Native hello 2> --output.log\n",
            ),
            (
                "redirect-error-space-plain",
                "Capture-Native hello 2> output.log\n",
            ),
            (
                "redirect-error-space-quoted",
                "Capture-Native hello 2> \"output file.log\"\n",
            ),
            (
                "redirect-error-spaces-dash",
                "Capture-Native hello 2>  -output.log\n",
            ),
            (
                "redirect-error-spaces-double-dash",
                "Capture-Native hello 2>  --output.log\n",
            ),
            (
                "redirect-error-spaces-plain",
                "Capture-Native hello 2>  output.log\n",
            ),
            (
                "redirect-error-spaces-quoted",
                "Capture-Native hello 2>  \"output file.log\"\n",
            ),
            (
                "redirect-error-tab-dash",
                "Capture-Native hello 2>\t-output.log\n",
            ),
            (
                "redirect-error-tab-double-dash",
                "Capture-Native hello 2>\t--output.log\n",
            ),
            (
                "redirect-error-tab-plain",
                "Capture-Native hello 2>\toutput.log\n",
            ),
            (
                "redirect-error-tab-quoted",
                "Capture-Native hello 2>\t\"output file.log\"\n",
            ),
            (
                "redirect-error-nbsp-dash",
                "Capture-Native hello 2> -output.log\n",
            ),
            (
                "redirect-error-nbsp-double-dash",
                "Capture-Native hello 2> --output.log\n",
            ),
            (
                "redirect-error-nbsp-plain",
                "Capture-Native hello 2> output.log\n",
            ),
            (
                "redirect-error-nbsp-quoted",
                "Capture-Native hello 2> \"output file.log\"\n",
            ),
            (
                "redirect-error-mixed-dash",
                "Capture-Native hello 2> \t  -output.log\n",
            ),
            (
                "redirect-error-mixed-double-dash",
                "Capture-Native hello 2> \t  --output.log\n",
            ),
            (
                "redirect-error-mixed-plain",
                "Capture-Native hello 2> \t  output.log\n",
            ),
            (
                "redirect-error-mixed-quoted",
                "Capture-Native hello 2> \t  \"output file.log\"\n",
            ),
            (
                "redirect-error-em-dash",
                "Capture-Native hello 2> -output.log\n",
            ),
            (
                "redirect-error-em-double-dash",
                "Capture-Native hello 2> --output.log\n",
            ),
            (
                "redirect-error-em-plain",
                "Capture-Native hello 2> output.log\n",
            ),
            (
                "redirect-error-em-quoted",
                "Capture-Native hello 2> \"output file.log\"\n",
            ),
            (
                "redirect-out-no-gap-plain",
                "Capture-Native hello >output.log\n",
            ),
            (
                "redirect-out-no-gap-quoted",
                "Capture-Native hello >\"output file.log\"\n",
            ),
        ] {
            let input = FileInput {
                lang: Lang::PowerShell,
                path: "redirect.ps1",
                source,
                role: CodeRole::Library,
            };
            let analysis = analyze(&mut Parsers::new(), &input, &rules);
            println!(
                "PUBLIC_QUERY_ROW {id} {query_name} error={} partial={} hits={}",
                analysis.parse_error.is_some(),
                analysis.partial,
                analysis.custom_hits.len()
            );
            for hit in &analysis.custom_hits {
                let captured = hit
                    .captures
                    .iter()
                    .find(|(name, _)| *name == "match")
                    .expect("match capture exists")
                    .1;
                print!("PUBLIC_QUERY_HIT {id} {query_name} {} ", hit.pos.byte);
                for byte in captured.bytes() {
                    print!("{byte:02x}");
                }
                println!();
            }
        }
    }
}
