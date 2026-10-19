//! With no PATHS, `check` reports the whole project unless the working directory is inside it.
//!
//! A working directory inside the project narrows the report to that subtree. A `-c` that points
//! at a project elsewhere is not a scope error, and explicit PATHS keep their errors.

use std::path::Path;
use std::process::{Command, Output};

const HANDLE_ROW: &str = "warn: `handle` is a generic name shared by unrelated code";

fn project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    std::fs::create_dir(tmp.path().join("sub")).unwrap_or_else(|e| panic!("mkdir sub: {e}"));
    for rel in ["sloplint.toml", "a.rs", "sub/b.rs"] {
        let body = if rel == "sloplint.toml" {
            ""
        } else {
            "pub fn handle() {}\n"
        };
        std::fs::write(tmp.path().join(rel), body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
    }
    tmp
}

fn sloplint(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(cwd)
        .arg("check")
        .arg("--no-cache")
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn sloplint: {e}"))
}

fn config_of(project: &tempfile::TempDir) -> String {
    project
        .path()
        .join("sloplint.toml")
        .to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 temp path"))
        .to_string()
}

fn finding_paths(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.contains(HANDLE_ROW))
        .filter_map(|line| line.split(':').next())
        .map(str::to_string)
        .collect()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// A004: a config outside the working directory checks the whole project.
#[test]
fn explicit_config_outside_the_working_directory_checks_the_whole_project() {
    let tmp = project();
    let elsewhere = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let out = sloplint(elsewhere.path(), &["-c", config_of(&tmp).as_str()]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(
        finding_paths(&out),
        ["a.rs", "sub/b.rs"],
        "a config outside the working directory must report the whole project"
    );
}

// A004 control: a working directory inside the project still scopes the report to it.
#[test]
fn working_directory_inside_the_project_scopes_the_report() {
    let tmp = project();
    let out = sloplint(&tmp.path().join("sub"), &[]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(
        finding_paths(&out),
        ["sub/b.rs"],
        "a working directory inside the project must scope the report to that subtree"
    );
}

// A004 control: an explicit config that holds the working directory does not widen the scope.
#[test]
fn explicit_config_above_the_working_directory_keeps_the_subdirectory_scope() {
    let tmp = project();
    let out = sloplint(&tmp.path().join("sub"), &["-c", "../sloplint.toml"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(
        finding_paths(&out),
        ["sub/b.rs"],
        "-c ../sloplint.toml from sub/ must keep the subdirectory scope"
    );
}

// A004 control: an explicit PATH inside the project narrows the report even from elsewhere.
#[test]
fn explicit_path_inside_the_project_scopes_the_report_from_elsewhere() {
    let tmp = project();
    let elsewhere = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let sub = tmp.path().join("sub");
    let sub = sub
        .to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 temp path"));
    let out = sloplint(elsewhere.path(), &["-c", config_of(&tmp).as_str(), sub]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(
        finding_paths(&out),
        ["sub/b.rs"],
        "an explicit PATH under sub/ must narrow the report even from another directory"
    );
}

// A004 control: explicit PATHS outside the project, or missing, still fail with exit 2.
#[test]
fn explicit_paths_outside_the_project_or_missing_still_fail() {
    let tmp = project();
    let elsewhere = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let outside = sloplint(elsewhere.path(), &["-c", config_of(&tmp).as_str(), "."]);
    assert_eq!(outside.status.code(), Some(2), "outside path exit code");
    assert!(
        stderr(&outside).contains("path is outside the project root: ."),
        "outside path message: {}",
        stderr(&outside)
    );
    assert!(outside.stdout.is_empty(), "no report on a scope error");
    let missing = sloplint(elsewhere.path(), &["-c", config_of(&tmp).as_str(), "nope"]);
    assert_eq!(missing.status.code(), Some(2), "missing path exit code");
    assert!(
        stderr(&missing).contains("path not found: nope"),
        "missing path message: {}",
        stderr(&missing)
    );
}
