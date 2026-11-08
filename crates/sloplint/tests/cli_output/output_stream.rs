//! Buffered stdout keeps every output format whole and ordered across buffer boundaries.
//!
//! Each case produces several buffers of output, so a missing final flush, a truncated tail,
//! dropped or reordered findings, a broken JSON frame, or a wrong exit code fails it. Cases
//! scope `--rule` to one per-file rule so a world rule cannot add findings to the counts.

use std::path::Path;
use std::process::{Command, Output};

use serde::Deserialize;

const MANY: usize = 1500;
const BUFFER_BYTES: usize = 64 * 1024;
const RULE: &str = "grep/generic-name";
const SUMMARY_MANY: &str = concat!(
    "summary: strictness=standard files=1500 findings=1500 ",
    "deny=0 warn=1500 baselined=0 stale=0 skipped=0"
);

#[derive(Deserialize)]
struct Row {
    path: String,
}

#[derive(Deserialize)]
struct Summary {
    files: usize,
    findings: usize,
    deny: usize,
    warn: usize,
    baselined: usize,
    stale: usize,
    skipped: usize,
}

#[derive(Deserialize)]
struct Document {
    findings: Vec<Row>,
    summary: Summary,
}

fn project(files: usize) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    put(tmp.path(), "sloplint.toml", "");
    for index in 0..files {
        put(
            tmp.path(),
            &format!("f{index:04}.rs"),
            "pub fn handle() {}\n",
        );
    }
    tmp
}

fn put(root: &Path, rel: &str, body: &str) {
    std::fs::write(root.join(rel), body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn sloplint(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn sloplint: {e}"))
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or_else(|e| panic!("output is not UTF-8: {e}"))
}

// A124: several buffers of JSON still parse to the full summary and every finding in path order.
#[test]
fn json_output_larger_than_the_buffer_arrives_whole() {
    let tmp = project(MANY);
    let out = sloplint(
        tmp.path(),
        &["check", "--no-cache", "--format", "json", "--rule", RULE],
    );
    assert_eq!(out.status.code(), Some(0), "exit code");
    assert!(
        out.stdout.len() > 4 * BUFFER_BYTES,
        "fixture is too small to cross buffer boundaries: {} bytes",
        out.stdout.len()
    );
    assert!(out.stdout.ends_with(b"}\n"), "missing final newline");
    let doc: Document =
        sonic_rs::from_slice(&out.stdout).unwrap_or_else(|e| panic!("stdout is not JSON: {e}"));
    assert_eq!(doc.summary.files, MANY, "summary file count");
    assert_eq!(doc.summary.findings, MANY, "summary finding count");
    assert_eq!(doc.summary.deny, 0, "summary deny count");
    assert_eq!(doc.summary.warn, MANY, "summary warn count");
    assert_eq!(doc.summary.baselined, 0, "summary baselined count");
    assert_eq!(doc.summary.stale, 0, "summary stale count");
    assert_eq!(doc.summary.skipped, 0, "summary skipped count");
    let paths: Vec<&str> = doc.findings.iter().map(|row| row.path.as_str()).collect();
    let expected: Vec<String> = (0..MANY).map(|index| format!("f{index:04}.rs")).collect();
    assert_eq!(
        paths, expected,
        "findings were dropped, duplicated, or reordered"
    );
}

// A124: agent lines cross many buffer boundaries and the exit code still follows the findings.
#[test]
fn agent_output_larger_than_the_buffer_is_complete_and_ordered() {
    let tmp = project(MANY);
    let out = sloplint(
        tmp.path(),
        &["check", "--no-cache", "--fail-on", "warn", "--rule", RULE],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "warn findings must fail under --fail-on warn"
    );
    assert!(
        out.stdout.len() > 4 * BUFFER_BYTES,
        "fixture is too small to cross buffer boundaries: {} bytes",
        out.stdout.len()
    );
    let stdout = text(&out.stdout);
    assert!(stdout.ends_with('\n'), "missing final newline");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3 * MANY + 4, "line count");
    for index in 0..MANY {
        let header = lines[3 * index];
        let prefix = format!("f{index:04}.rs:1:8: warn: ");
        assert!(header.starts_with(&prefix), "finding {index}: {header}");
        assert_eq!(
            lines[3 * index + 1],
            "  context: pub fn handle() {}",
            "finding {index}: context line"
        );
        assert!(
            lines[3 * index + 2].starts_with("  fix: "),
            "finding {index}: fix line was {:?}",
            lines[3 * index + 2]
        );
    }
    assert_eq!(lines[3 * MANY], "", "blank line before the rules block");
    assert_eq!(lines[3 * MANY + 1], "rules:", "rules block header");
    assert!(
        lines[3 * MANY + 2].starts_with(&format!("  {RULE}: ")),
        "rules block entry was {:?}",
        lines[3 * MANY + 2]
    );
    assert_eq!(
        lines[3 * MANY + 3],
        SUMMARY_MANY,
        "the last line must be the summary"
    );
}

// A124: the multi-line human renderer still delivers every finding and its buffered summary.
#[test]
fn human_output_larger_than_the_buffer_ends_with_the_summary() {
    let tmp = project(MANY);
    let out = sloplint(
        tmp.path(),
        &["check", "--no-cache", "--format", "human", "--rule", RULE],
    );
    assert_eq!(out.status.code(), Some(0), "exit code");
    assert!(
        out.stdout.len() > 4 * BUFFER_BYTES,
        "fixture is too small to cross buffer boundaries: {} bytes",
        out.stdout.len()
    );
    let stdout = text(&out.stdout);
    let marker = format!("warning[{RULE}]");
    assert_eq!(
        stdout.matches(marker.as_str()).count(),
        MANY,
        "finding blocks must each appear exactly once"
    );
    let mut cursor = 0;
    for index in 0..MANY {
        let identity = format!("f{index:04}.rs:1:8");
        let at = stdout[cursor..]
            .find(identity.as_str())
            .unwrap_or_else(|| panic!("finding {index} ({identity}) missing or out of order"));
        cursor += at + identity.len();
    }
    assert!(
        stdout.ends_with(&format!("{SUMMARY_MANY}\n")),
        "human output does not end with the summary line"
    );
}
