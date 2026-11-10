//! Custom query execution is bounded per rule per file and the cap surfaces as
//! a meta finding naming the rule.
//!
//! F10: repository-supplied custom queries ran unbounded; a query over sibling
//! identifiers grows quadratically and could exhaust memory or hang CI.

use std::path::{Path, PathBuf};

use sloplint::{Report, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const CAP: &str = "meta/custom-cap";
const PAIRS: &str = "((identifier) @match (identifier) @other)";

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&root, "sloplint.toml", &pair_rule());
    (tmp, root)
}

fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .unwrap_or_else(|e| panic!("could not create parent for {rel}: {e}"));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("could not write fixture {rel}: {e}"));
}

fn pair_rule() -> String {
    format!(
        "[[custom_rules]]\nid = \"custom/pair-flood\"\nlanguage = \"rust\"\nseverity = \"minor\"\nmessage = \"pair\"\nwhy = \"unbounded sibling combination\"\nfix = \"anchor the query\"\nquery = \"{PAIRS}\"\n"
    )
}

fn use_list(count: usize) -> String {
    let names: String = (0..count)
        .map(|index| format!("flood_alpha_{index:03}, "))
        .collect();
    format!("use crate::{{{names}}};\n")
}

fn options(root: &Path, selectors: &[&str]) -> RunOptions {
    let patterns = selectors
        .iter()
        .map(|selector| {
            RulePattern::parse(selector)
                .unwrap_or_else(|e| panic!("rule selector {selector} did not parse: {e}"))
        })
        .collect();
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(patterns),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    }
}

fn reported(report: &Report) -> Vec<(String, u32, &'static str, String)> {
    report
        .findings
        .iter()
        .map(|finding| {
            (
                finding.path.to_string(),
                finding.line,
                finding.severity.as_str(),
                finding.rule.to_string(),
            )
        })
        .collect()
}

#[test]
fn a_sibling_combination_query_is_capped_and_names_the_rule() {
    let (_tmp, root) = project();
    put(&root, "src/main.rs", &use_list(60));

    let report = run(&options(&root, &["meta/*"])).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert_eq!(
        reported(&report),
        vec![("src/main.rs".to_string(), 1, "major", CAP.to_string())],
        "{:#?}",
        report.findings
    );
    assert_eq!(report.summary.findings, 1);
    assert_eq!(report.summary.major, 1);
}

#[test]
fn a_query_under_the_cap_reports_its_pairs_without_a_cap_finding() {
    let (_tmp, root) = project();
    put(&root, "src/main.rs", &use_list(5));

    let report =
        run(&options(&root, &["meta/*", "custom/*"])).unwrap_or_else(|e| panic!("run failed: {e}"));
    let findings = reported(&report);
    assert!(
        findings
            .iter()
            .all(|(_, _, _, rule)| rule == "custom/pair-flood"),
        "{findings:#?}"
    );
    assert_eq!(findings.len(), 10, "{findings:#?}");
    assert_eq!(report.summary.findings, 10);
    assert_eq!(report.summary.major, 0);
}
