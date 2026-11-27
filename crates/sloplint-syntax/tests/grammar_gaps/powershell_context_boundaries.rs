//! Native conditional ownership and public separator boundaries.

use super::powershell::{call, run_clean};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_core::{Confidence, CustomRuleSpec, Severity};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn captures(source: &str, query: &str) -> Vec<(u32, String)> {
    let spec = CustomRuleSpec {
        id: "custom/context-boundary".to_owned(),
        language: "powershell".to_owned(),
        severity: Severity::Info,
        confidence: Confidence::High,
        message: "Captured {match}".to_owned(),
        why: "Preserve public query boundaries.".to_owned(),
        fix: "Inspect the captured source.".to_owned(),
        query: query.to_owned(),
        include: Vec::new(),
        exclude: Vec::new(),
    };
    let rules = CustomRules::compile(&[spec]).expect("boundary query compiles");
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "context.ps1",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &rules);
    assert!(analysis.parse_error.is_none(), "{source:?}");
    assert!(!analysis.timed_out && analysis.custom_capped.is_empty());
    analysis
        .custom_hits
        .iter()
        .map(|hit| {
            let text = hit
                .captures
                .iter()
                .find(|(name, _)| *name == "match")
                .expect("query returns its match capture")
                .1;
            (hit.pos.byte, text.to_owned())
        })
        .collect()
}

fn gaps(ending: &str) -> [String; 4] {
    [
        ending.to_owned(),
        format!("{ending}\x60{ending}"),
        format!("{ending}\x60{ending}<# note #> "),
        format!("{ending}\x60{ending}# note{ending}"),
    ]
}

#[test]
fn ordinary_commands_keep_keyword_like_following_calls_separate() {
    for ending in ["\n", "\r\n"] {
        for gap in gaps(ending) {
            for (tail, arguments) in [
                ("else.exe tail", 1),
                ("elseif.exe tail", 1),
                ("else/path tail", 1),
                ("elseif:path tail", 1),
                ("else#name tail", 1),
                ("else\"quoted\" tail", 1),
                ("else\x60x tail", 1),
                ("else$tail tail", 1),
                ("else\u{200b} tail", 1),
                ("elseé tail", 1),
                ("else { 0 }", 1),
                ("elseif ($false) { 0 }", 2),
                ("else", 0),
                ("elseif", 0),
                ("else tail", 1),
                ("elseif tail", 1),
            ] {
                let source = format!("Capture-Native hello{gap}{tail}{ending}");
                let analysis = run_clean(&source);
                assert_eq!(analysis.facts.calls.len(), 2, "{source:?}");
                assert_eq!(
                    call(&analysis, "Capture-Native").args.len(),
                    1,
                    "{source:?}"
                );
                let following = analysis
                    .facts
                    .calls
                    .iter()
                    .find(|call| call.callee != "Capture-Native")
                    .expect("native second command stays separate");
                assert_eq!(following.args.len(), arguments, "{source:?}");
            }
        }
    }
}

