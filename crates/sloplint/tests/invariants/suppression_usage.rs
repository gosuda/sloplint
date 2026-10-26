//! Per-operand suppression usage: `meta/unused-suppression` names each active operand that
//! matched no finding, whatever its siblings did, and suppressed duplicates do not renumber
//! the fingerprints of the findings that stay visible.

use sloplint::{Report, RunOptions};
use sloplint_core::{RuleFilter, RulePattern};

use super::{run_ok, standard, tempdir, write};

const BOOL_FN: &str = "pub fn handle(flag: bool, other: bool) {}\n";

/// The messages of every `meta/unused-suppression` finding, in report order.
fn unused_messages(report: &Report) -> Vec<&str> {
    report
        .findings
        .iter()
        .filter(|f| &*f.rule == "meta/unused-suppression")
        .map(|f| f.message.as_str())
        .collect()
}

fn one_unused_naming(report: &Report, operand: &str, silent: &str) {
    let messages = unused_messages(report);
    assert_eq!(
        messages.len(),
        1,
        "exactly one directive-level unused finding is expected, got {messages:?}"
    );
    assert!(
        messages[0].contains(operand),
        "the unused finding must name the unused active operand `{operand}`: {messages:?}"
    );
    assert!(
        !messages[0].contains(silent),
        "the unused finding must not name `{silent}`: {messages:?}"
    );
}

#[test]
fn unused_active_operand_is_reported_beside_a_used_sibling() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        &format!(
            "// sloplint-allow(complexity/boolean-params, slop/echo-comment): flags are the contract\n{BOOL_FN}"
        ),
    );
    let report = run_ok(&standard(&root));
    one_unused_naming(&report, "slop/echo-comment", "complexity/boolean-params");
    assert_eq!(
        report
            .findings
            .iter()
            .find(|f| &*f.rule == "meta/unused-suppression")
            .map(|f| f.line),
        Some(1),
        "the unused finding sits at the directive's own line"
    );
}

#[test]
fn inactive_operands_are_silent_when_filtered_out() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(slop/echo-comment, complexity/cognitive-depth): reviewed\npub fn handle() {}\n",
    );
    let options = RunOptions {
        rule_filter: RuleFilter(vec![
            RulePattern::parse("slop/echo-comment").unwrap_or_else(|e| panic!("pattern: {e}")),
            RulePattern::parse("meta/unused-suppression")
                .unwrap_or_else(|e| panic!("pattern: {e}")),
        ]),
        ..standard(&root)
    };
    let report = run_ok(&options);
    one_unused_naming(&report, "slop/echo-comment", "complexity/cognitive-depth");
}

#[test]
fn inactive_operands_are_silent_when_turned_off_in_config() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "sloplint.toml",
        "[rules.\"complexity/cognitive-depth\"]\nlevel = \"off\"\n",
    );
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(slop/echo-comment, complexity/cognitive-depth): reviewed\npub fn handle() {}\n",
    );
    let report = run_ok(&standard(&root));
    one_unused_naming(&report, "slop/echo-comment", "complexity/cognitive-depth");
}

#[test]
fn directive_whose_operands_all_match_reports_nothing() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        &format!(
            "// sloplint-allow(complexity/boolean-params, grep/generic-name): both are the contract\n{BOOL_FN}"
        ),
    );
    let report = run_ok(&standard(&root));
    assert!(
        unused_messages(&report).is_empty(),
        "every operand matched, so no unused finding is expected: {:?}",
        unused_messages(&report)
    );
}

#[test]
fn unused_family_operand_is_reported_beside_a_used_exact_operand() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(complexity/boolean-params, grep/*): flags are the contract\npub fn process_order(flag: bool, other: bool) {}\n",
    );
    let report = run_ok(&standard(&root));
    one_unused_naming(&report, "grep/*", "complexity/boolean-params");
}

#[test]
fn suppressed_duplicate_does_not_renumber_the_visible_fingerprint() {
    let (_tmp, with_duplicate) = tempdir();
    write(
        &with_duplicate,
        "src/lib.rs",
        &format!(
            "// sloplint-allow(complexity/boolean-params): flags are the contract\n{BOOL_FN}{BOOL_FN}"
        ),
    );
    let (_tmp_alone, alone) = tempdir();
    write(&alone, "src/lib.rs", BOOL_FN);

    let visible = |report: &Report| -> Vec<String> {
        report
            .findings
            .iter()
            .filter(|f| &*f.rule == "complexity/boolean-params")
            .map(|f| f.fingerprint.to_string())
            .collect()
    };
    let with = visible(&run_ok(&standard(&with_duplicate)));
    let without = visible(&run_ok(&standard(&alone)));
    assert_eq!(
        with.len(),
        1,
        "the suppressed copy leaves one visible finding"
    );
    assert_eq!(
        with, without,
        "a visible finding keeps its fingerprint when a suppressed duplicate is deleted"
    );
}
