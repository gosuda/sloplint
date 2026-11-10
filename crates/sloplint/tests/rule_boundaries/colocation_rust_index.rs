//! Rust colocated-test lookup: a test file covers only modules it shares a key with.

use sloplint::{RunOptions, execute};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const RULE: &str = "agent/colocated-tests";
const WIDGET: &str = "pub fn widget() {}\n";
const PRIVATE_ONLY: &str = "fn helper() {}\n";

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

fn with_widget(extra: &[(&str, &str)]) -> Vec<String> {
    let mut files = vec![("src/widget.rs", WIDGET)];
    files.extend_from_slice(extra);
    reported(&files)
}

#[test]
fn module_without_tests_is_reported() {
    assert_eq!(
        with_widget(&[("tests/unrelated.rs", PRIVATE_ONLY)]),
        ["src/widget.rs"]
    );
}

#[test]
fn package_without_evidence_stays_silent() {
    assert_eq!(with_widget(&[]), Vec::<String>::new());
}

#[test]
fn tests_dir_file_in_the_package_suppresses() {
    let found = with_widget(&[("tests/widget.rs", PRIVATE_ONLY)]);
    assert!(
        found.is_empty(),
        "tests/widget.rs must cover src/widget.rs, but the rule reported {found:?}"
    );
}

#[test]
fn any_tests_file_in_the_package_suppresses_regardless_of_stem() {
    let found = with_widget(&[("tests/unrelated.rs", PRIVATE_ONLY)]);
    assert_eq!(
        found,
        ["src/widget.rs"],
        "a different stem shares no key, so widget stays flagged"
    );
}

#[test]
fn tests_dir_in_another_package_does_not_suppress() {
    assert_eq!(
        with_widget(&[
            ("tests/unrelated.rs", PRIVATE_ONLY),
            ("crates/other/tests/widget.rs", PRIVATE_ONLY)
        ]),
        ["src/widget.rs"]
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
fn one_tests_file_covers_every_module_in_the_package() {
    let files = [
        ("src/widget.rs", "pub fn widget() {}\n"),
        ("src/a/widget.rs", "pub fn a_widget() {}\n"),
        ("tests/widget.rs", PRIVATE_ONLY),
    ];
    let found = reported(&files);
    assert_eq!(
        found,
        ["src/a/widget.rs"],
        "tests/widget.rs covers the module it shares a key with, the other stays flagged"
    );
}

#[test]
fn many_modules_without_tests_are_all_reported() {
    let modules: Vec<(String, String)> = (0..60)
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
    files.push(("tests/only.rs", PRIVATE_ONLY));
    assert_eq!(
        reported(&files).len(),
        60,
        "evidence without shared keys leaves all sixty modules flagged"
    );
    let bare: Vec<(&str, &str)> = modules
        .iter()
        .map(|(rel, contents)| (rel.as_str(), contents.as_str()))
        .collect();
    assert!(
        reported(&bare).is_empty(),
        "a package without evidence stays silent"
    );
}
