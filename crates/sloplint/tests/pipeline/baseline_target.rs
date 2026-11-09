//! `sloplint baseline` refuses a target that exists but is not a sloplint baseline and leaves it byte-for-byte.
//!
//! The baseline path is configurable, so naming `sloplint.toml` or a source file used to
//! overwrite it with baseline JSON while the command exited 0. A missing target and a prior
//! baseline are still written.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sloplint::{Report, RunOptions, run};
use sloplint_core::baseline::Baseline;
use sloplint_core::config::{Mode, Strictness};

const BASELINE: &str = "sloplint-baseline.json";
const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";

/// Mirrors the private `project::MAX_BASELINE_BYTES` ceiling on a baseline document.
const MAX_BASELINE_BYTES: u64 = 536_870_912;

fn project(baseline: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(
        &root,
        "sloplint.toml",
        format!("[project]\nbaseline = {baseline:?}\n").as_bytes(),
    );
    put(&root, "first.rs", SAMPLE.as_bytes());
    (tmp, root)
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    std::fs::write(root.join(rel), bytes).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn recording(root: &Path) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        jobs: 2,
        write_baseline: true,
        ..RunOptions::default()
    }
}

fn gating(root: &Path) -> RunOptions {
    RunOptions {
        write_baseline: false,
        mode: Some(Mode::Ratchet),
        ..recording(root)
    }
}

fn run_ok(options: &RunOptions) -> Report {
    run(options).unwrap_or_else(|e| panic!("run failed: {e}"))
}

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn {args:?}: {e}"))
}

fn temporaries(root: &Path) -> Vec<String> {
    std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("read_dir: {e}"))
        .map(|entry| {
            entry
                .unwrap_or_else(|e| panic!("entry: {e}"))
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".tmp"))
        .collect()
}

fn stored_entries(root: &Path) -> usize {
    let bytes = read(&root.join(BASELINE));
    let text = std::str::from_utf8(&bytes).unwrap_or_else(|e| panic!("baseline is not UTF-8: {e}"));
    Baseline::from_json(text)
        .unwrap_or_else(|e| panic!("baseline does not parse: {e}"))
        .entries
        .len()
}

/// Asserts that recording over `target` is refused as not a baseline, through the library and the CLI, with exit 2,
/// and that `target` keeps exactly the bytes in `before`.
fn assert_refused(root: &Path, target: &str, before: &[u8], case: &str) {
    let path = root.join(target);
    let error = run(&recording(root)).expect_err(&format!(
        "{case}: recording over a non-baseline must be refused"
    ));
    assert!(
        format!("{error:?}").starts_with("NotABaseline"),
        "{case}: expected NotABaseline, got {error}"
    );
    assert_eq!(
        error.exit_code(),
        2,
        "{case}: refusing to overwrite is a usage error: {error}"
    );
    assert!(
        error.to_string().contains(&*path.to_string_lossy()),
        "{case}: the refusal must name the path: {error}"
    );
    assert_eq!(read(&path), before, "{case}: the target was modified");

    let out = cli(root, &["baseline", "--no-cache"]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(
        out.status.code(),
        Some(2),
        "{case}: CLI status; stderr: {stderr}"
    );
    assert_eq!(read(&path), before, "{case}: the CLI modified the target");
    assert!(
        temporaries(root).is_empty(),
        "{case}: temporary files remain: {:?}",
        temporaries(root)
    );
}

#[test]
fn a_config_file_named_as_the_baseline_is_refused_and_left_alone() {
    let (_tmp, root) = project("sloplint.toml");
    let before = read(&root.join("sloplint.toml"));
    assert_refused(&root, "sloplint.toml", &before, "config");
    let out = cli(&root, &["check", "--no-cache", "--fail-on", "never"]);
    assert!(
        out.status.success(),
        "the configuration must still load: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn an_existing_source_file_named_as_the_baseline_is_refused_and_left_alone() {
    let (_tmp, root) = project("first.rs");
    let before = read(&root.join("first.rs"));
    assert_refused(&root, "first.rs", &before, "source");
}

#[test]
fn a_prior_baseline_is_replaced_by_a_fresh_one() {
    let (_tmp, root) = project(BASELINE);
    run_ok(&recording(&root));
    put(&root, "second.rs", SAMPLE.as_bytes());
    let report = run_ok(&recording(&root));
    assert_eq!(
        report.findings.len(),
        2,
        "one generic-name finding per source file"
    );
    assert_eq!(
        stored_entries(&root),
        2,
        "the replacement must be a valid baseline covering both findings"
    );
    let gate = run_ok(&gating(&root));
    assert_eq!(
        (
            gate.findings.len(),
            gate.summary.baselined,
            gate.summary.stale
        ),
        (0, 2, 0),
        "the replacement must gate cleanly"
    );
}

#[test]
fn a_missing_baseline_is_created() {
    let (_tmp, root) = project(BASELINE);
    assert!(
        !root.join(BASELINE).exists(),
        "the target must start missing"
    );
    let report = run_ok(&recording(&root));
    assert!(
        !report.findings.is_empty(),
        "the sample must produce a finding"
    );
    assert_eq!(
        stored_entries(&root),
        report.findings.len(),
        "the created baseline must cover every finding"
    );
}

#[test]
fn an_existing_file_that_is_not_a_baseline_is_refused_and_left_alone() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("corrupt JSON", b"{not json".to_vec()),
        ("plain text", b"hello\n".to_vec()),
        (
            "unsupported version",
            br#"{"version":2,"findings":[]}"#.to_vec(),
        ),
        ("not UTF-8", vec![0xff, 0xfe, 0x00, 0x80]),
    ];
    for (case, body) in cases {
        let (_tmp, root) = project(BASELINE);
        put(&root, BASELINE, &body);
        assert_refused(&root, BASELINE, &body, case);
    }
}

