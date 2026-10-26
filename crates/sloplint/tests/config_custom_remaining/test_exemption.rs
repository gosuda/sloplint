//! Rules that exempt test files report nothing there, and no other rule is affected.

use std::collections::BTreeSet;

use sloplint_core::config::Config;
use sloplint_core::lang::Lang;
use sloplint_core::{CATALOG, Finding, RuleFilter, RuleSet};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = "use std::collections::*;\n\npub fn handle(a: bool, b: bool) {}\n";

fn check_production_file(rules: &RuleSet) -> Vec<Finding> {
    check_with_test_flag(rules, false)
}

fn check_test_file(rules: &RuleSet) -> Vec<Finding> {
    check_with_test_flag(rules, true)
}

fn check_with_test_flag(rules: &RuleSet, is_test_file: bool) -> Vec<Finding> {
    let custom = CustomRules::compile(&[]).unwrap_or_else(|e| panic!("empty rules: {e}"));
    let mut parsers = Parsers::new();
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source: SOURCE,
        is_test_file,
    };
    let analysis = analyze(&mut parsers, &input, &custom);
    assert!(analysis.parse_error.is_none(), "the sample must parse");
    rules.check_file(&analysis.facts)
}

fn rule_ids(findings: &[Finding]) -> BTreeSet<&str> {
    findings.iter().map(|finding| &*finding.rule).collect()
}

#[test]
fn exempt_rules_report_nothing_in_test_files() {
    let rules = RuleSet::new(&Config::default(), &RuleFilter::default())
        .unwrap_or_else(|e| panic!("default rules rejected: {e}"));
    let production = check_production_file(&rules);
    let production_ids = rule_ids(&production);
    for exempt in ["grep/no-wildcard-import", "complexity/boolean-params"] {
        assert!(
            production_ids.contains(exempt),
            "{exempt} must fire outside tests: {production_ids:?}"
        );
    }
    let tests = check_test_file(&rules);
    let exempt_ids: BTreeSet<&str> = CATALOG
        .iter()
        .filter(|info| info.exempt_tests)
        .map(|info| info.id)
        .collect();
    let leaked: Vec<&str> = rule_ids(&tests)
        .intersection(&exempt_ids)
        .copied()
        .collect();
    assert!(
        leaked.is_empty(),
        "exempt rules reported in a test file: {leaked:?}"
    );
}

#[test]
fn rules_without_the_exemption_report_the_same_in_test_files() {
    let rules = RuleSet::new(&Config::default(), &RuleFilter::default())
        .unwrap_or_else(|e| panic!("default rules rejected: {e}"));
    let exempt_ids: BTreeSet<&str> = CATALOG
        .iter()
        .filter(|info| info.exempt_tests)
        .map(|info| info.id)
        .collect();
    let kept = |findings: Vec<Finding>| -> Vec<Finding> {
        findings
            .into_iter()
            .filter(|finding| !exempt_ids.contains(&*finding.rule))
            .collect()
    };
    let production = kept(check_production_file(&rules));
    let tests = kept(check_test_file(&rules));
    assert!(
        rule_ids(&production).contains("grep/generic-name"),
        "the sample must trigger a rule that has no exemption: {production:?}"
    );
    assert_eq!(production, tests);
}
