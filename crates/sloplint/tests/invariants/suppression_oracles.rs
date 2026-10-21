//! Suppression directives: each form removes exactly the findings it names, and malformed or
//! unused directives are reported at their own line.

use sloplint::Report;

use super::{run_ok, standard, tempdir, write};

/// Two findings on one line: the generic name, and the boolean parameters (two reach the preset).
const FUNCTION: &str = "pub fn handle(flag: bool, other: bool) {}\n";

/// `(rule, line)` pairs of the findings, in report order.
fn rule_lines(report: &Report) -> Vec<(&str, u32)> {
    report.findings.iter().map(|f| (&*f.rule, f.line)).collect()
}

/// The standard-profile report of a project whose only source is `directive` then [`FUNCTION`].
fn report_with(directive: &str) -> Report {
    let (_tmp, root) = tempdir();
    write(&root, "src/lib.rs", &format!("{directive}{FUNCTION}"));
    run_ok(&standard(&root))
}

#[test]
fn suppressions_drop_only_their_findings() {
    let control = report_with("");
    assert_eq!(
        rule_lines(&control),
        [("complexity/boolean-params", 1), ("grep/generic-name", 1)],
        "control: both findings exist before any directive"
    );
    let cases = [
        (
            "// sloplint-allow(grep/generic-name): the name is the contract\n",
            vec![("complexity/boolean-params", 2)],
        ),
        (
            "// sloplint-allow(complexity/*): flags are the contract\n",
            vec![("grep/generic-name", 2)],
        ),
        (
            "// sloplint-allow(grep/generic-name, complexity/boolean-params): both are the contract\n",
            vec![],
        ),
    ];
    for (directive, remaining) in cases {
        let report = report_with(directive);
        assert_eq!(
            rule_lines(&report),
            remaining,
            "{directive:?} must remove exactly its findings and leave no unused-suppression"
        );
    }
}

/// A doc comment that restates the function name.
const DOC: &str = "/// Process the order.\n";

/// A function whose signature carries two more findings next to the echo-comment one.
const ORDER_FN: &str = "pub fn process_order(id: &str, account: &str, qty: u64) -> Result<u64, Box<dyn std::error::Error>> {\n    Ok(0)\n}\n";

/// The standard-profile report of a project whose only source is `source`.
fn source_report(source: &str) -> Report {
    let (_tmp, root) = tempdir();
    write(&root, "src/lib.rs", source);
    run_ok(&standard(&root))
}

// A035: a directive next to a doc comment must not become part of the doc text. Folded in, it
// dilutes the restated name, so the echo finding never exists and the directive that should
// remove it is reported as unused instead.
#[test]
fn directive_beside_a_restating_doc_removes_only_the_echo_finding() {
    let control = source_report(&format!("{DOC}{ORDER_FN}"));
    assert_eq!(
        rule_lines(&control),
        [
            ("slop/echo-comment", 1),
            ("agent/stringly-api", 2),
            ("agent/typed-error-boundary", 2),
        ],
        "control: the restating doc fires before any directive exists"
    );
    let directive = "// sloplint-allow(slop/echo-comment): the name is the contract\n";
    let suppressed = source_report(&format!("{directive}{DOC}{ORDER_FN}"));
    assert_eq!(
        rule_lines(&suppressed),
        [("agent/stringly-api", 3), ("agent/typed-error-boundary", 3)],
        "the directive above the doc removes only the echo finding and is not reported unused"
    );
}

#[test]
fn invalid_and_unused_suppressions_report() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(slop/echo-comment)\npub fn no_colon() {}\n// sloplint-allow(slop/echo-comment):  \npub fn blank_reason() {}\n// sloplint-allow(grep/generic-name): placeholder\npub fn kept() {}\n",
    );
    let report = run_ok(&standard(&root));
    assert_eq!(
        rule_lines(&report),
        [
            ("meta/invalid-suppression", 1),
            ("meta/invalid-suppression", 3),
            ("meta/unused-suppression", 5),
        ],
        "each directive problem is reported once, at its own line"
    );
}

#[test]
fn suppressions_for_inactive_rules_stay_silent() {
    let rule = "[[custom_rules]]\nid = \"custom/no-call\"\nlanguage = \"rust\"\nseverity = \"warn\"\nmessage = \"call found\"\nwhy = \"test why\"\nfix = \"test fix\"\nquery = \"(call_expression) @match\"\n";
    for (directive, kind) in [
        ("// sloplint-allow(custom/no-call): reviewed\n", "exact"),
        ("// sloplint-allow(custom/*): reviewed\n", "family"),
    ] {
        let (_tmp, root) = tempdir();
        write(&root, "sloplint.toml", rule);
        write(
            &root,
            "src/lib.rs",
            &format!("{directive}pub fn handle() {{}}\n"),
        );
        let active = run_ok(&standard(&root));
        assert!(
            active
                .findings
                .iter()
                .any(|f| &*f.rule == "meta/unused-suppression"),
            "{kind}: active but unmatched suppression must be reported unused"
        );
        write(
            &root,
            "sloplint.toml",
            &format!("{rule}[rules.\"custom/no-call\"]\nlevel = \"off\"\n"),
        );
        let silent = run_ok(&standard(&root));
        assert!(
            !silent
                .findings
                .iter()
                .any(|f| &*f.rule == "meta/unused-suppression"),
            "{kind}: suppression for an inactive rule must stay silent"
        );
        assert!(
            !silent
                .findings
                .iter()
                .any(|f| &*f.rule == "meta/invalid-suppression"),
            "{kind}: suppression for a known inactive rule must stay valid"
        );
    }
}
