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
/// The number printed in the gutter of the block's source row: the digits before the ` │ `
/// separator on the one row whose gutter holds a number. Annotation and fold rows share the
/// separator but leave the gutter blank.
fn snippet_row_number(block: &str) -> Option<u32> {
    block.lines().find_map(|row| {
        row.split_once(" │ ")
            .and_then(|(gutter, _)| gutter.trim().parse().ok())
    })
}

fn assert_each_finding_shows_its_source_line(output: &str, expected: &[(&str, u32, &str)]) {
    assert!(
        !output.contains('\r'),
        "a raw carriage return reached the output:\n{output:?}"
    );
    let blocks: Vec<&str> = output.split("\n\n").collect();
    let mut start = 0;
    for &(path, line, source) in expected {
        let location = format!("{path}:{line}:");
        let index = blocks
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, block)| block.contains(&location))
            .map(|(index, _)| index)
            .unwrap_or_else(|| panic!("missing {location} in:\n{output}"));
        let block = blocks[index];
        assert!(
            block.contains(source),
            "{location} does not show {source:?} in:\n{block}"
        );
        assert_eq!(
            snippet_row_number(block),
            Some(line),
            "{location} must label its source row with the finding's line number"
        );
        start = index + 1;
    }
    let printed: Vec<u32> = blocks
        .iter()
        .filter_map(|block| snippet_row_number(block))
        .collect();
    let wanted: Vec<u32> = expected.iter().map(|&(_, line, _)| line).collect();
    assert_eq!(
        printed, wanted,
        "every printed snippet row must be one finding's own line, in order"
    );
}

#[test]
fn crlf_file_without_a_final_newline_shows_the_right_lines() {
    let tmp = project(&[(
        "a.rs",
        "pub fn handle() {}\r\n\r\npub fn process() {}\r\n\r\npub fn process() {}",
    )]);
    let output = human(tmp.path(), "grep/generic-name");
    assert!(
        !output.contains('\r'),
        "carriage return leaked:\n{output:?}"
    );
    assert_each_finding_shows_its_source_line(
        &output,
        &[
            ("a.rs", 1, "pub fn handle() {}"),
            ("a.rs", 3, "pub fn process() {}"),
            ("a.rs", 5, "pub fn process() {}"),
        ],
    );
}

#[test]
fn lf_file_with_leading_blank_line_and_final_newline_shows_the_right_lines() {
    let tmp = project(&[("a.rs", "\npub fn handle() {}\n\n\npub fn process() {}\n")]);
    let output = human(tmp.path(), "grep/generic-name");
    assert_each_finding_shows_its_source_line(
        &output,
        &[
            ("a.rs", 2, "pub fn handle() {}"),
            ("a.rs", 5, "pub fn process() {}"),
        ],
    );
}

#[test]
fn consecutive_files_each_show_their_own_source() {
    let tmp = project(&[
        ("a.rs", "pub fn handle() {}\n"),
        ("b.rs", "// padding\n\npub fn process() {}\n"),
        ("c.rs", "pub fn handle() {}\npub fn process() {}\n"),
    ]);
    let output = human(tmp.path(), "grep/generic-name");
    assert_each_finding_shows_its_source_line(
        &output,
        &[
            ("a.rs", 1, "pub fn handle() {}"),
            ("b.rs", 3, "pub fn process() {}"),
            ("c.rs", 1, "pub fn handle() {}"),
            ("c.rs", 2, "pub fn process() {}"),
        ],
    );
}

#[test]
fn many_findings_in_one_file_resolve_their_own_lines() {
    let body = "pub fn handle() {}\n".repeat(DEEP);
    let tmp = project(&[("a.rs", body.as_str())]);
    let output = human(tmp.path(), "grep/generic-name");
    let expected: Vec<(&str, u32, &str)> = (1..=DEEP)
        .map(|line| ("a.rs", line as u32, "pub fn handle() {}"))
        .collect();
    assert_each_finding_shows_its_source_line(&output, &expected);
}

#[test]
fn edge_case_line_endings_render_expected_snippets() {
    let tmp = project(&[
        ("trailing_lf.rs", "pub fn handle() {}\n"),
        ("no_trailing_lf.rs", "pub fn handle() {}"),
        ("trailing_crlf.rs", "pub fn handle() {}\r\n"),
        ("lone_cr.rs", "pub fn handle() {}\rpub fn process() {}\n"),
        ("empty.rs", ""),
    ]);
    let output = human(tmp.path(), "grep/generic-name");
    assert!(
        !output.contains('\r'),
        "a raw carriage return reached the output:\n{output:?}"
    );
    assert_each_finding_shows_its_source_line(
        &output,
        &[
            ("lone_cr.rs", 1, "pub fn handle() {}"),
            ("lone_cr.rs", 1, "pub fn process() {}"),
            ("no_trailing_lf.rs", 1, "pub fn handle() {}"),
            ("trailing_crlf.rs", 1, "pub fn handle() {}"),
            ("trailing_lf.rs", 1, "pub fn handle() {}"),
        ],
    );
    let lone = output
        .split("\n\n")
        .find(|block| block.contains("lone_cr.rs:1:"))
        .unwrap_or_else(|| panic!("missing lone_cr finding in:\n{output}"));
    assert!(
        lone.contains("pub fn process() {}"),
        "a lone carriage return must not split the line:\n{lone}"
    );
}
