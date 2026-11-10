//! What the summary counts as a skip: lintable names that are never analyzed, scoped like every other skip.
//!
//! A008/A059: a non-UTF-8 name outside the PATH scope was counted anyway.
//! A010: a symlinked source was dropped without a trace.
//! A011: pins the plan's generated-marker window of the first 2048 bytes at its edges.
//! Path identity: a valid U+FFFD name and a distinct invalid-byte name are lossy-equal
//! but must never share an explicit scope (same A008/A059 exact-scope defect).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sloplint::{Report, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";
const GENERATED_WINDOW: usize = 2048;

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&root, "sloplint.toml", "");
    (tmp, root)
}

fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .unwrap_or_else(|e| panic!("mkdir failed {}: {e}", dir.display()));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("write failed {rel}: {e}"));
}

fn scoped(root: &Path, paths: Vec<PathBuf>) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        paths,
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    }
}

fn run_ok(options: &RunOptions) -> Report {
    run(options).unwrap_or_else(|e| panic!("run failed: {e}"))
}

fn counts(report: &Report) -> (u32, u32) {
    (report.summary.files, report.summary.skipped)
}

fn reported(report: &Report) -> BTreeSet<String> {
    report.findings.iter().map(|f| f.path.to_string()).collect()
}

fn set<const N: usize>(paths: [&str; N]) -> BTreeSet<String> {
    paths.iter().map(|p| (*p).to_string()).collect()
}
#[cfg(unix)]
fn name_is_unrepresentable(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(libc::EILSEQ)
}

/// Prints the capability skip, or fails when SLOPLINT_REQUIRE_FS_CAPS demands the capability,
/// so a runner that silently drops the capability coverage turns red in CI.
#[cfg(unix)]
fn skip_unless_fs_caps_required(context: &str, error: &std::io::Error) {
    if std::env::var_os("SLOPLINT_REQUIRE_FS_CAPS").is_some_and(|value| !value.is_empty()) {
        panic!("SLOPLINT_REQUIRE_FS_CAPS is set but the capability is missing: {context}: {error}");
    }
    eprintln!("SKIP (capability unavailable) {context}: {error}");
}

/// A line comment whose `@generated` marker ends `shift` bytes after the window edge.
fn marker_near_window_edge(shift: usize) -> String {
    let marker = "@generated";
    let padding = GENERATED_WINDOW - "//".len() - marker.len() + shift;
    format!("//{}{marker}\n{SAMPLE}", " ".repeat(padding))
}

#[cfg(unix)]
#[test]
fn a_non_utf8_name_outside_the_scope_is_not_counted() {
    use std::os::unix::ffi::OsStrExt;
    let (_tmp, root) = project();
    put(&root, "src/lib.rs", SAMPLE);
    std::fs::create_dir_all(root.join("other")).unwrap_or_else(|e| panic!("mkdir failed: {e}"));
    let bad = root
        .join("other")
        .join(std::ffi::OsStr::from_bytes(b"bad\xff.rs"));
    if let Err(e) = std::fs::write(&bad, SAMPLE) {
        assert!(name_is_unrepresentable(&e), "write non-UTF-8 name: {e}");
        skip_unless_fs_caps_required(
            "a_non_utf8_name_outside_the_scope_is_not_counted: filesystem cannot represent the 0xFF name",
            &e,
        );
        return;
    }

    assert_eq!(
        counts(&run_ok(&scoped(&root, vec![]))),
        (1, 1),
        "whole root"
    );
    assert_eq!(
        counts(&run_ok(&scoped(&root, vec![root.join("src")]))),
        (1, 0),
        "the skip lies outside src"
    );
    assert_eq!(
        counts(&run_ok(&scoped(&root, vec![root.join("other")]))),
        (0, 1),
        "the skip lies inside other"
    );
}

