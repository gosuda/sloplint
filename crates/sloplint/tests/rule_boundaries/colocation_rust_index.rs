//! Rust colocated-test lookup: a `tests/` directory file named `<stem>.rs` or `<stem>_*` counts.

use sloplint::{RunOptions, execute};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const RULE: &str = "agent/colocated-tests";
const WIDGET: &str = "pub fn widget() {}\n";
const PRIVATE_ONLY: &str = "fn helper() {}\n";

/// Paths the rule reports for a project made of `files`, in report order.
fn reported(files: &[(&str, &str)]) -> Vec<String> {
    let tmp = tempfile::tempdir().unwrap();
    for (rel, contents) in files {
        let path = tmp.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
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
    execution
        .report
        .findings
        .iter()
        .inspect(|finding| assert_eq!(&*finding.rule, RULE))
        .map(|finding| finding.path.to_string())
        .collect()
}

/// `src/widget.rs` plus `extra`.
fn with_widget(extra: &[(&str, &str)]) -> Vec<String> {
    let mut files = vec![("src/widget.rs", WIDGET)];
    files.extend_from_slice(extra);
    reported(&files)
}

#[test]
fn module_without_tests_is_reported() {
    assert_eq!(with_widget(&[]), ["src/widget.rs"]);
}

#[test]
fn tests_dir_file_with_the_stem_name_suppresses() {
    let found = with_widget(&[("tests/widget.rs", PRIVATE_ONLY)]);
    assert!(
        found.is_empty(),
        "tests/widget.rs must cover src/widget.rs, but the rule reported {found:?}"
    );
}

#[test]
fn tests_dir_file_with_the_stem_prefix_suppresses() {
    let found = with_widget(&[("tests/widget_extra.rs", PRIVATE_ONLY)]);
    assert!(
        found.is_empty(),
        "tests/widget_extra.rs must cover src/widget.rs by prefix, but the rule reported {found:?}"
    );
}

#[test]
fn tests_dir_in_another_crate_suppresses() {
    let found = with_widget(&[("crates/other/tests/widget.rs", PRIVATE_ONLY)]);
    assert!(
        found.is_empty(),
        "crates/other/tests/widget.rs must cover src/widget.rs, but the rule reported {found:?}"
    );
}

#[test]
fn tests_dir_under_src_suppresses() {
    let found = with_widget(&[("src/tests/widget.rs", PRIVATE_ONLY)]);
    assert!(
        found.is_empty(),
        "src/tests/widget.rs must cover src/widget.rs, but the rule reported {found:?}"
    );
}

#[test]
fn stem_must_be_the_whole_name_or_a_leading_prefix() {
    let extra = [
        ("tests/widgets.rs", PRIVATE_ONLY),
        ("tests/gadget_widget.rs", PRIVATE_ONLY),
    ];
    assert_eq!(with_widget(&extra), ["src/widget.rs"]);
}

#[test]
fn directory_segment_must_be_exactly_tests() {
    let extra = [
        ("attests/widget.rs", PRIVATE_ONLY),
        ("tests_extra/widget_x.rs", PRIVATE_ONLY),
    ];
    assert_eq!(with_widget(&extra), ["src/widget.rs"]);
}

#[test]
fn one_tests_file_covers_every_module_with_that_stem() {
    let files = [
        ("src/a/widget.rs", "pub fn a_widget() {}\n"),
        ("src/b/widget.rs", "pub fn b_widget() {}\n"),
        ("tests/widget.rs", PRIVATE_ONLY),
    ];
    let found = reported(&files);
    assert!(
        found.is_empty(),
        "one tests/widget.rs must cover every module with stem widget, but the rule reported {found:?}"
    );
}

#[test]
fn numbered_stems_do_not_cover_each_other() {
    let modules: Vec<(String, String)> = (0..200)
        .map(|index| {
            (
                format!("src/m{index}.rs"),
                format!("pub fn f{index}() {{}}\n"),
            )
        })
        .collect();
    let mut files: Vec<(&str, &str)> = modules
        .iter()
        .map(|(rel, contents)| (rel.as_str(), contents.as_str()))
        .collect();
    files.push(("tests/m1_extra.rs", PRIVATE_ONLY));
    files.push(("tests/m10.rs", PRIVATE_ONLY));
    let uncovered = reported(&files);
    assert_eq!(uncovered.len(), 198);
    assert!(
        !uncovered.iter().any(|path| path == "src/m1.rs"),
        "m1_extra covers m1"
    );
    assert!(
        !uncovered.iter().any(|path| path == "src/m10.rs"),
        "m10 covers m10"
    );
    assert!(
        uncovered.iter().any(|path| path == "src/m100.rs"),
        "m10 must not cover m100"
    );
    assert!(
        uncovered.iter().any(|path| path == "src/m11.rs"),
        "m1_extra must not cover m11"
    );
}
