//! Rules that are off in test files report nothing there, and no other rule is affected.

use std::collections::BTreeSet;

use sloplint_core::config::Config;
use sloplint_core::lang::Lang;
use sloplint_core::role::{CodeRole, Shift};
use sloplint_core::{CATALOG, Finding, RuleFilter, RuleSet};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = "use std::collections::*;\n\npub fn handle(a: bool, b: bool) {\n    assert!(a);\n}\n\n#[test]\nfn rejects_empty() {\n    assert!(true, \"nonempty\");\n}\n";

fn check_production_file(rules: &RuleSet) -> Vec<Finding> {
    check_with_role(rules, CodeRole::Library)
}

fn check_test_file(rules: &RuleSet) -> Vec<Finding> {
    check_with_role(rules, CodeRole::Test)
}

fn check_with_role(rules: &RuleSet, role: CodeRole) -> Vec<Finding> {
    let custom = CustomRules::compile(&[]).unwrap_or_else(|e| panic!("empty rules: {e}"));
    let mut parsers = Parsers::new();
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source: SOURCE,
        role,
    };
    let analysis = analyze(&mut parsers, &input, &custom);
    assert!(analysis.parse_error.is_none(), "the sample must parse");
    rules.check_file(&analysis.facts)
}

fn rule_ids(findings: &[Finding]) -> BTreeSet<&str> {
    findings.iter().map(|finding| &*finding.rule).collect()
}

fn off_in_tests() -> BTreeSet<&'static str> {
    CATALOG
        .iter()
        .filter(|info| info.roles.shift(CodeRole::Test) == Shift::Off)
        .map(|info| info.id)
        .collect()
}

#[test]
fn exempt_rules_report_nothing_in_test_files() {
    let rules = RuleSet::new(&Config::default(), &RuleFilter::default())
        .unwrap_or_else(|e| panic!("default rules rejected: {e}"));
    let production = check_production_file(&rules);
    let production_ids = rule_ids(&production);
    for off in ["grep/no-wildcard-import", "complexity/boolean-params"] {
        assert!(
            production_ids.contains(off),
            "{off} must fire in library files: {production_ids:?}"
        );
    }
    let tests = check_test_file(&rules);
    let leaked: Vec<&str> = rule_ids(&tests)
        .intersection(&off_in_tests())
        .copied()
        .collect();
    assert!(
        leaked.is_empty(),
        "rules that are off in tests reported in a test file: {leaked:?}"
    );
}

#[test]
fn rules_without_the_exemption_report_the_same_in_test_files() {
    let rules = RuleSet::new(&Config::default(), &RuleFilter::default())
        .unwrap_or_else(|e| panic!("default rules rejected: {e}"));
    let off_ids = off_in_tests();
    let kept = |findings: Vec<Finding>| -> Vec<Finding> {
        findings
            .into_iter()
            .filter(|finding| !off_ids.contains(&*finding.rule))
            .collect()
    };
    let production = kept(check_production_file(&rules));
    let tests = kept(check_test_file(&rules));
    assert!(
        rule_ids(&production).contains("agent/informative-assertions"),
        "the sample must trigger a rule that keeps its findings in test files: {production:?}"
    );
    assert_eq!(production, tests);
}
