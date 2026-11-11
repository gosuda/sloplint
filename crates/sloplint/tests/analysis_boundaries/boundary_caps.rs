//! File-byte caps and unrepresentable names: the counted skips at the analysis boundary.

use super::*;

// AA-I11, AA-R1: limit-1 skipped, limit and limit+1 analyzed.
#[test]
fn boundary_byte_cap_limit_boundaries() {
    let (_t, root) = tmp();
    let body = "pub fn handle() {}\n";
    put(&root, "src/a.rs", body);
    let len = std::fs::metadata(root.join("src/a.rs"))
        .unwrap_or_else(|e| panic!("metadata failed: {e}"))
        .len();
    for (limit, files, skipped, hits) in [(len - 1, 0, 1, 0), (len, 1, 0, 1), (len + 1, 1, 0, 1)] {
        put(
            &root,
            "sloplint.toml",
            &format!("[project]\nmax_file_bytes = {limit}\n"),
        );
        let r = run_ok(&RunOptions {
            strictness: Some(Strictness::Standard),
            mode: Some(Mode::Threshold),
            rule_filter: filt(&["grep/generic-name", "meta/*"]),
            ..base(&root)
        });
        assert_eq!(r.summary.files, files, "files at limit {limit}");
        assert_eq!(r.summary.skipped, skipped, "skipped at limit {limit}");
        let g: Vec<_> = r
            .findings
            .iter()
            .filter(|f| &*f.rule == "grep/generic-name")
            .collect();
        assert_eq!(g.len(), hits, "generic-name count at limit {limit}");
        if hits == 1 {
            assert_eq!(&*g[0].path, "src/a.rs");
            assert_eq!(g[0].line, 1);
            assert_eq!(g[0].severity, Severity::Minor);
        }
    }
}

// AA-I12, AA-R1: zero cap disables analysis; non-UTF8 content is a counted skip.
#[test]
fn boundary_zero_cap_and_non_utf8_skip() {
    let (_t, root) = tmp();
    put(&root, "src/a.rs", "pub fn handle() {}\n");
    put(&root, "sloplint.toml", "[project]\nmax_file_bytes = 0\n");
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name", "meta/*"]),
        ..base(&root)
    });
    assert_eq!(r.summary.files, 0, "zero cap analyzes nothing");
    assert_eq!(r.summary.skipped, 1, "zero cap counts the skip");
    assert!(r.findings.is_empty(), "zero cap reports nothing");
    let (_t2, root2) = tmp();
    put(&root2, "src/a.rs", "pub fn handle() {}\n");
    std::fs::write(root2.join("src/bad.rs"), [0xffu8, 0xfe, 0x41]).unwrap();
    let r2 = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name", "meta/*"]),
        ..base(&root2)
    });
    assert_eq!(r2.summary.files, 1, "only the valid file is analyzed");
    assert_eq!(r2.summary.skipped, 1, "non-UTF8 content is a counted skip");
    assert!(
        r2.findings.iter().all(|f| &*f.path != "src/bad.rs"),
        "non-UTF8 file reports nothing"
    );
    assert!(
        r2.findings
            .iter()
            .any(|f| &*f.rule == "grep/generic-name" && &*f.path == "src/a.rs" && f.line == 1),
        "valid file still reports"
    );
}

// AA-I15: a non-UTF-8 file name is a counted skip; the valid sibling still reports.
#[cfg(unix)]
#[test]
fn boundary_non_utf8_filename_is_skipped() {
    use std::os::unix::ffi::OsStrExt;

    let (_t, root) = tmp();
    put(&root, "src/a.rs", "pub fn handle() {}\n");
    let raw = std::ffi::OsStr::from_bytes(b"src/bad\xff.rs");
    if let Err(e) = std::fs::write(root.join(raw), b"pub fn handle() {}\n") {
        const EILSEQ: i32 = 92;
        assert_eq!(e.raw_os_error(), Some(EILSEQ), "write non-UTF-8 name: {e}");
        skip_unless_fs_caps_required(
            "boundary_non_utf8_filename_is_skipped: filesystem cannot represent the 0xFF name",
            &e,
        );
        return;
    }
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name", "meta/*"]),
        ..base(&root)
    });
    assert_eq!(r.summary.files, 1, "only the valid file is analyzed");
    assert_eq!(r.summary.skipped, 1, "non-UTF8 name is a counted skip");
    let generic: Vec<_> = r
        .findings
        .iter()
        .filter(|f| &*f.rule == "grep/generic-name")
        .collect();
    assert_eq!(generic.len(), 1, "exactly one generic-name finding");
    assert_eq!(
        &*generic[0].path, "src/a.rs",
        "finding cites the valid path"
    );
    assert_eq!(generic[0].line, 1);
    assert_eq!(generic[0].severity, Severity::Minor);
    assert!(
        r.findings.iter().all(|f| &*f.path == "src/a.rs"),
        "no finding may cite the unnamed file: {:?}",
        r.findings.iter().map(|f| &*f.path).collect::<Vec<_>>()
    );
}
