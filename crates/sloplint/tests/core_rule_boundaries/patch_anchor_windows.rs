//! `agent/unique-patch-anchors`: which line windows count as one repeated block.

use sloplint::{RunOptions, execute};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const RULE: &str = "agent/unique-patch-anchors";

/// `(line, message)` of every finding for `source` written to `a.py`, in report order.
fn reported(source: &str) -> Vec<(u32, String)> {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.py"), source).unwrap();
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
    assert_eq!(execution.report.summary.files, 1, "a.py must be analyzed");
    execution
        .report
        .findings
        .iter()
        .inspect(|finding| assert_eq!(&*finding.rule, RULE))
        .map(|finding| (finding.line, finding.message.clone()))
        .collect()
}

/// Six strong lines.
fn block() -> String {
    (0..6)
        .map(|k| format!("result_{k} = helper_function_{k}(alpha_argument, beta_argument)\n"))
        .collect()
}

/// `block()` with its fourth line changed to `variant`.
fn block_with_variant(variant: usize) -> String {
    block().replace(
        "result_3 = helper_function_3(alpha_argument, beta_argument)",
        &format!("result_3 = helper_function_3(alpha_argument, variant_{variant})"),
    )
}

/// Two strong lines no other call shares.
fn noise(tag: &str) -> String {
    (0..2)
        .map(|i| format!("noise_{tag}_{i} = distinct_call_{tag}_{i}(left_{i}, right_{i})\n"))
        .collect()
}

/// Three copies separated by two-line gaps: block lines 1, 9 and 17.
fn three_copies(copy: impl Fn(usize) -> String) -> String {
    format!(
        "{}{}{}{}{}",
        copy(0),
        noise("x"),
        copy(1),
        noise("y"),
        copy(2)
    )
}

fn three_blocks_at_lines_1_9_17() -> Vec<(u32, String)> {
    vec![(1, "3 identical 6-line blocks (lines 1, 9, 17)".to_string())]
}

#[test]
fn three_identical_blocks_are_reported_once() {
    assert_eq!(
        reported(&three_copies(|_| block())),
        three_blocks_at_lines_1_9_17()
    );
}

#[test]
fn two_identical_blocks_are_below_the_repeat_threshold() {
    let source = format!("{}{}{}", block(), noise("x"), block());
    let found = reported(&source);
    assert!(
        found.is_empty(),
        "two copies are below min_repeats of 3, but got {found:?}"
    );
}

#[test]
fn blocks_sharing_first_and_last_lines_but_not_the_middle_are_not_grouped() {
    let found = reported(&three_copies(block_with_variant));
    assert!(
        found.is_empty(),
        "blocks that differ in one middle line must not group, but got {found:?}"
    );
}

#[test]
fn whitespace_differences_do_not_split_a_group() {
    let tabbed = block().replace(" = ", " =\t");
    let padded: String = block().lines().map(|line| format!("{line}   \n")).collect();
    let source = format!(
        "{}{}{}{}{}",
        block(),
        noise("x"),
        tabbed,
        noise("y"),
        padded
    );
    assert_eq!(reported(&source), three_blocks_at_lines_1_9_17());
}

#[test]
fn unicode_whitespace_does_not_split_a_group() {
    let labels = |separator: &str| -> String {
        (0..6)
            .map(|k| {
                format!("label_{k} = format_label(\"alpha beta{separator}gamma delta\", {k})\n")
            })
            .collect()
    };
    let separators = [" ", "\u{a0}", " \u{3000} "];
    let source = three_copies(|index| labels(separators[index]));
    assert_eq!(reported(&source), three_blocks_at_lines_1_9_17());
}

#[test]
fn periodic_input_reports_its_region_once_with_non_overlapping_starts() {
    let period: String = (0..3)
        .map(|k| format!("first_line_{k} = call_site_{k}(alpha_argument)\n"))
        .collect();
    let expected = vec![(1, "3 identical 6-line blocks (lines 1, 7, 13)".to_string())];
    assert_eq!(
        reported(&period.repeat(7)),
        expected,
        "lines 2 and 3 slide into one run with only two non-overlapping occurrences, so only line 1 is reported"
    );
}

#[test]
fn a_window_needs_three_strong_lines() {
    let strong = [
        "strong_value_one = compute_one(first_input)",
        "strong_value_two = compute_two(second_input)",
        "strong_value_three = compute_three(third_input)",
    ];
    let three_strong = format!(
        "{}\nwa = 1\n{}\nwb = 2\n{}\nwc = 3\n",
        strong[0], strong[1], strong[2]
    );
    let two_strong = format!(
        "{}\nwa = 1\nwb = 2\n{}\nwc = 3\nwd = 4\n",
        strong[0], strong[1]
    );
    assert_eq!(
        reported(&three_copies(|_| three_strong.clone())),
        three_blocks_at_lines_1_9_17()
    );
    let found = reported(&three_copies(|_| two_strong.clone()));
    assert!(
        found.is_empty(),
        "a window with two strong lines is below the minimum of three, but got {found:?}"
    );
}
