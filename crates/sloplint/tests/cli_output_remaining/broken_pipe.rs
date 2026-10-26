//! Broken pipe on stdout indicates downstream consumer chose to stop, not a failure.
//!
//! When stdout closes early (e.g. `sloplint check | head`), sloplint must exit with the gate
//! verdict rather than failing with exit 3 and emitting an error to stderr.

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const GENERIC_DENY_CONFIG: &str = "[rules.\"grep/generic-name\"]\nlevel = \"deny\"\n";
const GENERIC_HANDLE_FN: &str = "pub fn handle() {}\n";
const CLEAN_FN: &str = "pub fn specialized_worker() {}\n";
const MANY_FINDINGS: usize = 800;

fn deny_project(files: usize) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    fs::write(tmp.path().join("sloplint.toml"), GENERIC_DENY_CONFIG).expect("write config");
    for index in 0..files {
        fs::write(
            tmp.path().join(format!("f{index:04}.rs")),
            GENERIC_HANDLE_FN,
        )
        .expect("write source");
    }
    tmp
}

fn clean_project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    fs::write(tmp.path().join("sloplint.toml"), "").expect("write config");
    fs::write(tmp.path().join("clean.rs"), CLEAN_FN).expect("write source");
    tmp
}

fn spawn_closed_stdout(cwd: &Path, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(cwd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sloplint");
    drop(child.stdout.take());
    child.wait_with_output().expect("wait sloplint")
}

#[test]
fn check_with_deny_findings_and_closed_stdout_exits_one_without_stderr() {
    let project = deny_project(MANY_FINDINGS);
    let output = spawn_closed_stdout(project.path(), &["check", "--no-cache"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a closed stdout must keep the deny verdict"
    );
    assert!(
        output.stderr.is_empty(),
        "a closed stdout is the consumer's choice and prints nothing"
    );
}

#[test]
fn check_clean_project_with_closed_stdout_exits_zero_without_stderr() {
    let project = clean_project();
    let output = spawn_closed_stdout(project.path(), &["check", "--no-cache"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "a closed stdout must keep the clean verdict"
    );
    assert!(
        output.stderr.is_empty(),
        "a closed stdout is the consumer's choice and prints nothing"
    );
}

#[test]
fn rules_with_closed_stdout_exits_zero_without_stderr() {
    let project = clean_project();
    let output = spawn_closed_stdout(project.path(), &["rules"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "rules with a closed stdout still succeeds"
    );
    assert!(
        output.stderr.is_empty(),
        "a closed stdout is the consumer's choice and prints nothing"
    );
}

#[test]
fn baseline_with_closed_stdout_exits_zero_without_stderr() {
    let project = clean_project();
    let output = spawn_closed_stdout(project.path(), &["baseline", "--no-cache"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "baseline writes the file before it prints the status line"
    );
    assert!(
        output.stderr.is_empty(),
        "a closed stdout is the consumer's choice and prints nothing"
    );
    assert!(
        project.path().join("sloplint-baseline.json").exists(),
        "the baseline file must exist even when the status line is lost"
    );
}

#[cfg(unix)]
#[test]
fn stdout_to_dev_full_exits_three_with_stderr_message() {
    let full = Path::new("/dev/full");
    if !full.exists() {
        return;
    }
    let Ok(file) = fs::OpenOptions::new().write(true).open(full) else {
        return;
    };
    let project = clean_project();
    let output = Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(project.path())
        .args(["check", "--no-cache"])
        .stdout(Stdio::from(file))
        .stderr(Stdio::piped())
        .output()
        .expect("spawn sloplint");
    assert_eq!(
        output.status.code(),
        Some(3),
        "a full disk is an IO error, not a closed consumer"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("sloplint: error: "),
        "a full disk must still be reported on stderr"
    );
}
