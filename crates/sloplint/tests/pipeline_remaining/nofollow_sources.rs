//! Source reads never follow a symlink: a source swapped for a link to an outside file is a counted skip.
//!
//! The end-to-end case runs the shipped `check` command before and after the swap and reads its
//! JSON summary; it needs `symlink(2)`, so it is unix-only. Discovery already refuses links, so
//! it pins the shared skip accounting. `read_source`, the re-read the renderer uses, is refused
//! directly: that case plants its link with `symlink(2)` on unix and `symlink_file` on Windows,
//! where an account without the link privilege prints a skip instead of failing. A unix socket
//! at a source path cannot be opened at all, so its refusal is pinned too.

use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
use serde::Deserialize;

#[cfg(unix)]
const SAMPLE: &str = "pub fn handle(
    flag: bool,
) {}\n";

#[cfg(unix)]
#[derive(Deserialize)]
struct Document {
    findings: Vec<serde::de::IgnoredAny>,
    summary: Summary,
}

#[cfg(unix)]
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

/// Plants a file symlink at `link` pointing to `target`; Windows needs a link privilege
/// the runner may lack, and the caller prints that skip, never silently.
#[cfg(unix)]
fn plant_link(target: &Path, link: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::symlink;
    symlink(target, link)
}

#[cfg(windows)]
fn plant_link(target: &Path, link: &Path) -> std::io::Result<()> {
    use std::os::windows::fs::symlink_file;
    symlink_file(target, link)
}

/// Whether `plant_link` failed only because the account cannot create symlinks; the
/// caller prints that skip, never silently.
#[cfg(unix)]
fn link_privilege_missing(_error: &std::io::Error) -> bool {
    false
}

/// 1314 is `ERROR_PRIVILEGE_NOT_HELD`, the privilege creating a file symlink needs.
#[cfg(windows)]
fn link_privilege_missing(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(1314)
}

#[cfg(unix)]
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

#[cfg(unix)]
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
    let (_tmp, root) = project();
    let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let target = outside.path().join("outside.rs");
    std::fs::write(&target, "pub fn outside_secret() {}\n")
        .unwrap_or_else(|e| panic!("write outside: {e}"));
    if let Err(error) = plant_link(&target, &root.join("link.rs")) {
        if link_privilege_missing(&error) {
            eprintln!(
                "SKIP read_source_refuses_a_symlink_to_outside_content_and_reads_a_regular_file: \
                 this account cannot create symlinks: {error}"
            );
            return;
        }
        panic!("symlink: {error}");
    }
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

/// A unix socket at a source path cannot be opened at all (`ENXIO`); the read must count it
/// as the non-regular refusal instead of surfacing the raw open error.
#[cfg(unix)]
#[test]
fn read_source_refuses_a_socket_instead_of_failing_with_its_open_error() {
    use std::os::unix::net::UnixListener;
    let (_tmp, root) = project();
    let listener =
        UnixListener::bind(root.join("sock.rs")).unwrap_or_else(|e| panic!("bind socket: {e}"));

    let error = sloplint::read_source(&root.join("sock.rs"), 4096)
        .expect_err("read_source must refuse a socket, not read it or fail as a raw open");
    let message = error.to_string();
    assert!(
        message.contains("not regular"),
        "a socket must be the counted non-regular refusal: {message}"
    );
    assert!(
        !message.contains("os error 6"),
        "the socket's ENXIO open error must not surface: {message}"
    );
    drop(listener);
}
