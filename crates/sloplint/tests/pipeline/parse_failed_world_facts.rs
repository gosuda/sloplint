//! Parse-failed files report their error position and contribute no world facts.
//!
//! The first case pins the emitted `meta/parse-error` column and context for a
//! multiline UTF-8 input. The other two pin the consumer-visible side of empty
//! facts: a broken file holding a `render` declaration plus a clean file
//! holding `render` must not trigger `grep/homonym-definitions`, while two
//! clean files holding `render` must trigger it under `min_files = 2`.

use std::path::{Path, PathBuf};

use sloplint::{Report, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const PARSE_ERROR: &str = "meta/parse-error";
const HOMONYM: &str = "grep/homonym-definitions";
const BROKEN_UTF8: &str = "fn ok() {}\n// café\n@\n";
const HOMONYM_TOML: &str = "[rules.\"grep/homonym-definitions\"]\nmin_files = 2\n";
const RENDER_CLEAN: &str = "pub fn render() {}\n";
const RENDER_BROKEN: &str = "pub fn render() {}\nfn broken( {\n";

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&root, "sloplint.toml", "");
    (tmp, root)
}

fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn options(root: &Path) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    }
}

fn homonym_options(root: &Path) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![
            RulePattern::parse(HOMONYM).expect("homonym pattern parses"),
        ]),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    }
}

fn homonym_paths(report: &Report) -> Vec<(String, u32)> {
    let mut found: Vec<(String, u32)> = report
        .findings
        .iter()
        .filter(|f| f.rule.as_ref() == HOMONYM)
        .map(|f| (f.path.to_string(), f.line))
        .collect();
    found.sort();
    found
}

#[test]
fn a_syntax_error_reports_its_first_error_column_and_context() {
    let (_tmp, root) = project();
    put(&root, "broken.rs", BROKEN_UTF8);

    let report = run(&options(&root)).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert_eq!(
        report.findings.len(),
        1,
        "an unparseable file is judged by its syntax error alone: {:#?}",
        report.findings
    );
    let finding = &report.findings[0];
    assert_eq!(finding.rule.as_ref(), PARSE_ERROR);
    assert_eq!(finding.path.as_ref(), "broken.rs");
    assert_eq!(finding.line, 3, "the `@` line is the third line");
    assert_eq!(
        finding.column, 1,
        "the `@` line holds only the error marker"
    );
    assert_eq!(
        finding.context.as_ref(),
        "@",
        "context is the trimmed error line"
    );
}

#[test]
fn a_parse_failed_file_contributes_no_definitions_to_world_rules() {
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", HOMONYM_TOML);
    put(&root, "a.rs", RENDER_BROKEN);
    put(&root, "b.rs", RENDER_CLEAN);

    let report = run(&homonym_options(&root)).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        homonym_paths(&report).is_empty(),
        "the broken `a.rs` contributes no definition, so one clean `render` stays below min_files: {:#?}",
        report.findings
    );
}

#[test]
fn two_parseable_files_with_the_same_name_trigger_homonym() {
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", HOMONYM_TOML);
    put(&root, "a.rs", RENDER_CLEAN);
    put(&root, "b.rs", RENDER_CLEAN);

    let report = run(&homonym_options(&root)).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert_eq!(
        homonym_paths(&report),
        [("a.rs".to_string(), 1)],
        "two clean `render` definitions meet min_files with the finding at the first site: {:#?}",
        report.findings
    );
}