#[test]
fn keyword_prefixes_do_not_extend_completed_conditionals() {
    for ending in ["\n", "\r\n"] {
        for gap in gaps(ending) {
            for tail in [
                "else.exe tail",
                "elseif.exe tail",
                "else/path tail",
                "elseif:path tail",
                "else#name tail",
                "else\"quoted\" tail",
                "else\x60x tail",
                "else$tail tail",
                "else\u{200b} tail",
                "elseé tail",
            ] {
                let source = format!("$result = if ($true) {{ 1 }}{gap}{tail}{ending}");
                let analysis = run_clean(&source);
                assert_eq!(analysis.facts.calls.len(), 1, "{source:?}");
                assert_eq!(analysis.facts.calls[0].args.len(), 1, "{source:?}");
                assert!(
                    captures(&source, "(else_clause) @match").is_empty(),
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn owned_branches_keep_public_fields_and_keyword_ranges() {
    for ending in ["\n", "\r\n"] {
        for (keyword, tail, field, node) in [
            ("else", "else { 0 }", "else_clause", "else_clause"),
            (
                "elseif",
                "elseif ($false) { 0 }",
                "elseif_clauses",
                "elseif_clauses",
            ),
        ] {
            let source = format!(
                "$result = if ($true) {{ 1 }}{ending}\x60{ending}<# note #> {tail}{ending}"
            );
            run_clean(&source);
            let byte = u32::try_from(source.find(tail).expect("branch in source"))
                .expect("small source offset");
            assert_eq!(
                captures(&source, &format!("(if_statement {field}: ({node}) @match)")),
                vec![(byte, tail.to_owned())]
            );
            assert_eq!(
                captures(&source, &format!("\"{keyword}\" @match")),
                vec![(byte, keyword.to_owned())]
            );
        }
    }
}

#[test]
fn mixed_case_branches_keep_their_native_ownership() {
    for (head, middle, tail) in [("iF", "eLsEiF", "ELSE"), ("IF", "ELSEIF", "eLsE")] {
        let source = format!(
            "$result = {head} ($true) {{ 1 }}\n\x60\n<# note #> {middle} ($false) {{ 2 }}\n{tail} {{ 3 }}\n"
        );
        let analysis = run_clean(&source);
        assert!(analysis.facts.calls.is_empty(), "{source:?}");
        for (keyword, node) in [(middle, "elseif_clause"), (tail, "else_clause")] {
            let byte = u32::try_from(source.find(keyword).expect("keyword in source"))
                .expect("small source offset");
            let query = format!("({node} \"{}\" @match)", keyword.to_ascii_lowercase());
            assert_eq!(captures(&source, &query), vec![(byte, keyword.to_owned())]);
        }
    }
}

#[test]
fn incomplete_owned_branches_remain_syntax_errors() {
    let language: tree_sitter::Language = tree_sitter_powershell::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("powershell language loads");
    for ending in ["\n", "\r\n"] {
        for gap in gaps(ending) {
            for tail in ["else", "elseif", "else tail", "elseif tail"] {
                let source = format!("$result = if ($true) {{ 1 }}{gap}{tail}{ending}");
                let tree = parser.parse(&source, None).expect("parser returns a tree");
                assert!(tree.root_node().has_error(), "{source:?}");
            }
        }
    }
}

#[test]
fn indented_bare_dashes_follow_separated_line_continuations() {
    for (ending_name, ending) in [("lf", "\n"), ("crlf", "\r\n")] {
        for (before_name, before) in [
            ("space", " "),
            ("tab", "\t"),
            ("nbsp", "\u{a0}"),
            ("em", "\u{2003}"),
        ] {
            for (indent_name, indent) in [
                ("space", " "),
                ("spaces", "  "),
                ("tab", "\t"),
                ("nbsp", "\u{a0}"),
                ("em", "\u{2003}"),
                ("mixed", " \t\u{a0}\u{2003}"),
            ] {
                for (dash_count, dashes) in [(1, "-"), (2, "--")] {
                    let source = format!(
                        "Capture-Native before{before}\x60{ending}{indent}{dashes} after{ending}"
                    );
                    let analysis = run_clean(&source);
                    assert_eq!(analysis.facts.calls.len(), 1, "{source:?}");
                    let count = call(&analysis, "Capture-Native").args.len();
                    assert_eq!(count, 3, "{source:?}");
                    println!(
                        "NATIVE_CONTINUED_DASH_CASE continued-{ending_name}-{before_name}-{indent_name}-{dash_count} {count}"
                    );
                }
            }
        }
    }
}

#[test]
fn spaced_dash_redirection_retains_its_legacy_separator_capture() {
    let source = "Capture-Native hello > -output.log\n";
    assert_eq!(
        captures(
            source,
            "(redirected_file_name (command_argument_sep \" \" @match))"
        ),
        vec![(22, " ".to_owned())]
    );
}

#[test]
fn legacy_redirect_targets_keep_spacing_outside_the_word() {
    for operator in [">", ">>", "2>"] {
        for gap in [" ", "  ", "\t", "\u{a0}", " \t\u{a0} ", "\u{2003}"] {
            // This freezes the existing query contract. Native PowerShell splits
            // the single-dash target before ".log"; that older parser gap is separate.
            for target in ["-output.log", "--output.log", "output.log"] {
                let source = format!("Capture-Native hello {operator}{gap}{target}\n");
                let byte =
                    u32::try_from(21 + operator.len() + gap.len()).expect("small source offset");
                assert_eq!(
                    captures(&source, "(redirected_file_name (generic_token) @match)"),
                    vec![(byte, target.to_owned())],
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn mixed_native_spaces_keep_a_single_separator() {
    for gap in [" \u{2003}", "\u{2003}\t"] {
        let source = format!("Capture-Native hello{gap}world\n");
        let analysis = run_clean(&source);
        assert_eq!(call(&analysis, "Capture-Native").args.len(), 2);
        assert_eq!(
            captures(&source, "(command_elements (command_argument_sep) @match)"),
            vec![(14, " ".to_owned()), (20, gap.to_owned())]
        );
        let source = format!("Capture-Native hello >{gap}output.log\n");
        assert_eq!(
            captures(
                &source,
                "(redirected_file_name (command_argument_sep) @match)"
            ),
            vec![(22, gap.to_owned())]
        );
    }
}
