//! `agent/unique-patch-anchors`: which line windows count as one repeated block.

use sloplint::{RunOptions, execute};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const RULE: &str = "agent/unique-patch-anchors";

/// `(line, message)` of every finding for `source` written to `a.py`, in report order.
fn reported(source: &str) -> Vec<(u32, String)> {
    reported_at(Strictness::Strict, source)
}

/// `reported` at an explicit strictness, for scenarios that need a window other than the
/// strict one.
fn reported_at(strictness: Strictness, source: &str) -> Vec<(u32, String)> {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.py"), source).unwrap();
    let execution = execute(&RunOptions {
        cwd: tmp.path().to_path_buf(),
        root: Some(tmp.path().to_path_buf()),
        strictness: Some(strictness),
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

/// Ten strong lines.
fn long_block() -> String {
    (0..10)
        .map(|k| format!("result_{k} = helper_function_{k}(alpha_argument, beta_argument)\n"))
        .collect()
}

/// `long_block()` with its last two lines calling `variant_argument` instead.
fn long_block_with_variant_tail() -> String {
    (0..10)
        .map(|k| {
            if k < 8 {
                format!("result_{k} = helper_function_{k}(alpha_argument, beta_argument)\n")
            } else {
                format!("result_{k} = helper_function_{k}(variant_argument, beta_argument)\n")
            }
        })
        .collect()
}

/// Fails when a finding claims a region `source` does not support: every listed occurrence must
/// leave room for the claimed span of kept lines, and every claimed region must match the first
/// one line for line, so a shortened or file-end-truncated tail can never be reported identical.
fn assert_reported_regions_are_identical_and_in_bounds(source: &str, found: &[(u32, String)]) {
    let kept: Vec<String> = source
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();
    let kept_index_of_line = |line: u32| -> usize {
        let mut kept_index = 0;
        for (offset, raw) in source.lines().enumerate() {
            let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
            if normalized.is_empty() {
                continue;
            }
            let number = u32::try_from(offset + 1).unwrap_or_else(|error| {
                panic!("line {} does not fit a line number: {error}", offset + 1)
            });
            if number == line {
                return kept_index;
            }
            kept_index += 1;
        }
        panic!("line {line} is not a kept line of the source");
    };
    for (line, message) in found {
        let tail = message
            .split_once(" identical ")
            .unwrap_or_else(|| panic!("unrecognized message for line {line}: {message}"))
            .1;
        let (span, list) = tail
            .split_once("-line blocks (lines ")
            .unwrap_or_else(|| panic!("unrecognized message for line {line}: {message}"));
        let span: usize = span
            .parse()
            .unwrap_or_else(|error| panic!("unparseable span in {message}: {error}"));
        let list = list
            .strip_suffix(')')
            .unwrap_or_else(|| panic!("unrecognized message for line {line}: {message}"));
        let starts: Vec<u32> = list
            .split(", ")
            .map(|start| {
                start
                    .parse()
                    .unwrap_or_else(|error| panic!("unparseable line in {message}: {error}"))
            })
            .collect();
        let first = kept_index_of_line(starts[0]);
        assert!(
            first + span <= kept.len(),
            "{message}: the region at line {} claims kept lines {}..{}, but only {} exist",
            starts[0],
            first,
            first + span,
            kept.len(),
        );
        for start in &starts[1..] {
            let at = kept_index_of_line(*start);
            assert!(
                at + span <= kept.len(),
                "{message}: the region at line {start} claims kept lines {at}..{}, but only {} \
                 exist",
                at + span,
                kept.len(),
            );
            assert_eq!(
                &kept[at..at + span],
                &kept[first..first + span],
                "{message}: the block at line {start} must be identical to the one at line {}",
                starts[0],
            );
        }
    }
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
fn a_run_whose_coalesced_span_drops_below_min_repeats_keeps_a_qualifying_span() {
    let block: String = (0..8)
        .map(|k| format!("result_{k} = helper_function_{k}(alpha_argument, beta_argument)\n"))
        .collect();
    let head: String = block
        .lines()
        .take(2)
        .map(|line| format!("{line}\n"))
        .collect();
    let source = format!("{block}{block}{block}{head}");
    assert_eq!(
        reported_at(Strictness::Standard, &source),
        vec![(1, "3 identical 8-line blocks (lines 1, 9, 17)".to_string())],
        "the coalesced span-10 run fits only two non-overlapping occurrences, so the run must \
         fall back to its span-8 window instead of suppressing three qualifying repeats"
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

#[test]
fn a_fourth_block_with_an_altered_tail_is_not_reported_as_identical() {
    let source = format!(
        "{}\n{}\n{}\n{}",
        long_block(),
        long_block(),
        long_block(),
        long_block_with_variant_tail(),
    );
    let found = reported_at(Strictness::Standard, &source);
    assert_eq!(
        found,
        vec![(
            1,
            "3 identical 10-line blocks (lines 1, 12, 23)".to_string()
        )],
        "the fourth block shares only its first eight lines with the others, so the run may \
         report only the occurrences its full span still covers"
    );
    assert_reported_regions_are_identical_and_in_bounds(&source, &found);
}

#[test]
fn a_fourth_block_truncated_by_the_file_end_is_not_claimed_past_it() {
    let head: String = long_block()
        .lines()
        .take(8)
        .map(|line| format!("{line}\n"))
        .collect();
    let source = format!(
        "{}\n{}\n{}\n{}",
        long_block(),
        long_block(),
        long_block(),
        head
    );
    let found = reported_at(Strictness::Standard, &source);
    assert_eq!(
        found,
        vec![(
            1,
            "3 identical 10-line blocks (lines 1, 12, 23)".to_string()
        )],
        "the file ends eight lines into the fourth block, so no occurrence may be claimed past \
         the last kept line"
    );
    assert_reported_regions_are_identical_and_in_bounds(&source, &found);
}
