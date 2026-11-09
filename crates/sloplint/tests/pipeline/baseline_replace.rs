//! `sloplint baseline` replaces the file atomically instead of rewriting it in place.
//!
//! A009: the baseline was truncated and rewritten, so a failed or concurrent write
//! destroyed the previous baseline and a reader mid-write saw a corrupt one.

use std::path::{Path, PathBuf};

use sloplint::{Report, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};

#[cfg(unix)]
const BASELINE: &str = "sloplint-baseline.json";
const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&root, "sloplint.toml", "");
    put(&root, "first.rs", SAMPLE);
    (tmp, root)
}

fn put(root: &Path, rel: &str, body: &str) {
    std::fs::write(root.join(rel), body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

#[cfg(unix)]
fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn checking(root: &Path, mode: Mode) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(mode),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    }
}

fn recording(root: &Path) -> RunOptions {
    RunOptions {
        write_baseline: true,
        ..checking(root, Mode::Threshold)
    }
}

fn run_ok(options: &RunOptions) -> Report {
    run(options).unwrap_or_else(|e| panic!("run failed: {e}"))
}

fn names(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("read_dir: {e}"))
        .map(|entry| {
            entry
                .unwrap_or_else(|e| panic!("entry: {e}"))
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    found.sort();
    found
}

#[test]
fn a_written_baseline_is_whole_and_leaves_no_temporary_file() {
    let (_tmp, root) = project();
    run_ok(&recording(&root));
    put(&root, "second.rs", SAMPLE);
    let second = run_ok(&recording(&root));
    assert_eq!(
        second.findings.len(),
        2,
        "the second recording must see one generic-name finding per source file"
    );

    let leftovers: Vec<String> = names(&root)
        .into_iter()
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files remain: {leftovers:?}"
    );

    let gate = run_ok(&checking(&root, Mode::Ratchet));
    assert_eq!(
        (
            gate.findings.len(),
            gate.summary.baselined,
            gate.summary.stale
        ),
        (0, 2, 0),
        "the new baseline must be a valid document covering both findings"
    );
}

#[cfg(unix)]
#[test]
fn a_replacement_never_exposes_a_partial_file_to_existing_links() {
    let (_tmp, root) = project();
    run_ok(&recording(&root));
    let before = read(&root.join(BASELINE));
    std::fs::hard_link(root.join(BASELINE), root.join("reader-link.json"))
        .unwrap_or_else(|e| panic!("hard link: {e}"));

    put(&root, "second.rs", SAMPLE);
    run_ok(&recording(&root));

    assert_ne!(
        read(&root.join(BASELINE)),
        before,
        "the baseline must change"
    );
    assert_eq!(
        read(&root.join("reader-link.json")),
        before,
        "an open reader keeps seeing the complete previous baseline"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_leaf_inside_the_root_keeps_pointing_at_its_updated_target() {
    use std::os::unix::fs::symlink;
    let (_tmp, root) = project();
    let prior = "{\"version\":1,\"findings\":[]}\n";
    put(&root, "real.json", prior);
    symlink("real.json", root.join("link.json")).unwrap_or_else(|e| panic!("link: {e}"));
    put(
        &root,
        "sloplint.toml",
        "[project]\nbaseline = \"link.json\"\n",
    );

    run_ok(&recording(&root));

    let meta =
        std::fs::symlink_metadata(root.join("link.json")).unwrap_or_else(|e| panic!("lstat: {e}"));
    assert!(meta.file_type().is_symlink(), "the link was replaced");
    let gate = run_ok(&checking(&root, Mode::Ratchet));
    assert_eq!(
        (gate.findings.len(), gate.summary.baselined),
        (0, 1),
        "the baseline read through the link must cover the one finding"
    );
    assert_ne!(
        read(&root.join("real.json")),
        prior.as_bytes(),
        "the target was not updated"
    );
}

#[cfg(unix)]
#[test]
fn a_read_only_baseline_is_refused_and_left_alone() {
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, root) = project();
    run_ok(&recording(&root));
    let path = root.join(BASELINE);
    let before = read(&path);
    let set_mode = |mode: u32| {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
            .unwrap_or_else(|e| panic!("chmod {mode:o}: {e}"));
    };
    set_mode(0o444);
    assert!(
        std::fs::OpenOptions::new().write(true).open(&path).is_err(),
        "permission bits were bypassed; run these tests as an unprivileged user, not root"
    );
    put(&root, "second.rs", SAMPLE);

    let outcome = run(&recording(&root));
    set_mode(0o644);
    let error = outcome.expect_err("a read-only baseline must not be replaced");
    assert_eq!(error.exit_code(), 3, "{error}");
    assert_eq!(read(&path), before, "the read-only baseline changed");
    let leftovers: Vec<String> = names(&root)
        .into_iter()
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files remain: {leftovers:?}"
    );
}

/// Runs the real CLI with a zero file-size limit and SIGXFSZ ignored: creating a file still works,
/// but any non-empty write fails with an error. The unit of `ulimit -f` differs between shells,
/// which is why the limit is zero rather than a calibrated size.
#[cfg(unix)]
fn limited_cli(root: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("sh")
        .arg("-c")
        .arg("trap '' XFSZ; ulimit -f 0; exec \"$0\" \"$@\"")
        .arg(env!("CARGO_BIN_EXE_sloplint"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| panic!("spawn sh: {e}"))
}

#[cfg(unix)]
#[test]
fn a_failed_write_keeps_the_previous_baseline_and_cleans_up() {
    let (_tmp, root) = project();
    run_ok(&recording(&root));
    let path = root.join(BASELINE);
    let before = read(&path);
    assert!(
        !before.is_empty(),
        "the first baseline must exist before the failing run"
    );
    put(&root, "second.rs", SAMPLE);

    let out = limited_cli(&root, &["baseline", "--no-cache"]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(3), "stderr: {stderr}");
    assert!(
        stderr.starts_with("sloplint: error: ") && stderr.contains(BASELINE),
        "stderr: {stderr}"
    );
    assert_eq!(read(&path), before, "the previous baseline was damaged");
    let leftovers: Vec<String> = names(&root)
        .into_iter()
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files remain: {leftovers:?}"
    );
}
