//! Custom rule findings: path globs, severity caps, and message interpolation.

use sloplint_core::config::parse_config;
use sloplint_core::lang::Lang;
use sloplint_core::{Finding, RuleFilter, RuleSet, Severity};
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const CALL_SOURCE: &str = "fn main() { first(1); }\n";

fn rule_src(severity: &str, message: &str, extra: &str, tables: &str) -> String {
    format!(
        "[[custom_rules]]\nid = \"custom/x\"\nlanguage = \"rust\"\nseverity = \"{severity}\"\nmessage = {message:?}\nwhy = \"w\"\nfix = \"f\"\nquery = '''\n(call_expression function: (identifier) @callee) @match\n'''\n{extra}{tables}"
    )
}

fn findings_for(toml_src: &str, path: &str, source: &str) -> Vec<Finding> {
    let config = parse_config(toml_src, "sloplint.toml")
        .unwrap_or_else(|e| panic!("valid config rejected: {e}"));
    let custom = CustomRules::compile(&config.custom_rules)
        .unwrap_or_else(|e| panic!("valid rule failed to compile: {e}"));
    let rules = RuleSet::new(&config, &RuleFilter::default())
        .unwrap_or_else(|e| panic!("valid rules rejected: {e}"));
    let mut parsers = Parsers::new();
    let input = FileInput {
        lang: Lang::Rust,
        path,
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut parsers, &input, &custom);
    rules.custom_findings(&analysis.facts, &analysis.custom_hits)
}

fn hit_count(toml_src: &str, path: &str) -> usize {
    findings_for(toml_src, path, CALL_SOURCE).len()
}

#[test]
fn include_and_exclude_globs_filter_by_relative_path() {
    let both = rule_src(
        "deny",
        "call",
        "include = [\"src/**\"]\nexclude = [\"**/gen/**\"]\n",
        "",
    );
    for (path, expected) in [
        ("src/a.rs", 1),
        ("src/deep/a.rs", 1),
        ("tests/a.rs", 0),
        ("src/gen/a.rs", 0),
        ("src/deep/gen/a.rs", 0),
    ] {
        assert_eq!(hit_count(&both, path), expected, "include+exclude, {path}");
    }
    let exclude_only = rule_src("deny", "call", "exclude = [\"**/gen/**\"]\n", "");
    for (path, expected) in [("lib/z.rs", 1), ("gen/z.rs", 0)] {
        assert_eq!(
            hit_count(&exclude_only, path),
            expected,
            "exclude only, {path}"
        );
    }
    let unfiltered = rule_src("deny", "call", "", "");
    assert_eq!(hit_count(&unfiltered, "anywhere/at/all.rs"), 1);
}

#[test]
fn rule_severity_applies_without_a_level_table() {
    for (declared, expected) in [("deny", Severity::Deny), ("warn", Severity::Warn)] {
        let found = findings_for(&rule_src(declared, "call", "", ""), "src/a.rs", CALL_SOURCE);
        assert_eq!(found.len(), 1, "{declared}");
        assert_eq!(found[0].severity, expected, "{declared}");
        assert_eq!(&*found[0].rule, "custom/x");
    }
}

#[test]
fn a_level_table_caps_or_disables_but_never_raises() {
    let table = |level: &str| format!("[rules.\"custom/x\"]\nlevel = \"{level}\"\n");
    let capped = findings_for(
        &rule_src("deny", "call", "", &table("warn")),
        "src/a.rs",
        CALL_SOURCE,
    );
    assert_eq!(capped.len(), 1);
    assert_eq!(
        capped[0].severity,
        Severity::Warn,
        "level warn caps a deny rule"
    );
    let off = findings_for(
        &rule_src("deny", "call", "", &table("off")),
        "src/a.rs",
        CALL_SOURCE,
    );
    assert!(off.is_empty(), "level off disables the rule: {off:?}");
    let same = findings_for(
        &rule_src("deny", "call", "", &table("deny")),
        "src/a.rs",
        CALL_SOURCE,
    );
    assert_eq!(same[0].severity, Severity::Deny);
    let not_raised = findings_for(
        &rule_src("warn", "call", "", &table("deny")),
        "src/a.rs",
        CALL_SOURCE,
    );
    assert_eq!(
        not_raised[0].severity,
        Severity::Warn,
        "level deny must not raise a warn rule"
    );
}

#[test]
fn captures_interpolate_at_the_match_position() {
    let found = findings_for(
        &rule_src("warn", "calls {callee}", "", ""),
        "src/a.rs",
        "fn f() {\n    first(1);\n}\n",
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].message, "calls first");
    assert_eq!((found[0].line, found[0].column), (2, 5));
    assert_eq!(found[0].fix, "f");
    assert_eq!(&*found[0].context, "first(1);");
}

#[test]
fn interpolated_captures_are_clipped_to_sixty_characters() {
    let long = "a".repeat(70);
    let source = format!("fn f() {{ {long}(); }}\n");
    let found = findings_for(
        &rule_src("warn", "calls {callee}", "", ""),
        "src/a.rs",
        &source,
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].message, format!("calls {}", "a".repeat(60)));
}

#[test]
fn multiline_captures_interpolate_their_first_line_only() {
    let found = findings_for(
        &rule_src("warn", "{match}", "", ""),
        "src/a.rs",
        "fn f() {\n    first(\n        1,\n    );\n}\n",
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].message, "first(");
}