#[cfg(unix)]
#[test]
fn lossy_equal_file_names_do_not_share_an_explicit_scope() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    let (_tmp, root) = project();
    put(&root, "other/bad\u{FFFD}.rs", SAMPLE);
    let invalid = root
        .join("other")
        .join(std::ffi::OsStr::from_bytes(b"bad\xff.rs"));
    if let Err(e) = std::fs::write(&invalid, SAMPLE) {
        assert!(name_is_unrepresentable(&e), "write non-UTF-8 name: {e}");
        skip_unless_fs_caps_required(
            "lossy_equal_file_names_do_not_share_an_explicit_scope: filesystem cannot represent the 0xFF name",
            &e,
        );
        return;
    }
    symlink("bad\u{FFFD}.rs", root.join("other/link.rs"))
        .unwrap_or_else(|e| panic!("symlink failed: {e}"));

    assert_eq!(
        counts(&run_ok(&scoped(&root, vec![]))),
        (1, 2),
        "whole root sees the valid file and both skips"
    );
    assert_eq!(
        counts(&run_ok(&scoped(
            &root,
            vec![root.join("other/bad\u{FFFD}.rs")]
        ))),
        (1, 0),
        "an explicit valid file scope must not count the lossy-equal invalid name or the sibling link"
    );
    assert_eq!(
        counts(&run_ok(&scoped(&root, vec![root.join("other")]))),
        (1, 2),
        "the parent scope includes the invalid name and the link"
    );
}

