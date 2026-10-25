//! Source reads never follow a symlink: a source swapped for a link to an outside file is a counted skip.
//!
//! The end-to-end case runs the shipped `check` command before and after the swap and reads its
//! JSON summary. Discovery already refuses links, so it pins the shared skip accounting.
//! `read_source`, the re-read the renderer uses, is refused directly. Every case needs symlinks.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";

#[derive(Deserialize)]
struct Document {
    findings: Vec<serde::de::IgnoredAny>,
    summary: Summary,
}

#[derive(Deserialize)]
struct Summary {
    files: usize,
    skipped: usize,
}

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    put(&root, "sloplint.toml", "");
    (tmp, root)
}

fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {}: {e}", dir.display()));
    }
    std::fs::write(&path, body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn check_json(root: &Path, home: &Path) -> Document {
    let output = Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(root)
        .args([
            "check",
            "--format",
            "json",
            "--no-cache",
            "--fail-on",
            "never",
            "-j",
            "1",
        ])
        .env("HOME", home)
        .env("XDG_CACHE_HOME", home)
        .env("LOCALAPPDATA", home)
        .output()
        .unwrap_or_else(|e| panic!("spawn sloplint: {e}"));
    assert!(
        output.status.success(),
        "check must succeed, got {:?}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    sonic_rs::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("stdout is not the JSON document: {e}"))
}

#[test]
fn a_source_swapped_for_an_outside_symlink_is_a_counted_skip_through_check() {
    use std::os::unix::fs::symlink;
    let (_tmp, root) = project();
    let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let target = outside.path().join("outside.rs");
    std::fs::write(&target, SAMPLE).unwrap_or_else(|e| panic!("write outside: {e}"));
    put(&root, "sample.rs", SAMPLE);
    let home = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));

    let before = check_json(&root, home.path());
    assert_eq!(
        (before.summary.files, before.summary.skipped),
        (1, 0),
        "the regular source is analyzed before it is swapped"
    );
    assert!(
        !before.findings.is_empty(),
        "the regular source must produce a finding before the swap"
    );

    std::fs::remove_file(root.join("sample.rs")).unwrap_or_else(|e| panic!("remove: {e}"));
    symlink(&target, root.join("sample.rs")).unwrap_or_else(|e| panic!("symlink: {e}"));
    let after = check_json(&root, home.path());
    assert_eq!(
        (after.summary.files, after.summary.skipped),
        (0, 1),
        "the link replaces the source: it is one counted skip and is never analyzed"
    );
    assert!(
        after.findings.is_empty(),
        "no finding may come from the outside file"
    );
}

#[test]
fn read_source_refuses_a_symlink_to_outside_content_and_reads_a_regular_file() {
    use std::os::unix::fs::symlink;
    let (_tmp, root) = project();
    let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let target = outside.path().join("outside.rs");
    std::fs::write(&target, "pub fn outside_secret() {}\n")
        .unwrap_or_else(|e| panic!("write outside: {e}"));
    symlink(&target, root.join("link.rs")).unwrap_or_else(|e| panic!("symlink: {e}"));
    put(&root, "inside.rs", "pub fn inside() {}\n");

    let linked = sloplint::read_source(&root.join("link.rs"), 4096);
    assert!(
        linked.is_err(),
        "read_source must refuse a symlink, not return the outside file's content: {linked:?}"
    );
    let regular = sloplint::read_source(&root.join("inside.rs"), 4096)
        .unwrap_or_else(|e| panic!("a regular source must still be read: {e}"));
    assert_eq!(
        regular, "pub fn inside() {}\n",
        "the regular file must be read in full"
    );
}
