//! Recording a baseline onto a directory refuses with the invalid-input error the
//! `write_replacing` doc comment promises, not a raw open error for the directory.

use std::io;
use std::path::PathBuf;

use sloplint::{RunError, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};

fn project() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    std::fs::write(
        root.join("sloplint.toml"),
        "[project]\nbaseline = \"baseline-dir\"\n",
    )
    .unwrap_or_else(|e| panic!("write config: {e}"));
    std::fs::create_dir(root.join("baseline-dir")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    std::fs::write(root.join("first.rs"), "pub fn handle(flag: bool) {}\n")
        .unwrap_or_else(|e| panic!("write source: {e}"));
    (tmp, root)
}

#[test]
fn recording_onto_a_directory_is_refused_as_not_a_regular_file() {
    let (_tmp, root) = project();
    let options = RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        write_baseline: true,
        ..RunOptions::default()
    };
    let Err(error) = run(&options) else {
        panic!("recording a baseline onto a directory must fail");
    };
    let RunError::Io { path, source } = error else {
        panic!("expected an I/O refusal naming the target, got: {error}");
    };
    assert!(
        path.ends_with("baseline-dir"),
        "the refusal names the directory target: {}",
        path.display()
    );
    assert_eq!(
        source.kind(),
        io::ErrorKind::InvalidInput,
        "a directory target is refused as invalid input, not as a raw open error: {source}"
    );
}
