//! The human renderer shows each finding's own source line, whatever the line endings or count.
//!
//! Cases cover the boundaries of the per-file line index: CRLF endings, a last line without a
//! newline, blank lines, deep line numbers, and consecutive files.

use std::path::Path;
use std::process::{Command, Output};

const DEEP: usize = 2000;

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    std::fs::write(tmp.path().join("sloplint.toml"), "")
        .unwrap_or_else(|e| panic!("write sloplint.toml: {e}"));
    for (rel, body) in files {
        std::fs::write(tmp.path().join(rel), body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
    }
    tmp
}

fn human(cwd: &Path, rule: &str) -> String {
    let out: Output = Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(cwd)
        .args(["check", "--no-cache", "--format", "human", "--rule", rule])
        .output()
        .unwrap_or_else(|e| panic!("spawn sloplint: {e}"));
    assert_eq!(out.status.code(), Some(0), "exit code");
    String::from_utf8(out.stdout).unwrap_or_else(|e| panic!("output is not UTF-8: {e}"))
}

fn snippet_rows(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter(|row| row.contains(" \u{2502} pub fn "))
        .collect()
}

// A125: CRLF endings are stripped from the snippet and the last line needs no newline.
#[test]
fn crlf_file_without_a_final_newline_shows_the_right_lines() {
    let tmp = project(&[(
        "a.rs",
        "pub fn handle() {}\r\n\r\npub fn process() {}\r\n\r\npub fn execute() {}",
    )]);
    let output = human(tmp.path(), "grep/generic-name");
    assert!(
        !output.contains('\r'),
        "carriage return leaked:\n{output:?}"
    );
    assert_eq!(
        snippet_rows(&output),
        [
            "1 \u{2502} pub fn handle() {}",
            "3 \u{2502} pub fn process() {}",
            "5 \u{2502} pub fn execute() {}",
        ],
        "each finding must show its own line of a CRLF file with no final newline"
    );
}

// A125: a leading blank line and a trailing newline must not shift or invent lines.
#[test]
fn lf_file_with_leading_blank_line_and_final_newline_shows_the_right_lines() {
    let tmp = project(&[("a.rs", "\npub fn handle() {}\n\n\npub fn run() {}\n")]);
    let output = human(tmp.path(), "grep/generic-name");
    assert_eq!(
        snippet_rows(&output),
        [
            "2 \u{2502} pub fn handle() {}",
            "5 \u{2502} pub fn run() {}",
        ],
        "a leading blank line and a final newline must not shift or invent source lines"
    );
}

// A125: the source cache follows the sorted findings from one file to the next.
#[test]
fn consecutive_files_each_show_their_own_source() {
    let tmp = project(&[
        ("a.rs", "pub fn handle() {}\n"),
        ("b.rs", "// padding\n\npub fn process() {}\n"),
        ("c.rs", "pub fn execute() {}\npub fn run() {}\n"),
    ]);
    let output = human(tmp.path(), "grep/generic-name");
    assert_eq!(
        snippet_rows(&output),
        [
            "1 \u{2502} pub fn handle() {}",
            "3 \u{2502} pub fn process() {}",
            "1 \u{2502} pub fn execute() {}",
            "2 \u{2502} pub fn run() {}",
        ],
        "each of three consecutive files must show its own source lines"
    );
}

// A125: every one of thousands of findings in one file resolves to its own line number.
#[test]
fn many_findings_in_one_file_resolve_their_own_lines() {
    let body = "pub fn handle() {}\n".repeat(DEEP);
    let tmp = project(&[("a.rs", body.as_str())]);
    let output = human(tmp.path(), "grep/generic-name");
    let blocks: Vec<&str> = output.split("\n\n").collect();
    assert_eq!(
        blocks.len(),
        DEEP + 1,
        "one block per finding plus the summary"
    );
    for (index, block) in blocks.iter().take(DEEP).enumerate() {
        let line = index + 1;
        let row = format!("{line} \u{2502} pub fn handle() {{}}");
        assert!(
            block.contains(&format!("a.rs:{line}:8")),
            "block {index} has the wrong location:\n{block}"
        );
        assert!(
            block.lines().any(|candidate| candidate == row),
            "block {index} shows the wrong source line:\n{block}"
        );
    }
}

// A125: edge-case line endings resolve correctly through the unified line_starts index.
// Edge differences between the old Source index and core line_starts helper:
// 1. Empty file (""): old index was empty (line(1) -> None); line_starts returns [0] (line(1) -> Some("")).
//    Rendered fallback maps both to empty string.
// 2. Trailing newline ("a\n"): old index filtered start < text.len() (line(2) -> None); line_starts returns [0, 2] (line(2) -> Some("")).
//    Rendered fallback maps both to empty string.
// 3. CRLF ("\r\n"): both split on '\n' alone; strip_suffix('\r') handles the carriage return.
// 4. Lone CR ('\r'): neither splits on '\r'; content remains on line 1 and is escaped as \x0d.
// 5. Last line without newline ("a\nb"): both include the final line offset in starts.
#[test]
fn edge_case_line_endings_render_expected_snippets() {
    let tmp = project(&[
        ("trailing_lf.rs", "pub fn handle() {}\n"),
        ("no_trailing_lf.rs", "pub fn handle() {}"),
        ("trailing_crlf.rs", "pub fn handle() {}\r\n"),
        ("lone_cr.rs", "pub fn handle() {}\rpub fn run() {}\n"),
        ("empty.rs", ""),
    ]);
    let output = human(tmp.path(), "grep/generic-name");
    assert!(
        output.contains("trailing_lf.rs:1:8"),
        "trailing LF missing in:\n{output}"
    );
    assert!(
        output.contains("no_trailing_lf.rs:1:8"),
        "no trailing LF missing in:\n{output}"
    );
    assert!(
        output.contains("trailing_crlf.rs:1:8"),
        "trailing CRLF missing in:\n{output}"
    );
    assert!(
        output.contains("lone_cr.rs:1:8"),
        "lone CR missing in:\n{output}"
    );
    assert!(
        output.contains("1 \u{2502} pub fn handle() {}\\x0dpub fn run() {}"),
        "lone CR must render escaped and not split lines in:\n{output}"
    );
}
