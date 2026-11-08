//! `--rule` under ratchet mode: baseline entries of unselected rules are not stale.
//!
//! A007/A060: the rule filter ran before the baseline gate, so every entry of another
//! rule matched no finding and inflated `stale` on a clean tree.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sloplint::{Report, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::{RuleFilter, RulePattern};

const BASELINE: &str = "sloplint-baseline.json";
const SOURCE: &str = "/// Process the order.\npub fn process_order(id: &str, account: &str, qty: u64) {}\npub fn handle(flag: bool) {}\n";

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&root, "sloplint.toml", "");
    put(&root, "sample.rs", SOURCE);
    (tmp, root)
}

fn put(root: &Path, rel: &str, body: &str) {
    std::fs::write(root.join(rel), body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn checking(root: &Path, mode: Mode, rules: &[&str]) -> RunOptions {
    let patterns = rules
        .iter()
        .map(|rule| RulePattern::parse(rule).unwrap_or_else(|e| panic!("rule {rule}: {e}")))
        .collect();
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(mode),
        rule_filter: RuleFilter(patterns),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    }
}

fn recording(root: &Path) -> RunOptions {
    RunOptions {
        write_baseline: true,
        ..checking(root, Mode::Threshold, &[])
    }
}

fn run_ok(options: &RunOptions) -> Report {
    run(options).unwrap_or_else(|e| panic!("run failed: {e}"))
}

/// `(reported findings, baselined, stale)`.
fn tally(report: &Report) -> (usize, u32, u32) {
    (
        report.findings.len(),
        report.summary.baselined,
        report.summary.stale,
    )
}

fn count_of(findings: &[sloplint_core::Finding], rule: &str) -> u32 {
    let hits = findings.iter().filter(|f| &*f.rule == rule).count();
    u32::try_from(hits).unwrap_or_else(|e| panic!("count: {e}"))
}

fn baseline_with_extra(findings: &[sloplint_core::Finding], extra_rule: &str) -> String {
    let mut entries: Vec<String> = findings
        .iter()
        .map(|f| {
            format!(
                r#"{{"path":"{}","rule":"{}","fingerprint":"{}"}}"#,
                f.path, f.rule, f.fingerprint
            )
        })
        .collect();
    entries.push(format!(
        r#"{{"path":"sample.rs","rule":"{extra_rule}","fingerprint":"00000000000000000000000000000000"}}"#
    ));
    format!(r#"{{"version":1,"findings":[{}]}}"#, entries.join(","))
}

#[test]
fn entries_of_unselected_rules_are_not_stale() {
    let (_tmp, root) = project();
    let recorded = run_ok(&recording(&root));
    let rules: BTreeSet<String> = recorded
        .findings
        .iter()
        .map(|f| f.rule.to_string())
        .collect();
    assert!(
        rules.len() >= 2,
        "the sample must trip two rules: {rules:?}"
    );
    let total = recorded.findings.len();

    let all = run_ok(&checking(&root, Mode::Ratchet, &[]));
    assert_eq!(
        tally(&all),
        (0, u32::try_from(total).unwrap_or(u32::MAX), 0),
        "unfiltered control"
    );

    for rule in &rules {
        let selected = run_ok(&checking(&root, Mode::Ratchet, &[rule.as_str()]));
        assert_eq!(
            tally(&selected),
            (0, count_of(&recorded.findings, rule), 0),
            "--rule {rule}"
        );
    }

    let silent = run_ok(&checking(&root, Mode::Ratchet, &["meta/parse-error"]));
    assert_eq!(tally(&silent), (0, 0, 0), "a rule with no baseline entries");
}

#[test]
fn a_genuinely_stale_entry_counts_only_when_its_rule_is_selected() {
    let (_tmp, root) = project();
    let recorded = run_ok(&recording(&root));
    let rules: Vec<String> = recorded
        .findings
        .iter()
        .map(|f| f.rule.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let (stale_rule, other_rule) = (&rules[0], &rules[1]);
    put(
        &root,
        BASELINE,
        &baseline_with_extra(&recorded.findings, stale_rule),
    );

    let unfiltered = run_ok(&checking(&root, Mode::Ratchet, &[]));
    assert_eq!(unfiltered.summary.stale, 1, "unfiltered");
    let own = run_ok(&checking(&root, Mode::Ratchet, &[stale_rule.as_str()]));
    assert_eq!(own.summary.stale, 1, "--rule {stale_rule}");
    let other = run_ok(&checking(&root, Mode::Ratchet, &[other_rule.as_str()]));
    assert_eq!(other.summary.stale, 0, "--rule {other_rule}");
}
