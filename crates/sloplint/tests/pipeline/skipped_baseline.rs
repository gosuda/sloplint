//! Skip gates and ratchet baselines: opted-in gate failures, true skip reasons, and distinct invalid-byte names.

#[cfg(unix)]
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sloplint::RunOptions;
use sloplint_core::config::{Mode, Strictness};

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";

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

#[cfg(unix)]
#[test]
fn skipped_gate_names_the_true_skip_reason() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", "[project]\nfail_on = \"never\"\n");
    put(&root, "sample.rs", SAMPLE);
    symlink("sample.rs", root.join("link.rs")).unwrap_or_else(|e| panic!("symlink failed: {e}"));
    let status = std::process::Command::new("mkfifo")
        .arg(root.join("pipe.rs"))
        .status()
        .unwrap_or_else(|e| panic!("spawn mkfifo: {e}"));
    assert!(status.success(), "mkfifo failed: {status}");
    let bad = root.join(std::ffi::OsStr::from_bytes(b"bad\xff.rs"));
    if let Err(error) = std::fs::write(&bad, SAMPLE) {
        assert!(
            name_is_unrepresentable(&error),
            "write non-UTF-8 name: {error}"
        );
        skip_unless_fs_caps_required(
            "skip reasons need a representable invalid-byte name",
            &error,
        );
        return;
    }
    let mut gated = scoped(&root, vec![]);
    gated.fail_on_skipped = true;
    let execution = sloplint::execute(&gated).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(execution.failed, "skips must fail the opted-in gate");
    assert_eq!(
        execution.report.summary.skipped, 3,
        "link, FIFO, and bad name are three skips"
    );
    let reasons: BTreeSet<(String, String)> = execution
        .report
        .skipped_gate
        .iter()
        .map(|entry| (entry.path.to_string(), entry.reason.to_string()))
        .collect();
    assert!(
        reasons.contains(&("link.rs".to_string(), "symlink".to_string())),
        "a symlink reports its kind: {reasons:?}"
    );
    assert!(
        reasons.contains(&("pipe.rs".to_string(), "non-regular".to_string())),
        "a FIFO is not a symlink: {reasons:?}"
    );
    assert!(
        reasons
            .iter()
            .any(|(path, reason)| reason == "non-utf8-name" && path.starts_with("bad")),
        "an invalid-byte name reports its kind: {reasons:?}"
    );
}

#[cfg(unix)]
#[test]
fn lossy_equal_skipped_names_stay_distinct() {
    use std::os::unix::ffi::OsStringExt;
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", "[project]\nfail_on = \"never\"\n");
    for byte in [0xffu8, 0xfeu8] {
        let name = std::ffi::OsString::from_vec(vec![b'b', b'a', b'd', byte, b'.', b'r', b's']);
        if let Err(error) = std::fs::write(root.join(name), SAMPLE) {
            assert!(
                name_is_unrepresentable(&error),
                "write non-UTF-8 name: {error}"
            );
            skip_unless_fs_caps_required(
                "distinct skips need representable invalid-byte names",
                &error,
            );
            return;
        }
    }
    let mut gated = scoped(&root, vec![]);
    gated.fail_on_skipped = true;
    let execution = sloplint::execute(&gated).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(execution.failed, "skips must fail the opted-in gate");
    assert_eq!(
        execution.report.summary.skipped, 2,
        "two distinct invalid names are two skips: {:?}",
        execution.report.summary
    );
    assert_eq!(
        execution.report.skipped_gate.len(),
        2,
        "the gate names both files: {:?}",
        execution.report.skipped_gate
    );
}

#[cfg(unix)]
#[test]
fn ratchet_gate_sees_a_second_invalid_name() {
    use std::os::unix::ffi::OsStringExt;
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", "[project]\nfail_on = \"never\"\n");
    let first = std::ffi::OsString::from_vec(vec![b'b', b'a', b'd', 0xff, b'.', b'r', b's']);
    if let Err(error) = std::fs::write(root.join(first), SAMPLE) {
        assert!(
            name_is_unrepresentable(&error),
            "write non-UTF-8 name: {error}"
        );
        skip_unless_fs_caps_required(
            "the ratchet check needs a representable invalid-byte name",
            &error,
        );
        return;
    }
    let mut writing = scoped(&root, vec![]);
    writing.mode = Some(Mode::Ratchet);
    writing.fail_on_skipped = true;
    writing.write_baseline = true;
    sloplint::execute(&writing).unwrap_or_else(|e| panic!("baseline write failed: {e}"));
    let second = std::ffi::OsString::from_vec(vec![b'b', b'a', b'd', 0xfe, b'.', b'r', b's']);
    if let Err(error) = std::fs::write(root.join(second), SAMPLE) {
        assert!(
            name_is_unrepresentable(&error),
            "write non-UTF-8 name: {error}"
        );
        skip_unless_fs_caps_required(
            "the ratchet check needs a representable invalid-byte name",
            &error,
        );
        return;
    }
    let mut gated = scoped(&root, vec![]);
    gated.mode = Some(Mode::Ratchet);
    gated.fail_on_skipped = true;
    let execution = sloplint::execute(&gated).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        execution.failed,
        "a second invalid name is a newly skipped file"
    );
    assert_eq!(
        execution
            .report
            .skipped_gate
            .iter()
            .map(|entry| entry.path.to_string())
            .collect::<Vec<_>>(),
        vec!["bad\\xfe.rs".to_string()],
        "the gate names the newly skipped file"
    );
}

