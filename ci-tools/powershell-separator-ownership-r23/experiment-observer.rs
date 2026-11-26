//! Frozen public-query observations; installed only in native proof checkouts.

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_core::{Confidence, CustomRuleSpec, Severity};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

#[test]
fn observe_separator_ownership_queries() {
    let path = std::env::var("SLOPLINT_BOUNDARY_CASES").expect("frozen cases path");
    let cases = std::fs::read_to_string(path).expect("read frozen cases");
    for (query_name, query) in [
        ("direct-comment", "(command_elements (comment) @match)"),
        ("separator-comment", "(command_argument_sep (comment) @match)"),
        ("element-separator", "(command_elements (command_argument_sep) @match)"),
        ("all-comments", "(comment) @match"),
        ("parameter", "(command_parameter) @match"),
        ("all-words", "(generic_token) @match"),
        ("redirect-word", "(redirected_file_name (generic_token) @match)"),
        ("redirect-separator", "(redirected_file_name (command_argument_sep) @match)"),
        ("redirect-comment", "(redirected_file_name (comment) @match)"),
        ("all-separators", "(command_argument_sep) @match"),
        ("space", "(command_argument_sep \" \" @match)"),
        ("tab", "(command_argument_sep \"\\t\" @match)"),
        ("nbsp", "(command_argument_sep \"\u{a0}\" @match)"),
        ("redirect-space", "(redirected_file_name (command_argument_sep \" \" @match))"),
        ("redirect-tab", "(redirected_file_name (command_argument_sep \"\\t\" @match))"),
        ("redirect-nbsp", "(redirected_file_name (command_argument_sep \"\u{a0}\" @match))"),
    ] {
        let spec = CustomRuleSpec {
            id: "custom/ownership-observation".to_owned(),
            language: "powershell".to_owned(),
            severity: Severity::Info,
            confidence: Confidence::High,
            message: "Captured {match}".to_owned(),
            why: "Observe the public boundary contract.".to_owned(),
            fix: "Inspect the captured source.".to_owned(),
            query: query.to_owned(),
            include: Vec::new(),
            exclude: Vec::new(),
        };
        let rules = match CustomRules::compile(&[spec]) {
            Ok(rules) => rules,
            Err(error) => {
                println!("OWNERSHIP_QUERY_COMPILE {query_name} ERROR {error}");
                continue;
            }
        };
        println!("OWNERSHIP_QUERY_COMPILE {query_name} OK");
        for line in cases.lines() {
            let (id, hex) = line.split_once('\t').expect("id and source");
            assert!(hex.len().is_multiple_of(2));
            let bytes: Vec<u8> = hex.as_bytes().chunks_exact(2).map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).expect("hex pair"), 16).expect("hex byte")
            }).collect();
            let source = String::from_utf8(bytes).expect("UTF-8 source");
            let input = FileInput { lang: Lang::PowerShell, path: "ownership.ps1", source: &source, role: CodeRole::Library };
            let analysis = analyze(&mut Parsers::new(), &input, &rules);
            println!("OWNERSHIP_QUERY_ROW {id} {query_name} error={} partial={} timeout={} hits={}",
                analysis.parse_error.is_some(), analysis.partial, analysis.timed_out, analysis.custom_hits.len());
            for hit in &analysis.custom_hits {
                let captured = hit.captures.iter().find(|(name, _)| *name == "match").expect("match capture").1;
                print!("OWNERSHIP_QUERY_HIT {id} {query_name} {} ", hit.pos.byte);
                for byte in captured.bytes() { print!("{byte:02x}"); }
                println!();
            }
        }
    }
}
