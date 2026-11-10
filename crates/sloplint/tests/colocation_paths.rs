//! Web colocated-test lookup: any test file in the package covers every module.

use std::path::Path;

use sloplint::{RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const SOURCE: &str = "export function widget(): void {}\n";
const STUB: &str = "export const x = 1;\n";
const RULE: &str = "agent/colocated-tests";

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn flagged(root: &Path) -> bool {
    let report = run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![RulePattern::parse(RULE).unwrap()]),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    })
    .unwrap();
    report.findings.iter().any(|f| &*f.rule == RULE)
}

fn root_with(extra: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "src/widget.ts", SOURCE);
    for (rel, contents) in extra {
        write(tmp.path(), rel, contents);
    }
    tmp
}

#[test]
fn missing_reports_source() {
    let tmp = root_with(&[]);
    assert!(
        flagged(tmp.path()),
        "wave-self-lint: untested src/widget.ts must be flagged"
    );
}

#[test]
fn sibling_test_still_suppresses() {
    let tmp = root_with(&[("src/widget.test.ts", STUB)]);
    assert!(
        !flagged(tmp.path()),
        "wave-self-lint: src/widget.test.ts must suppress the finding"
    );
}

#[test]
fn tests_dir_suppresses() {
    let tmp = root_with(&[("src/__tests__/widget.test.ts", STUB)]);
    assert!(
        !flagged(tmp.path()),
        "wave-self-lint: src/__tests__/widget.test.ts must suppress the finding"
    );
}

#[test]
fn tests_dir_any_extension_suppresses() {
    let tmp = root_with(&[("src/__tests__/widget.test.tsx", STUB)]);
    assert!(
        !flagged(tmp.path()),
        "wave-self-lint: tsx variant in __tests__ must suppress the finding"
    );
}

#[test]
fn any_test_in_the_package_suppresses_regardless_of_stem() {
    let tmp = root_with(&[("src/__tests__/gadget.test.ts", STUB)]);
    assert!(
        !flagged(tmp.path()),
        "wave-self-lint: any test file marks the package tested"
    );
}

#[test]
fn other_package_tests_do_not_suppress() {
    let tmp = root_with(&[("other/__tests__/widget.test.ts", STUB)]);
    assert!(
        flagged(tmp.path()),
        "wave-self-lint: other/__tests__ must not suppress src/widget.ts"
    );
}

#[test]
fn unrelated_library_file_does_not_suppress() {
    let tmp = root_with(&[("src/helper.ts", SOURCE)]);
    assert!(
        flagged(tmp.path()),
        "wave-self-lint: unrelated src/helper.ts must not suppress src/widget.ts"
    );
}