#[cfg(unix)]
#[test]
fn an_excluded_non_utf8_name_is_not_a_skip() {
    use std::os::unix::ffi::OsStrExt;
    let (_tmp, root) = project();
    put(
        &root,
        "sloplint.toml",
        "[project]\nexclude = [\"vendor/**\"]\nfail_on = \"never\"\n",
    );
    std::fs::create_dir_all(root.join("vendor")).unwrap_or_else(|e| panic!("mkdir failed: {e}"));
    let bad = root
        .join("vendor")
        .join(std::ffi::OsStr::from_bytes(b"bad\xff.rs"));
    if let Err(error) = std::fs::write(&bad, SAMPLE) {
        assert!(
            name_is_unrepresentable(&error),
            "write non-UTF-8 name: {error}"
        );
        skip_unless_fs_caps_required("excludes need a representable invalid-byte name", &error);
        return;
    }
    let mut gated = scoped(&root, vec![]);
    gated.fail_on_skipped = true;
    let execution = sloplint::execute(&gated).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert_eq!(
        execution.report.summary.skipped, 0,
        "an excluded name is not counted: {:?}",
        execution.report.summary
    );
    assert!(!execution.failed, "an excluded file never trips the gate");
}

#[cfg(unix)]
#[test]
fn a_literal_backslash_name_and_an_invalid_byte_name_stay_distinct() {
    use std::os::unix::ffi::OsStringExt;
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", "[project]\nfail_on = \"never\"\n");
    put(&root, "sample.rs", SAMPLE);
    std::os::unix::fs::symlink("sample.rs", root.join("a\\xff.rs"))
        .unwrap_or_else(|e| panic!("symlink failed: {e}"));
    let raw = std::ffi::OsString::from_vec(vec![b'a', 0xff, b'.', b'r', b's']);
    if let Err(error) = std::fs::write(root.join(raw), SAMPLE) {
        assert!(
            name_is_unrepresentable(&error),
            "write non-UTF-8 name: {error}"
        );
        skip_unless_fs_caps_required(
            "distinct skips need a representable invalid-byte name",
            &error,
        );
        return;
    }
    let mut gated = scoped(&root, vec![]);
    gated.fail_on_skipped = true;
    let execution = sloplint::execute(&gated).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(execution.failed, "skips must fail the opted-in gate");
    assert_eq!(
        execution.report.summary.skipped, 2,
        "backslash text and an invalid byte are two skips: {:?}",
        execution.report.summary
    );
    assert_eq!(
        execution.report.skipped_gate.len(),
        2,
        "the gate names both files: {:?}",
        execution.report.skipped_gate
    );
}

#[cfg(unix)]
#[test]
fn a_scoped_ratchet_run_sees_skips_under_a_backslash_dir() {
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", "[project]\nfail_on = \"never\"\n");
    put(&root, "a\\b/ok.rs", SAMPLE);
    let mut writing = scoped(&root, vec![]);
    writing.mode = Some(Mode::Ratchet);
    writing.fail_on_skipped = true;
    writing.write_baseline = true;
    sloplint::execute(&writing).unwrap_or_else(|e| panic!("baseline write failed: {e}"));
    put(&root, "a\\b/ok.rs", &format!("// @generated\n{SAMPLE}"));
    let mut gated = scoped(&root, vec![root.join("a\\b")]);
    gated.mode = Some(Mode::Ratchet);
    gated.fail_on_skipped = true;
    let execution = sloplint::execute(&gated).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        execution.failed,
        "a newly skipped file under an explicit scope must fail the gate"
    );
    assert_eq!(
        execution
            .report
            .skipped_gate
            .iter()
            .map(|entry| entry.path.to_string())
            .collect::<Vec<_>>(),
        vec!["a\\\\b/ok.rs".to_string()],
        "the gate names the newly skipped file"
    );
}

#[test]
fn a_scoped_baseline_write_records_out_of_scope_skips() {
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", "[project]\nfail_on = \"never\"\n");
    put(&root, "src/ok.rs", SAMPLE);
    put(&root, "gen/a.rs", &format!("// @generated\n{SAMPLE}"));
    let mut writing = scoped(&root, vec![root.join("src")]);
    writing.mode = Some(Mode::Ratchet);
    writing.fail_on_skipped = true;
    writing.write_baseline = true;
    sloplint::execute(&writing).unwrap_or_else(|e| panic!("baseline write failed: {e}"));
    let mut gated = scoped(&root, vec![]);
    gated.mode = Some(Mode::Ratchet);
    gated.fail_on_skipped = true;
    let execution = sloplint::execute(&gated).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        !execution.failed,
        "a skip recorded by a scoped baseline write is not new: {:?}",
        execution.report.skipped_gate
    );
    let mut scoped_check = scoped(&root, vec![root.join("src")]);
    scoped_check.mode = Some(Mode::Ratchet);
    scoped_check.fail_on_skipped = true;
    let scoped_run = sloplint::execute(&scoped_check).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert!(
        !scoped_run.failed,
        "the scope that wrote the baseline still passes: {:?}",
        scoped_run.report.skipped_gate
    );
}
