//! Explicit and implicit configuration path failures: directories and unreadable files.

#[cfg(unix)]
use super::{DENIED, assert_denied, lock};
use super::{
    assert_cli_failure, assert_io, cause_of, io_failure, mkdir, opts, project, put, str_of, tweak,
};
use sloplint::run;
use std::io::ErrorKind::NotFound;

// AA-I4: a directory as explicit or implicit config is I/O (exit 3), not "missing" (exit 2).
#[test]
fn config_directory_is_io_exit_3() {
    let (_tmp, root) = project();
    let dir = root.join("cfgdir");
    mkdir(&dir);
    let explicit = tweak(&root, |o| o.config = Some(dir.clone()));
    let (path, kind) = io_failure(run(&explicit), "explicit config directory");
    assert_eq!(path, dir);
    assert_ne!(kind, NotFound, "a directory is not missing");
    let cause = cause_of(std::fs::read_to_string(&dir));
    let args = ["check", "--no-cache", "--config", str_of(&dir)];
    assert_cli_failure(&root, &args, 3, &[str_of(&dir), &cause]);
    let implicit = root.join("sloplint.toml");
    std::fs::remove_file(&implicit).unwrap_or_else(|e| panic!("remove: {e}"));
    mkdir(&implicit);
    let (path, kind) = io_failure(run(&opts(&root)), "implicit config directory");
    assert_eq!(path, implicit);
    assert_ne!(kind, NotFound, "no fallback to defaults");
}

// AA-I4: an unreadable config file is I/O (exit 3) with its path and cause.
#[cfg(unix)]
#[test]
fn unreadable_config_file_is_io_exit_3() {
    let (_tmp, root) = project();
    put(&root, "locked.toml", "");
    let (explicit, implicit) = (root.join("locked.toml"), root.join("sloplint.toml"));
    let _guards = [lock(&explicit, 0), lock(&implicit, 0)];
    assert_denied(std::fs::File::open(&explicit), "locked.toml");
    let given = tweak(&root, |o| o.config = Some(explicit.clone()));
    assert_io(run(&given), "explicit config", &explicit, DENIED);
    assert_io(run(&opts(&root)), "implicit config", &implicit, DENIED);
}
