//! Configuration, baseline, and custom-rule inputs must be regular files; a FIFO or device is refused, never read.
//!
//! A029: a FIFO planted at `sloplint.toml`, the baseline, or a custom-rule file blocked the run
//! forever, and `-c /dev/zero` read without end until memory ran out. Every case runs the real
//! binary under a deadline, so a regression fails the test instead of hanging the suite.

#![cfg(unix)]

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";
const NOT_REGULAR: &str = "not a regular file";
const DEADLINE: Duration = Duration::from_secs(30);

struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&root, "sample.rs", SAMPLE);
    (tmp, root)
}

fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn mkfifo(path: &Path) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
    }
    let status = Command::new("mkfifo")
        .arg(path)
        .status()
        .unwrap_or_else(|e| panic!("spawn mkfifo: {e}"));
    assert!(
        status.success(),
        "mkfifo {} failed: {status}",
        path.display()
    );
}

/// Runs the binary with a deadline; `code` is `None` when the deadline killed a hung child.
fn cli(cwd: &Path, args: &[&str]) -> Outcome {
    cli_through(cwd, Command::new(env!("CARGO_BIN_EXE_sloplint")), args)
}

fn cli_through(cwd: &Path, mut command: Command, args: &[&str]) -> Outcome {
    let logs = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let out_path = logs.path().join("stdout");
    let err_path = logs.path().join("stderr");
    let stdout = File::create(&out_path).unwrap_or_else(|e| panic!("stdout file: {e}"));
    let stderr = File::create(&err_path).unwrap_or_else(|e| panic!("stderr file: {e}"));
    let mut child = command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {args:?}: {e}"));
    let started = Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if started.elapsed() < DEADLINE => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(e) => panic!("wait {args:?}: {e}"),
        }
    };
    Outcome {
        code,
        stdout: std::fs::read_to_string(&out_path).unwrap_or_default(),
        stderr: std::fs::read_to_string(&err_path).unwrap_or_default(),
    }
}

fn assert_refused(outcome: &Outcome, path: &Path, what: &str) {
    assert!(
        outcome.code.is_some(),
        "{what}: the run hung on the FIFO until the deadline killed it"
    );
    assert_eq!(outcome.code, Some(3), "{what}: stderr: {}", outcome.stderr);
    assert!(
        outcome.stderr.contains(NOT_REGULAR) && outcome.stderr.contains(&*path.to_string_lossy()),
        "{what}: the error must name the path and the reason: {}",
        outcome.stderr
    );
}

#[test]
fn a_fifo_baseline_is_refused_without_blocking() {
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", "");
    let fifo = root.join("sloplint-baseline.json");
    mkfifo(&fifo);

    let outcome = cli(&root, &["check", "--no-cache", "--mode", "ratchet"]);
    assert_refused(&outcome, &fifo, "baseline");
}

#[test]
fn a_fifo_baseline_target_is_refused_when_recording() {
    let (_tmp, root) = project();
    put(&root, "sloplint.toml", "");
    let fifo = root.join("sloplint-baseline.json");
    mkfifo(&fifo);

    let outcome = cli(&root, &["baseline", "--no-cache"]);
    assert_refused(&outcome, &fifo, "baseline recording");
}

#[test]
fn a_fifo_configuration_is_refused_without_blocking() {
    let (_tmp, root) = project();
    let fifo = root.join("sloplint.toml");
    mkfifo(&fifo);

    let outcome = cli(&root, &["check", "--no-cache"]);
    assert_refused(&outcome, &fifo, "implicit config");
}

#[test]
fn a_fifo_custom_rule_file_named_by_the_configuration_is_refused() {
    let (_tmp, root) = project();
    put(
        &root,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\"rules/*.toml\"]\n",
    );
    let fifo = root.join("rules/planted.toml");
    mkfifo(&fifo);

    let outcome = cli(&root, &["check", "--no-cache"]);
    assert_refused(&outcome, &fifo, "custom rule file");
}

#[test]
fn a_fifo_that_no_input_names_is_simply_ignored() {
    let (_tmp, root) = project();
    put(
        &root,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\"rules/*.toml\"]\n",
    );
    mkfifo(&root.join("rules/unrelated.txt"));

    let outcome = cli(&root, &["check", "--no-cache", "--fail-on", "never"]);
    assert_eq!(outcome.code, Some(0), "stderr: {}", outcome.stderr);
    assert!(
        outcome.stdout.contains("files=1 ") && outcome.stdout.contains("skipped=0"),
        "only sample.rs is a source: {}",
        outcome.stdout
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_character_device_config_is_refused_without_reading_it() {
    let (_tmp, root) = project();
    let mut capped = Command::new("sh");
    capped
        .arg("-c")
        .arg("ulimit -v 4000000; exec \"$0\" \"$@\"")
        .arg(env!("CARGO_BIN_EXE_sloplint"));

    let outcome = cli_through(&root, capped, &["check", "--no-cache", "-c", "/dev/zero"]);
    assert_refused(&outcome, Path::new("/dev/zero"), "explicit config");
}

#[test]
fn a_symlink_to_a_regular_configuration_is_still_followed() {
    let (_tmp, root) = project();
    put(
        &root,
        "real.toml",
        "[rules.\"grep/generic-name\"]\nlevel = \"off\"\n",
    );
    std::os::unix::fs::symlink("real.toml", root.join("linked.toml"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));

    let outcome = cli(&root, &["check", "--no-cache", "-c", "linked.toml"]);
    assert_eq!(outcome.code, Some(0), "stderr: {}", outcome.stderr);
    assert!(
        !outcome.stdout.contains("grep/generic-name"),
        "the linked configuration must apply: {}",
        outcome.stdout
    );
}
