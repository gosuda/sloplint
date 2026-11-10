//! Rule filters: unknown selectors fail, valid selectors scope findings.

use sloplint::{RunError, run};
use sloplint_core::{Config, ConfigError, RuleSet};

use super::{SAMPLE, audit_filter_of, audit_filter_opts, fired, run_ok, tempdir, write};

#[test]
fn audit_filter_rejects_unknown_selectors() {
    let (_tmp, root) = tempdir();
    write(&root, "sample.rs", SAMPLE);
    for (selectors, want) in [
        (vec!["grep/generic-nam"], "grep/generic-nam"),
        (vec!["typo/*"], "typo/*"),
        (vec!["grep/generic-name", "typo/*"], "typo/*"),
    ] {
        let Err(error) = run(&audit_filter_opts(&root, &selectors)) else {
            panic!("unknown selector {want} was accepted");
        };
        assert_eq!(error.exit_code(), 2, "unknown rule must exit 2");
        let RunError::Config(ConfigError::UnknownRule { id, .. }) = error else {
            panic!("unexpected error for {want}");
        };
        assert_eq!(id, want, "rejected selector mismatch");
    }
}

#[test]
fn audit_filter_ruleset_new_rejects_unknown() {
    let bad = audit_filter_of(&["grep/generic-nam"]);
    let Err(ConfigError::UnknownRule { id, .. }) = RuleSet::new(&Config::default(), &bad) else {
        panic!("RuleSet::new accepted an unknown filter");
    };
    assert_eq!(id, "grep/generic-nam", "rejected selector mismatch");
}

#[test]
fn audit_filter_accepts_valid_selectors() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "sample.rs",
        "pub fn handle(flag: bool, other: bool) { foo(); }\nfn foo() {}\n",
    );
    let call_rule = "[[custom_rules]]\nid = \"custom/no-call\"\nlanguage = \"rust\"\nseverity = \"minor\"\nmessage = \"call found\"\nwhy = \"test why\"\nfix = \"test fix\"\nquery = \"(call_expression) @match\"\n";
    write(&root, "sloplint.toml", call_rule);
    let check = |selector: &str| run_ok(&audit_filter_opts(&root, &[selector]));
    for (selector, rule, kind) in [
        ("grep/generic-name", "grep/generic-name", "exact"),
        ("grep/*", "grep/generic-name", "family"),
        ("custom/no-call", "custom/no-call", "custom"),
    ] {
        assert!(
            fired(&check(selector), rule),
            "{kind} filter missed its rule"
        );
    }
    let off = check("grep/multiline-argument");
    assert!(
        off.findings
            .iter()
            .all(|f| &*f.rule == "grep/multiline-argument"),
        "off-rule filter leaked other rules"
    );
    write(
        &root,
        "sloplint.toml",
        &format!("{call_rule}[rules.\"custom/no-call\"]\nlevel = \"off\"\n"),
    );
    let disabled = check("custom/no-call");
    assert!(
        disabled.findings.is_empty(),
        "disabled custom rule should report nothing but still be accepted"
    );
}