#[test]
fn a_bom_prefixed_prior_baseline_is_replaced_by_a_fresh_one() {
    let (_tmp, root) = project(BASELINE);
    run_ok(&recording(&root));
    let mut bommed = b"\xef\xbb\xbf".to_vec();
    bommed.extend_from_slice(&read(&root.join(BASELINE)));
    put(&root, BASELINE, &bommed);
    put(&root, "second.rs", SAMPLE.as_bytes());
    let report = run_ok(&recording(&root));
    assert_eq!(
        report.findings.len(),
        2,
        "one generic-name finding per source file"
    );
    let stored = read(&root.join(BASELINE));
    assert!(
        !stored.starts_with(b"\xef\xbb\xbf"),
        "the replacement must drop the byte-order mark"
    );
    assert_eq!(
        stored_entries(&root),
        2,
        "the replacement must be a valid baseline covering both findings"
    );
    let gate = run_ok(&gating(&root));
    assert_eq!(
        (
            gate.findings.len(),
            gate.summary.baselined,
            gate.summary.stale
        ),
        (0, 2, 0),
        "the replacement must gate cleanly"
    );
}

#[test]
fn a_sparse_prior_target_past_the_byte_cap_is_refused_and_left_untouched() {
    let (_tmp, root) = project(BASELINE);
    let path = root.join(BASELINE);
    let file = std::fs::File::create(&path).unwrap_or_else(|e| panic!("create: {e}"));
    file.set_len(MAX_BASELINE_BYTES + 1)
        .unwrap_or_else(|e| panic!("set_len: {e}"));
    drop(file);
    let unchanged = || {
        assert_eq!(
            std::fs::metadata(&path)
                .unwrap_or_else(|e| panic!("meta: {e}"))
                .len(),
            MAX_BASELINE_BYTES + 1,
            "the sparse target must keep its size"
        );
    };

    let error =
        run(&recording(&root)).expect_err("a prior target past the byte cap must be refused");
    assert!(
        format!("{error:?}").starts_with("NotABaseline"),
        "expected NotABaseline, got {error}"
    );
    assert_eq!(
        error.exit_code(),
        2,
        "refusing to overwrite is a usage error: {error}"
    );
    assert!(
        error.to_string().contains(&*path.to_string_lossy()),
        "the refusal must name the path: {error}"
    );
    unchanged();
    assert!(
        temporaries(&root).is_empty(),
        "temporary files remain: {:?}",
        temporaries(&root)
    );

    let out = cli(&root, &["baseline", "--no-cache"]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(2), "CLI status; stderr: {stderr}");
    unchanged();
    assert!(
        temporaries(&root).is_empty(),
        "the CLI left temporary files: {:?}",
        temporaries(&root)
    );
}
