//! Custom queries run only against files of the language they were compiled for,
//! and the hits borrow capture text from the file they matched.

use sloplint_core::lang::Lang;
use sloplint_core::{CustomRuleSpec, Severity};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn spec(id: &str, language: &str, query: &str) -> CustomRuleSpec {
    CustomRuleSpec {
        id: id.to_string(),
        language: language.to_string(),
        severity: Severity::Warn,
        message: "call".to_string(),
        why: "w".to_string(),
        fix: "f".to_string(),
        query: query.to_string(),
        include: Vec::new(),
        exclude: Vec::new(),
    }
}

fn rust_and_python() -> Vec<CustomRuleSpec> {
    vec![
        spec(
            "custom/rust-call",
            "rust",
            "(call_expression function: (identifier) @callee) @match",
        ),
        spec(
            "custom/python-call",
            "python",
            "(call function: (identifier) @callee) @match",
        ),
    ]
}

fn callee_hits(custom: &CustomRules, lang: Lang, path: &str, source: &str) -> Vec<(u32, String)> {
    let mut parsers = Parsers::new();
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut parsers, &input, custom);
    assert!(analysis.parse_error.is_none(), "{path} must parse");
    analysis
        .custom_hits
        .iter()
        .map(|hit| {
            let callee = hit
                .captures
                .iter()
                .find(|(name, _)| *name == "callee")
                .map(|(_, text)| (*text).to_string())
                .unwrap_or_else(|| panic!("{path}: hit lacks the callee capture"));
            (hit.rule, callee)
        })
        .collect()
}

#[test]
fn a_rule_set_without_rules_produces_no_hits() {
    let custom = CustomRules::compile(&[]).unwrap_or_else(|e| panic!("empty rules: {e}"));
    let hits = callee_hits(&custom, Lang::Rust, "a.rs", "fn main() { first(1); }\n");
    assert!(
        hits.is_empty(),
        "a rule set without rules must produce no hits, got {hits:?}"
    );
}

#[test]
fn each_file_only_sees_its_own_languages_rules() {
    let custom = CustomRules::compile(&rust_and_python())
        .unwrap_or_else(|e| panic!("valid rules failed to compile: {e}"));
    let rust = callee_hits(&custom, Lang::Rust, "a.rs", "fn main() { first(1); }\n");
    assert_eq!(rust, [(0, "first".to_string())]);
    let python = callee_hits(&custom, Lang::Python, "a.py", "second(2)\n");
    assert_eq!(python, [(1, "second".to_string())]);
}

#[test]
fn a_language_without_rules_gets_no_hits() {
    let custom = CustomRules::compile(&rust_and_python())
        .unwrap_or_else(|e| panic!("valid rules failed to compile: {e}"));
    let go = callee_hits(
        &custom,
        Lang::Go,
        "a.go",
        "package p\n\nfunc f() {\n\tfirst(1)\n}\n",
    );
    assert!(
        go.is_empty(),
        "a language without rules must get no hits, got {go:?}"
    );
}

#[test]
fn one_rules_table_serves_many_files_in_turn() {
    let custom = CustomRules::compile(&rust_and_python())
        .unwrap_or_else(|e| panic!("valid rules failed to compile: {e}"));
    for round in 0..3 {
        let first = callee_hits(&custom, Lang::Rust, "a.rs", "fn f() { alpha(); beta(); }\n");
        assert_eq!(
            first,
            [(0, "alpha".to_string()), (0, "beta".to_string())],
            "round {round}"
        );
        let second = callee_hits(&custom, Lang::Python, "b.py", "gamma()\n");
        assert_eq!(second, [(1, "gamma".to_string())], "round {round}");
    }
}
