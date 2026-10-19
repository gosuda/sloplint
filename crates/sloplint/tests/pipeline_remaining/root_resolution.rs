//! Project root discovery stops at the nearest `.git`, so a planted ancestor config cannot capture a repository.
//!
//! A031: `sloplint.toml` was searched in every ancestor before `.git` was consulted, so
//! `/tmp/sloplint.toml` became the root, exclude list, and baseline location of any
//! checkout that had no config of its own.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";
const GENERIC: &str = "grep/generic-name";
const EXCLUDE_ALL: &str =
    "[project]\nextend_exclude = [\"**/*.rs\"]\nbaseline = \"planted-baseline.json\"\n";
const GENERIC_OFF: &str = "[rules.\"grep/generic-name\"]\nlevel = \"off\"\n";

fn base() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    (tmp, root)
}

fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn cli(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn {args:?}: {e}"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `check` from `cwd` with no PATHS; returns stdout after asserting the run did not fail.
fn check(cwd: &Path) -> String {
    let out = cli(cwd, &["check", "--no-cache", "--fail-on", "never"]);
    assert!(out.status.success(), "stderr: {}", text(&out.stderr));
    text(&out.stdout)
}

#[test]
fn a_planted_config_above_the_git_root_is_ignored() {
    let (_tmp, base) = base();
    put(&base, "sloplint.toml", EXCLUDE_ALL);
    std::fs::create_dir_all(base.join("repo/.git")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    put(&base, "repo/src/lib.rs", SAMPLE);
    let cwd = base.join("repo/src");

    let stdout = check(&cwd);
    assert!(
        stdout.contains(GENERIC),
        "the planted exclude applied: {stdout}"
    );
    assert!(stdout.contains("files=1 "), "{stdout}");

    let out = cli(&cwd, &["baseline", "--no-cache"]);
    assert!(out.status.success(), "stderr: {}", text(&out.stderr));
    assert!(
        base.join("repo/sloplint-baseline.json").is_file(),
        "baseline belongs to the repo"
    );
    assert!(
        !base.join("planted-baseline.json").exists()
            && !base.join("sloplint-baseline.json").exists(),
        "nothing may be written next to the planted config"
    );
}

#[test]
fn a_git_file_marks_the_boundary_like_a_git_directory() {
    let (_tmp, base) = base();
    put(&base, "sloplint.toml", EXCLUDE_ALL);
    put(&base, "repo/.git", "gitdir: /elsewhere\n");
    put(&base, "repo/src/lib.rs", SAMPLE);

    let stdout = check(&base.join("repo/src"));
    assert!(
        stdout.contains(GENERIC),
        "a .git file must stop the ancestor search so the planted config is ignored: {stdout}"
    );
}

#[test]
fn a_config_inside_the_git_root_still_applies() {
    let (_tmp, base) = base();
    std::fs::create_dir_all(base.join("repo/.git")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    put(&base, "repo/sloplint.toml", GENERIC_OFF);
    put(&base, "repo/src/lib.rs", SAMPLE);

    let stdout = check(&base.join("repo/src"));
    assert!(
        !stdout.contains(GENERIC),
        "the config inside the git root must switch the rule off: {stdout}"
    );
}

#[test]
fn a_nearer_config_below_the_git_root_wins() {
    let (_tmp, base) = base();
    std::fs::create_dir_all(base.join("repo/.git")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    put(&base, "repo/sloplint.toml", EXCLUDE_ALL);
    put(&base, "repo/pkg/sloplint.toml", "");
    put(&base, "repo/pkg/src/lib.rs", SAMPLE);

    let stdout = check(&base.join("repo/pkg/src"));
    assert!(
        stdout.contains(GENERIC),
        "the package config is the nearest: {stdout}"
    );
}

#[test]
fn an_ancestor_config_is_used_when_no_git_boundary_intervenes() {
    let (_tmp, base) = base();
    put(&base, "sloplint.toml", GENERIC_OFF);
    put(&base, "a/b/src/lib.rs", SAMPLE);

    let stdout = check(&base.join("a/b/src"));
    assert!(
        !stdout.contains(GENERIC),
        "with no .git boundary the ancestor config must still apply: {stdout}"
    );
}

#[test]
fn an_explicit_config_overrides_discovery() {
    let (_tmp, base) = base();
    put(&base, "sloplint.toml", EXCLUDE_ALL);
    std::fs::create_dir_all(base.join("repo/.git")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    put(&base, "repo/src/lib.rs", SAMPLE);
    put(&base, "elsewhere/sloplint.toml", GENERIC_OFF);
    put(&base, "elsewhere/lib.rs", SAMPLE);

    let config = base.join("elsewhere/sloplint.toml");
    let root = base.join("elsewhere");
    let out = cli(
        &base.join("repo/src"),
        &[
            "check",
            "--no-cache",
            "--fail-on",
            "never",
            "-c",
            &config.to_string_lossy(),
            &root.to_string_lossy(),
        ],
    );
    assert!(out.status.success(), "stderr: {}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(!stdout.contains(GENERIC), "{stdout}");
    assert!(
        stdout.contains("files=1 "),
        "the explicit root holds one source: {stdout}"
    );
}
