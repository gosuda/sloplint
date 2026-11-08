//! `implicit_scope`: the PATH scope of a command line that names no PATHS.
//!
//! It must resolve the root exactly as `execute` does and never load configuration or walk
//! sources, so a `--config` elsewhere cannot turn the default scope into an outside-root error.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sloplint::{RunError, RunOptions, implicit_scope, run};
use sloplint_core::config::{Mode, Strictness};

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";

fn temp() -> (tempfile::TempDir, PathBuf) {
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

fn scope(cwd: &Path, root: Option<&Path>, config: Option<&Path>) -> Vec<PathBuf> {
    implicit_scope(cwd, root, config).unwrap_or_else(|e| panic!("implicit_scope: {e}"))
}

#[test]
fn a_directory_inside_the_root_scopes_to_itself() {
    let (_tmp, root) = temp();
    put(&root, "sloplint.toml", "");
    put(&root, "pkg/src/lib.rs", SAMPLE);
    let cwd = root.join("pkg/src");

    assert_eq!(
        scope(&cwd, Some(&root), None),
        vec![cwd.clone()],
        "explicit root"
    );
    assert_eq!(
        scope(&cwd, None, None),
        vec![cwd.clone()],
        "discovered root"
    );
}

#[test]
fn the_root_itself_and_directories_outside_it_scope_to_the_whole_root() {
    let (_tmp, root) = temp();
    put(&root, "sloplint.toml", "");
    let (_other, outside) = temp();

    assert!(
        scope(&root, Some(&root), None).is_empty(),
        "cwd is the root"
    );
    assert!(
        scope(&outside, Some(&root), None).is_empty(),
        "cwd is outside the root"
    );
}

#[test]
fn a_config_elsewhere_decides_the_root() {
    let (_tmp, project) = temp();
    put(&project, "sloplint.toml", "");
    put(&project, "inner/lib.rs", SAMPLE);
    let config = project.join("sloplint.toml");
    let (_other, unrelated) = temp();

    assert!(
        scope(&unrelated, None, Some(&config)).is_empty(),
        "a cwd outside the config's directory must not become an out-of-root PATH"
    );
    let inner = project.join("inner");
    assert_eq!(
        scope(&inner, None, Some(&config)),
        vec![inner.clone()],
        "a cwd inside the config's directory scopes to itself"
    );
    assert!(
        scope(&project, None, Some(&config)).is_empty(),
        "a cwd equal to the config's directory is the root, so the scope is the whole root"
    );
}

#[test]
fn it_loads_no_configuration_and_discovers_no_sources() {
    let (_tmp, root) = temp();
    put(&root, "sloplint.toml", "this is [not toml\n");
    put(&root, "pkg/broken.rs", "fn (\n");
    let cwd = root.join("pkg");

    assert_eq!(
        scope(&cwd, Some(&root), None),
        vec![cwd.clone()],
        "an unparseable config and a broken source must not stop the scope computation"
    );
}

#[test]
fn an_unresolvable_root_is_an_io_error() {
    let (_tmp, root) = temp();
    let missing = root.join("no-such-root");

    let error = implicit_scope(&root, Some(&missing), None).expect_err("a missing root must fail");
    assert!(
        matches!(&error, RunError::Io { path, .. } if *path == missing),
        "{error:?}"
    );
    assert_eq!(
        error.exit_code(),
        3,
        "an unresolvable root is an I/O error: {error:?}"
    );
}

#[test]
fn the_scope_feeds_run_and_limits_its_report() {
    let (_tmp, root) = temp();
    put(&root, "sloplint.toml", "");
    put(&root, "pkg/lib.rs", SAMPLE);
    put(&root, "other/lib.rs", SAMPLE);
    let cwd = root.join("pkg");

    let options = RunOptions {
        cwd: cwd.clone(),
        root: Some(root.clone()),
        paths: scope(&cwd, Some(&root), None),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    };
    let report = run(&options).unwrap_or_else(|e| panic!("run failed: {e}"));
    let reported: BTreeSet<String> = report.findings.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(
        reported,
        BTreeSet::from(["pkg/lib.rs".to_string()]),
        "the implicit scope must limit the report to the working directory"
    );
}
