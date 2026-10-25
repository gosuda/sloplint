//! `.gitignore` and `.git/info/exclude` are read only inside the repository that encloses the project
//! root, so a nested repository under an ignoring parent must still analyze its own files. `.ignore`
//! files keep ripgrep semantics and apply from every ancestor. Every run gets an isolated `HOME`, so
//! host-wide excludes cannot change the result.

use std::path::{Path, PathBuf};
use std::process::Command;

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";
const GENERIC: &str = "grep/generic-name";

fn base() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("creating tempdir failed: {e}"));
    let root =
        std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize failed: {e}"));
    mkdir(&root, "home");
    (tmp, root)
}

fn mkdir(root: &Path, rel: &str) {
    std::fs::create_dir_all(root.join(rel))
        .unwrap_or_else(|e| panic!("creating directory {rel} failed: {e}"));
}

fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .unwrap_or_else(|e| panic!("creating directory {} failed: {e}", dir.display()));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("writing fixture {rel} failed: {e}"));
}

/// `check` from `cwd` with `HOME` and `XDG_CONFIG_HOME` inside `base`; returns stdout after asserting success.
fn check(base: &Path, cwd: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(cwd)
        .env("HOME", base.join("home"))
        .env("XDG_CONFIG_HOME", base.join("config"))
        .args(["check", "--no-cache", "--fail-on", "never"])
        .output()
        .unwrap_or_else(|e| panic!("spawning sloplint check failed: {e}"));
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_nested_repo_ignores_its_parents_gitignore() {
    let (_tmp, base) = base();
    put(&base, "parent/.gitignore", "lib/\n");
    mkdir(&base, "parent/svc/.git");
    put(&base, "parent/svc/lib/a.rs", SAMPLE);

    let stdout = check(&base, &base.join("parent/svc"));
    assert!(
        stdout.contains(GENERIC),
        "the nested repo's own file must be analyzed: {stdout}"
    );
    assert!(stdout.contains("files=1 "), "{stdout}");
}

#[test]
fn a_subdirectory_root_still_honors_its_repo_root_gitignore() {
    let (_tmp, base) = base();
    mkdir(&base, "repo/.git");
    put(&base, "repo/.gitignore", "target/\nsub/ignored.rs\n");
    put(&base, "repo/sub/sloplint.toml", "");
    put(&base, "repo/sub/target/dummy.rs", SAMPLE);
    put(&base, "repo/sub/ignored.rs", SAMPLE);
    put(&base, "repo/sub/src/lib.rs", SAMPLE);

    let stdout = check(&base, &base.join("repo/sub"));
    assert!(stdout.contains("files=1 "), "{stdout}");
}

#[test]
fn a_subdirectory_root_still_honors_its_repo_info_exclude() {
    let (_tmp, base) = base();
    mkdir(&base, "repo/.git/info");
    put(&base, "repo/.git/info/exclude", "ignored.rs\n");
    put(&base, "repo/sub/sloplint.toml", "");
    put(&base, "repo/sub/ignored.rs", SAMPLE);
    put(&base, "repo/sub/src/lib.rs", SAMPLE);

    let stdout = check(&base, &base.join("repo/sub"));
    assert!(stdout.contains("files=1 "), "{stdout}");
}

#[test]
fn a_worktree_root_still_honors_the_common_info_exclude() {
    let (_tmp, base) = base();
    put(&base, "main/.git/info/exclude", "ignored.rs\n");
    put(&base, "main/.git/worktrees/wt/commondir", "../..\n");
    put(
        &base,
        "main/wt/.git",
        &format!(
            "gitdir: {}\n",
            base.join("main/.git/worktrees/wt").display()
        ),
    );
    put(&base, "main/wt/sub/sloplint.toml", "");
    put(&base, "main/wt/sub/ignored.rs", SAMPLE);
    put(&base, "main/wt/sub/src/lib.rs", SAMPLE);

    let stdout = check(&base, &base.join("main/wt/sub"));
    assert!(stdout.contains("files=1 "), "{stdout}");
}

#[test]
fn a_repository_negation_beats_global_excludes_in_a_subdirectory_root() {
    let (_tmp, base) = base();
    let excludes = base.join("home/excludes");
    put(
        &base,
        "home/.gitconfig",
        &format!("[core]\n\texcludesFile = {}\n", excludes.display()),
    );
    put(&base, "home/excludes", "x.rs\n");
    mkdir(&base, "repo/.git");
    put(&base, "repo/.gitignore", "!sub/x.rs\n");
    put(&base, "repo/sub/sloplint.toml", "");
    put(&base, "repo/sub/x.rs", SAMPLE);
    put(&base, "repo/sub/keep.rs", SAMPLE);

    let stdout = check(&base, &base.join("repo/sub"));
    assert!(stdout.contains("files=2 "), "{stdout}");
}

#[test]
fn global_excludes_still_apply_in_a_subdirectory_root() {
    let (_tmp, base) = base();
    let excludes = base.join("home/excludes");
    put(
        &base,
        "home/.gitconfig",
        &format!("[core]\n\texcludesFile = {}\n", excludes.display()),
    );
    put(&base, "home/excludes", "glob.rs\n");
    mkdir(&base, "repo/.git");
    put(&base, "repo/sub/sloplint.toml", "");
    put(&base, "repo/sub/glob.rs", SAMPLE);
    put(&base, "repo/sub/keep.rs", SAMPLE);

    let stdout = check(&base, &base.join("repo/sub"));
    assert!(stdout.contains("files=1 "), "{stdout}");
}

#[test]
fn a_git_file_marks_the_gitignore_boundary_like_a_git_directory() {
    let (_tmp, base) = base();
    put(&base, "parent/.gitignore", "lib/\n");
    put(&base, "parent/wt/.git", "gitdir: /elsewhere\n");
    put(&base, "parent/wt/lib/a.rs", SAMPLE);

    let stdout = check(&base, &base.join("parent/wt"));
    assert!(
        stdout.contains(GENERIC),
        "a .git file must stop the ancestor gitignore search: {stdout}"
    );
    assert!(stdout.contains("files=1 "), "{stdout}");
}

#[test]
fn a_nested_repo_inside_the_root_does_not_inherit_the_roots_gitignore() {
    let (_tmp, base) = base();
    mkdir(&base, "repo/.git");
    mkdir(&base, "repo/inner/.git");
    put(&base, "repo/.gitignore", "inner/skip.rs\n");
    put(&base, "repo/sloplint.toml", "");
    put(&base, "repo/top.rs", SAMPLE);
    put(&base, "repo/inner/skip.rs", SAMPLE);
    put(&base, "repo/inner/keep.rs", SAMPLE);

    let stdout = check(&base, &base.join("repo"));
    assert!(stdout.contains("files=3 "), "{stdout}");
}

#[test]
fn ignore_files_apply_from_ancestors_and_negations_override_gitignore() {
    let (_tmp, base) = base();
    mkdir(&base, "repo/.git");
    put(&base, "repo/.gitignore", "x.rs\n");
    put(&base, "repo/.ignore", "sub/skip.rs\n!sub/x.rs\n");
    put(&base, "repo/sub/sloplint.toml", "");
    put(&base, "repo/sub/skip.rs", SAMPLE);
    put(&base, "repo/sub/x.rs", SAMPLE);
    put(&base, "repo/sub/keep.rs", SAMPLE);

    let stdout = check(&base, &base.join("repo/sub"));
    assert!(stdout.contains("files=2 "), "{stdout}");
}

#[test]
fn a_hidden_directory_above_the_repo_does_not_hide_the_root() {
    let (_tmp, base) = base();
    mkdir(&base, "repo/.git");
    put(&base, "repo/.hidden-chain/proj/sloplint.toml", "");
    put(&base, "repo/.hidden-chain/proj/lib.rs", SAMPLE);

    let stdout = check(&base, &base.join("repo/.hidden-chain/proj"));
    assert!(stdout.contains("files=1 "), "{stdout}");
}

#[test]
fn hidden_entries_below_the_root_stay_skipped() {
    let (_tmp, base) = base();
    mkdir(&base, "repo/.git");
    put(&base, "repo/proj/sloplint.toml", "");
    put(&base, "repo/proj/.hidden/x.rs", SAMPLE);
    put(&base, "repo/proj/src/lib.rs", SAMPLE);

    let stdout = check(&base, &base.join("repo/proj"));
    assert!(stdout.contains("files=1 "), "{stdout}");
}
