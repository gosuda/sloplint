//! Keep the published custom-query contract for legacy separator tokens.

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_core::{Confidence, CustomRuleSpec, Severity};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

#[test]
fn legacy_separator_queries_keep_original_capture_spans() {
    for separator in [' ', '\t', '\u{a0}'] {
        let spec = CustomRuleSpec {
            id: "custom/separator".to_owned(),
            language: "powershell".to_owned(),
            severity: Severity::Info,
            confidence: Confidence::High,
            message: "Separator {match}".to_owned(),
            why: "Keep the existing anonymous token query contract.".to_owned(),
            fix: "Inspect the captured separator.".to_owned(),
            query: format!("\"{separator}\" @match"),
            include: Vec::new(),
            exclude: Vec::new(),
        };
        let rules = CustomRules::compile(&[spec]).expect("legacy separator query compiles");
        // These whole-gap spans were frozen on the pre-change grammar.
        // Both mixed controls were captured under its anonymous space token.
        for (gap, token) in [
            (" ", ' '),
            ("\t", '\t'),
            ("\u{a0}", '\u{a0}'),
            ("  ", ' '),
            ("\t\t", '\t'),
            ("\u{a0}\u{a0}", '\u{a0}'),
            (" \t\u{a0} ", ' '),
            ("\u{a0}\t ", ' '),
        ] {
            let source = format!("Capture-Native{gap}hello{gap}world\n");
            let input = FileInput {
                lang: Lang::PowerShell,
                path: "legacy.ps1",
                source: &source,
                role: CodeRole::Library,
            };
            let analysis = analyze(&mut Parsers::new(), &input, &rules);
            assert!(analysis.parse_error.is_none(), "{source:?}");
            assert!(!analysis.timed_out && analysis.custom_capped.is_empty());
            let actual: Vec<_> = analysis
                .custom_hits
                .iter()
                .map(|hit| {
                    let captured = hit
                        .captures
                        .iter()
                        .find(|(name, _)| *name == "match")
                        .expect("query returns its match capture")
                        .1;
                    (hit.pos.byte, captured)
                })
                .collect();
            let expected = if separator == token {
                vec![
                    (14, gap),
                    (
                        u32::try_from(19 + gap.len()).expect("small fixture offset"),
                        gap,
                    ),
                ]
            } else {
                Vec::new()
            };
            assert_eq!(
                actual,
                expected,
                "{source:?}, U+{:04X}",
                u32::from(separator)
            );
            println!(
                "LEGACY_SEPARATOR_QUERY U+{:04X} {:?} {:?}",
                u32::from(separator),
                gap,
                actual
            );
        }
    }
}
