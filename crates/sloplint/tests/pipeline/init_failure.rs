//! `sloplint init` leaves either a complete configuration or nothing, and never touches an existing name.
//!
//! A012: a write failure after the file was created left an empty `sloplint.toml`, which
//! later runs loaded as defaults and later `init` calls refused to replace.

use std::path::Path;
use std::process::{Command, Output};

const CONFIG: &str = "sloplint.toml";

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

fn entries(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
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

#[cfg(unix)]
#[test]
fn a_failed_write_leaves_no_configuration_and_a_retry_succeeds() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let limited = Command::new("sh")
        .arg("-c")
        .arg("trap '' XFSZ; ulimit -f 0; exec \"$0\" \"$@\"")
        .arg(env!("CARGO_BIN_EXE_sloplint"))
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap_or_else(|e| panic!("spawn sh: {e}"));
    let stderr = text(&limited.stderr);
    assert_eq!(limited.status.code(), Some(3), "stderr: {stderr}");
    assert!(
        stderr.starts_with("sloplint: error: ") && stderr.contains(CONFIG),
        "stderr: {stderr}"
    );
    assert_eq!(
        entries(dir.path()),
        Vec::<String>::new(),
        "init left residue"
    );

    let retry = cli(dir.path(), &["init"]);
    assert!(retry.status.success(), "retry: {}", text(&retry.stderr));
    let written = std::fs::read_to_string(dir.path().join(CONFIG))
        .unwrap_or_else(|e| panic!("read config: {e}"));
    assert!(
        written.starts_with("# sloplint configuration."),
        "retry wrote: {written}"
    );
    assert_eq!(
        entries(dir.path()),
        [CONFIG.to_string()],
        "temporary file remains"
    );
}

#[test]
fn an_existing_configuration_is_refused_untouched() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    std::fs::write(dir.path().join(CONFIG), "# hand written\n")
        .unwrap_or_else(|e| panic!("write: {e}"));

    let out = cli(dir.path(), &["init"]);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains("already exists"), "stderr: {stderr}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join(CONFIG)).unwrap_or_default(),
        "# hand written\n",
        "init must not touch an existing configuration"
    );
    assert_eq!(
        entries(dir.path()),
        [CONFIG.to_string()],
        "a refused init must leave no temporary file behind"
    );
}

#[cfg(unix)]
#[test]
fn a_dangling_symlink_is_never_followed_to_create_its_target() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let target = dir.path().join("elsewhere.toml");
    std::os::unix::fs::symlink(&target, dir.path().join(CONFIG))
        .unwrap_or_else(|e| panic!("symlink: {e}"));

    let out = cli(dir.path(), &["init"]);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains("already exists"), "stderr: {stderr}");
    assert!(!target.exists(), "init wrote through the link");
}
