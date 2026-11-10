use std::path::{Path, PathBuf};
use std::process::Command;

use sloplint::{RunOptions, execute};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::{Confidence, Severity};
use sloplint_core::suppress::RulePattern;
use sloplint_core::{FailOn, RuleFilter};

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir {parent:?}: {e}"));
    }
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("write {path:?}: {e}"));
}

fn filt(ids: &[&str]) -> RuleFilter {
    RuleFilter(
        ids.iter()
            .map(|s| RulePattern::parse(s).unwrap_or_else(|e| panic!("filter parse failed: {e}")))
            .collect(),
    )
}

fn gate_failed(root: &Path, min: Confidence, rule: &str) -> (bool, usize) {
    let opts = RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        fail_on: Some(FailOn::At(Severity::Info)),
        min_confidence: Some(min),
        rule_filter: filt(&[rule]),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    };
    let execution = execute(&opts).unwrap_or_else(|e| panic!("run failed: {e}"));
    (execution.failed, execution.report.findings.len())
}

#[test]
fn min_confidence_high_drops_low_confidence_findings_from_the_gate() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    write(root, "src/widget.ts", "export function widget(): void {}\n");
    write(root, "sloplint.toml", "");
    let (failed_low, count_low) = gate_failed(root, Confidence::Low, "agent/colocated-tests");
    assert!(
        count_low > 0,
        "colocated-tests must fire without a test file"
    );
    assert!(failed_low, "low confidence findings must fail with min low");
    let (failed_high, count_high) = gate_failed(root, Confidence::High, "agent/colocated-tests");
    assert_eq!(
        count_high, count_low,
        "min_confidence filters the gate, never the reported findings"
    );
    assert!(
        !failed_high,
        "low confidence findings must not fail with min high"
    );
}

#[test]
fn bench_glob_file_gets_bench_shifts() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    write(root, "src/handle.rs", "pub fn handle() {}\n");
    write(root, "benches/handle.rs", "pub fn handle() {}\n");
    write(root, "sloplint.toml", "");
    let opts = RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name"]),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    };
    let report = sloplint::run(&opts).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.path.as_ref() == "src/handle.rs"),
        "library file must report generic-name"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.path.as_ref() == "benches/handle.rs"),
        "bench file must be shifted off for generic-name"
    );
}

#[test]
fn support_glob_file_gets_support_shifts() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    write(root, "tests/handle.rs", "pub fn handle() {}\n");
    write(root, "tests/common/handle.rs", "pub fn handle() {}\n");
    write(root, "sloplint.toml", "");
    let opts = RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name"]),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    };
    let report = sloplint::run(&opts).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.path.as_ref() == "tests/handle.rs"),
        "test file must be shifted off for generic-name"
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.path.as_ref() == "tests/common/handle.rs"),
        "support file must report generic-name"
    );
}

#[test]
fn data_glob_file_is_not_analyzed() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    write(root, "src/e.rs", "pub fn handle() {}\n");
    write(root, "testdata/e.rs", "pub fn handle() {}\n");
    write(root, "sloplint.toml", "");
    let opts = RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name"]),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    };
    let report = sloplint::run(&opts).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.path.as_ref() == "src/e.rs"),
        "library file must report"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.path.as_ref() == "testdata/e.rs"),
        "data file must not be analyzed"
    );
}

fn sloplint_bin(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn sloplint: {e}"))
}

#[test]
fn agent_line_and_summary_match_the_contract() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root: PathBuf = tmp.path().to_path_buf();
    write(&root, "sloplint.toml", "");
    write(&root, "f.rs", "pub fn handle() {}\n");
    let out = sloplint_bin(
        &root,
        &["check", "--no-cache", "--rule", "grep/generic-name"],
    );
    assert_eq!(out.status.code(), Some(0), "minor must pass the major gate");
    let stdout = String::from_utf8(out.stdout).unwrap_or_else(|e| panic!("utf8: {e}"));
    let mut lines: Vec<&str> = stdout.lines().collect();
    let summary = lines
        .pop()
        .unwrap_or_else(|| panic!("missing summary: {stdout}"));
    assert_eq!(
        summary,
        "summary: strictness=standard files=1 findings=1 blocker=0 critical=0 major=0 minor=1 info=0 baselined=0 stale=0 skipped=0",
        "summary format"
    );
    assert_eq!(
        lines.len(),
        6,
        "agent block is header plus context plus fix: {lines:?}"
    );
    assert_eq!(
        lines[0],
        "f.rs:1:8: minor/medium: `handle` is a generic name shared by unrelated code [grep/generic-name]",
        "agent line format"
    );
    assert_eq!(lines[1], "  context: pub fn handle() {}", "context line");
    assert!(lines[2].starts_with("  fix: "), "fix line: {}", lines[2]);
    assert_eq!(lines[3], "", "blank line before rules");
    assert_eq!(lines[4], "rules:", "rules header: {}", lines[4]);
    assert!(
        lines[5].starts_with("  grep/generic-name: "),
        "rules entry: {}",
        lines[5]
    );
    let out = sloplint_bin(
        &root,
        &[
            "check",
            "--no-cache",
            "--format",
            "json",
            "--rule",
            "grep/generic-name",
        ],
    );
    #[derive(serde::Deserialize)]
    struct JsonFinding {
        severity: String,
        confidence: String,
    }
    #[derive(serde::Deserialize)]
    struct JsonSummary {
        minor: usize,
        blocker: usize,
    }
    #[derive(serde::Deserialize)]
    struct JsonDoc {
        findings: Vec<JsonFinding>,
        summary: JsonSummary,
    }
    let doc: JsonDoc = sonic_rs::from_slice(&out.stdout).unwrap_or_else(|e| panic!("json: {e}"));
    assert_eq!(doc.findings[0].severity, "minor", "json severity rung");
    assert_eq!(doc.findings[0].confidence, "medium", "json confidence");
    assert_eq!(doc.summary.minor, 1, "json minor count");
    assert_eq!(doc.summary.blocker, 0, "json blocker count");
}

#[test]
fn human_format_shows_rung_and_confidence_for_major_and_minor() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root: PathBuf = tmp.path().to_path_buf();
    write(
        &root,
        "sloplint.toml",
        "[[custom_rules]]\nid = \"custom/major-hit\"\nlanguage = \"rust\"\nseverity = \"major\"\nmessage = \"major hit {match}\"\nwhy = \"test why\"\nfix = \"test fix\"\nquery = \"(call_expression) @match\"\n[[custom_rules]]\nid = \"custom/minor-hit\"\nlanguage = \"rust\"\nseverity = \"minor\"\nmessage = \"minor hit {match}\"\nwhy = \"test why\"\nfix = \"test fix\"\nquery = \"(call_expression) @match\"\n",
    );
    write(&root, "f.rs", "pub fn handle() { foo(); }\nfn foo() {}\n");
    let out = sloplint_bin(
        &root,
        &[
            "check",
            "--no-cache",
            "--format",
            "human",
            "--rule",
            "custom/major-hit",
            "--rule",
            "custom/minor-hit",
        ],
    );
    let stdout = String::from_utf8(out.stdout).unwrap_or_else(|e| panic!("utf8: {e}"));
    assert!(
        stdout.contains("major/medium"),
        "human must show the major rung and confidence: {stdout}"
    );
    assert!(
        stdout.contains("minor/medium"),
        "human must show the minor rung and confidence: {stdout}"
    );
}
