//! A log call with no arguments has no message at all and must be reported as such.

use sloplint::{RunOptions, execute};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const RULE: &str = "grep/contiguous-log-literal";

/// Line of every finding in `source` written to `rel` in a fresh project.
fn findings_in(rel: &str, source: &str) -> Vec<u32> {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join(rel), source).unwrap();
    let execution = execute(&RunOptions {
        cwd: tmp.path().to_path_buf(),
        root: Some(tmp.path().to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![RulePattern::parse(RULE).unwrap()]),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    })
    .unwrap();
    assert_eq!(execution.report.summary.files, 1, "{rel} must be analyzed");
    execution
        .report
        .findings
        .iter()
        .inspect(|finding| assert_eq!(&*finding.rule, RULE))
        .map(|finding| finding.line)
        .collect()
}

/// `(no-argument line, dynamic line, literal line)` triple passed to
/// `assert_zero_argument_call_reports_no_message`.
type CallLines = (u32, u32, u32);

fn assert_zero_argument_call_reports_no_message(
    rel: &str,
    source: &str,
    (no_message_line, dynamic_line, literal_line): CallLines,
) {
    let lines = findings_in(rel, source);
    assert_eq!(
        lines,
        [no_message_line, dynamic_line],
        "{rel}: the zero-argument call and the dynamic call must be reported"
    );
    assert!(
        !lines.contains(&literal_line),
        "{rel}: the literal call on line {literal_line} must not be reported"
    );
}

#[test]
fn python_zero_argument_call_reports_no_message() {
    let source = "import logging\n\nlogger = logging.getLogger(\"x\")\n\n\ndef f(message):\n    logger.error()\n    logger.error(message)\n    logger.error(\"failed to open the configuration file\")\n";
    assert_zero_argument_call_reports_no_message("a.py", source, (7, 8, 9));
}

#[test]
fn typescript_zero_argument_call_reports_no_message() {
    let source = "export function f(message: string): void {\n  console.log();\n  console.log(message);\n  console.log(\"loading configuration files now\");\n}\n";
    assert_zero_argument_call_reports_no_message("b.ts", source, (2, 3, 4));
}

#[test]
fn rust_zero_argument_macro_reports_no_message() {
    let source = "pub fn f(message: &str) {\n    error!();\n    error!(message);\n    error!(\"failed to connect to the peer\");\n}\n";
    assert_zero_argument_call_reports_no_message("c.rs", source, (2, 3, 4));
}

#[test]
fn java_zero_argument_call_reports_no_message() {
    let source = "class D {\n    void f(Exception e, String message) {\n        e.printStackTrace();\n        System.out.println(message);\n        System.out.println(\"failed to open the configuration file\");\n    }\n}\n";
    assert_zero_argument_call_reports_no_message("D.java", source, (3, 4, 5));
}