#[cfg(unix)]
#[test]
fn lossy_equal_dir_names_do_not_share_an_explicit_scope() {
    use std::os::unix::ffi::OsStrExt;
    let (_tmp, root) = project();
    put(&root, "parent/scope\u{FFFD}/a.rs", SAMPLE);
    let fifo = root.join("parent/scope\u{FFFD}/pipe.rs");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap_or_else(|e| panic!("spawn mkfifo: {e}"));
    assert!(
        status.success(),
        "mkfifo {} failed: {status}",
        fifo.display()
    );
    let invalid_dir = root
        .join("parent")
        .join(std::ffi::OsStr::from_bytes(b"scope\xff"));
    if let Err(e) = std::fs::create_dir_all(&invalid_dir) {
        assert!(name_is_unrepresentable(&e), "mkdir: {e}");
        skip_unless_fs_caps_required(
            "lossy_equal_dir_names_do_not_share_an_explicit_scope: filesystem cannot represent the 0xFF name",
            &e,
        );
        return;
    }
    std::fs::write(invalid_dir.join("b.rs"), SAMPLE)
        .unwrap_or_else(|e| panic!("write non-UTF-8 dir child: {e}"));

    assert_eq!(
        counts(&run_ok(&scoped(&root, vec![]))),
        (1, 2),
        "whole root sees the valid file and both skips"
    );
    assert_eq!(
        counts(&run_ok(&scoped(
            &root,
            vec![root.join("parent/scope\u{FFFD}")]
        ))),
        (1, 1),
        "an explicit valid directory scope must count only its own FIFO, not the lossy-equal invalid directory"
    );
    assert_eq!(
        counts(&run_ok(&scoped(&root, vec![root.join("parent")]))),
        (1, 2),
        "the valid parent includes both directories"
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_source_is_a_counted_skip_and_never_followed() {
    use std::os::unix::fs::symlink;
    let (_tmp, root) = project();
    put(&root, "shared/lib.rs", SAMPLE);
    std::fs::create_dir_all(root.join("src")).unwrap_or_else(|e| panic!("mkdir failed: {e}"));
    symlink("../shared/lib.rs", root.join("src/lib.rs"))
        .unwrap_or_else(|e| panic!("symlink failed: {e}"));
    symlink("missing.rs", root.join("src/dangling.rs"))
        .unwrap_or_else(|e| panic!("symlink failed: {e}"));
    symlink("shared", root.join("alias")).unwrap_or_else(|e| panic!("symlink failed: {e}"));

    let whole = run_ok(&scoped(&root, vec![]));
    assert_eq!(
        counts(&whole),
        (1, 2),
        "two lintable links, the plain alias is not a source"
    );
    assert_eq!(
        reported(&whole),
        set(["shared/lib.rs"]),
        "the link is never analyzed"
    );

    let in_src = run_ok(&scoped(&root, vec![root.join("src")]));
    assert_eq!(
        counts(&in_src),
        (0, 2),
        "both links lie inside the src scope and none is analyzed"
    );
    let in_shared = run_ok(&scoped(&root, vec![root.join("shared")]));
    assert_eq!(
        counts(&in_shared),
        (1, 0),
        "the real file is analyzed and no link lies inside shared"
    );
}

#[cfg(unix)]
#[test]
fn a_fifo_with_a_source_name_is_a_counted_skip_and_never_opened() {
    let (_tmp, root) = project();
    put(&root, "sample.rs", SAMPLE);
    let fifo = root.join("pipe.rs");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap_or_else(|e| panic!("spawn mkfifo: {e}"));
    assert!(
        status.success(),
        "mkfifo {} failed: {status}",
        fifo.display()
    );

    let report = run_ok(&scoped(&root, vec![]));
    assert_eq!(
        counts(&report),
        (1, 1),
        "the FIFO must be counted as a skip, not analyzed and not silently dropped"
    );
    assert_eq!(
        reported(&report),
        set(["sample.rs"]),
        "only the regular file may produce findings"
    );
}

#[test]
fn the_generated_marker_window_is_the_first_2048_bytes() {
    let (_tmp, root) = project();
    put(&root, "inside.rs", &marker_near_window_edge(0));
    put(&root, "straddle.rs", &marker_near_window_edge(1));
    put(
        &root,
        "header_only.rs",
        &format!("// Code generated by hand.\n{SAMPLE}"),
    );
    put(
        &root,
        "both_phrases.rs",
        &format!("// Code generated by tool. DO NOT EDIT.\n{SAMPLE}"),
    );

    let report = run_ok(&scoped(&root, vec![]));
    assert_eq!(
        counts(&report),
        (2, 2),
        "two files carry a marker inside the 2048-byte window and are skipped"
    );
    assert_eq!(
        reported(&report),
        set(["header_only.rs", "straddle.rs"]),
        "the marker straddling the window and a lone `Code generated` header are analyzed"
    );
}

#[test]
fn threshold_gate_fails_on_any_skip_only_when_opted_in() {
    let (_tmp, root) = project();
    put(
        &root,
        "sloplint.toml",
        "[project]\nfail_on = \"never\"\nfail_on_skipped = true\n",
    );
    put(&root, "clean.rs", SAMPLE);
    put(&root, "gen.rs", &format!("// do not edit\n{SAMPLE}"));
    let gated =
        sloplint::execute(&scoped(&root, vec![])).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        gated.failed,
        "a generated skip must fail the opted-in threshold gate"
    );
    assert_eq!(
        gated.report.summary.skipped, 1,
        "one file is skipped: {:?}",
        gated.report.summary
    );
    assert_eq!(
        gated
            .report
            .skipped_gate
            .iter()
            .map(|entry| entry.path.to_string())
            .collect::<Vec<_>>(),
        vec!["gen.rs".to_string()],
        "the gate names the skipped file"
    );
    put(&root, "sloplint.toml", "[project]\nfail_on = \"never\"\n");
    let passed =
        sloplint::execute(&scoped(&root, vec![])).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(!passed.failed, "the same skip passes when the gate is off");
    assert_eq!(
        passed.report.summary.skipped, 1,
        "the summary still counts the skip: {:?}",
        passed.report.summary
    );
}

#[test]
fn ratchet_gate_fails_only_on_newly_skipped_paths() {
    let (_tmp, root) = project();
    put(&root, "a.rs", SAMPLE);
    put(&root, "b.rs", SAMPLE);
    let mut writing = scoped(&root, vec![]);
    writing.mode = Some(Mode::Ratchet);
    writing.fail_on_skipped = true;
    writing.write_baseline = true;
    sloplint::execute(&writing).unwrap_or_else(|e| panic!("baseline write failed: {e}"));
    put(&root, "b.rs", &format!("// @generated\n{SAMPLE}"));
    let mut gated = scoped(&root, vec![]);
    gated.mode = Some(Mode::Ratchet);
    gated.fail_on_skipped = true;
    let failed = sloplint::execute(&gated).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        failed.failed,
        "a file that gained a marker since the baseline must fail the ratchet gate"
    );
    assert_eq!(
        failed
            .report
            .skipped_gate
            .iter()
            .map(|entry| entry.path.to_string())
            .collect::<Vec<_>>(),
        vec!["b.rs".to_string()],
        "the gate names the newly skipped file"
    );
    sloplint::execute(&writing).unwrap_or_else(|e| panic!("baseline rewrite failed: {e}"));
    let settled = sloplint::execute(&gated).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        !settled.failed,
        "a long-standing generated file recorded in the baseline must not fail"
    );
}
