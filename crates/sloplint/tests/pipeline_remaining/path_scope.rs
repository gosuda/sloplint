//! PATH arguments: every one is validated, and a scope matches whole path components.
//!
//! A006: `. does-not-exist` used to exit 0 while `does-not-exist .` exited 2.
//! A115: the scope index must keep the exact component semantics of the old linear scan.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sloplint::{Report, RunError, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";

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

fn scoped(root: &Path, paths: Vec<PathBuf>) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        paths,
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    }
}

fn run_ok(options: &RunOptions) -> Report {
    run(options).unwrap_or_else(|e| panic!("run failed: {e}"))
}

fn failure(options: &RunOptions) -> RunError {
    run(options).expect_err("the run must fail")
}

fn reported(report: &Report) -> BTreeSet<String> {
    report.findings.iter().map(|f| f.path.to_string()).collect()
}

fn set<const N: usize>(paths: [&str; N]) -> BTreeSet<String> {
    paths.iter().map(|p| (*p).to_string()).collect()
}

#[test]
fn every_path_is_validated_whatever_its_position() {
    let (_tmp, root) = project();
    put(&root, "src/lib.rs", SAMPLE);
    let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let missing = root.join("does-not-exist");

    for paths in [
        vec![root.clone(), missing.clone()],
        vec![missing.clone(), root.clone()],
        vec![root.join("src"), root.clone(), missing.clone()],
    ] {
        let error = failure(&scoped(&root, paths.clone()));
        assert!(
            matches!(&error, RunError::PathNotFound { path } if *path == missing),
            "{paths:?}: {error:?}"
        );
        assert_eq!(error.exit_code(), 2, "{paths:?}");
    }

    for paths in [
        vec![root.clone(), outside.path().to_path_buf()],
        vec![outside.path().to_path_buf(), root.clone()],
    ] {
        let error = failure(&scoped(&root, paths.clone()));
        assert!(
            matches!(&error, RunError::PathOutsideRoot { .. }),
            "{paths:?}: {error:?}"
        );
        assert_eq!(error.exit_code(), 2, "{paths:?}");
    }
}

#[test]
fn a_scope_matches_whole_path_components_only() {
    let (_tmp, root) = project();
    put(&root, "src/a.rs", SAMPLE);
    put(&root, "src/nested/c.rs", SAMPLE);
    put(&root, "src2/b.rs", SAMPLE);

    let directory = run_ok(&scoped(&root, vec![root.join("src")]));
    assert_eq!(
        reported(&directory),
        set(["src/a.rs", "src/nested/c.rs"]),
        "scope `src` must cover its subtree but not the sibling `src2`"
    );
    assert_eq!(directory.summary.files, 2, "files counts only the scope");

    let file = run_ok(&scoped(&root, vec![root.join("src/a.rs")]));
    assert_eq!(
        reported(&file),
        set(["src/a.rs"]),
        "a file path scopes to exactly that file"
    );

    let relative = run_ok(&scoped(&root, vec![PathBuf::from("src/../src2")]));
    assert_eq!(
        reported(&relative),
        set(["src2/b.rs"]),
        "`src/../src2` must resolve against cwd to the `src2` scope"
    );

    let overlapping = run_ok(&scoped(
        &root,
        vec![
            root.join("src"),
            root.join("src/nested"),
            root.join("src/a.rs"),
            root.join("src"),
        ],
    ));
    assert_eq!(
        reported(&overlapping),
        set(["src/a.rs", "src/nested/c.rs"]),
        "duplicate and nested scopes must not change the covered set"
    );

    let disjoint = run_ok(&scoped(
        &root,
        vec![root.join("src/nested"), root.join("src2")],
    ));
    assert_eq!(
        reported(&disjoint),
        set(["src/nested/c.rs", "src2/b.rs"]),
        "two disjoint scopes must cover exactly their own files"
    );
}

#[test]
fn the_root_among_the_paths_covers_everything() {
    let (_tmp, root) = project();
    put(&root, "src/a.rs", SAMPLE);
    put(&root, "src2/b.rs", SAMPLE);
    let everything = set(["src/a.rs", "src2/b.rs"]);

    for paths in [
        vec![root.join("src"), root.clone()],
        vec![root.clone(), root.join("src")],
        Vec::new(),
    ] {
        let report = run_ok(&scoped(&root, paths.clone()));
        assert_eq!(reported(&report), everything, "{paths:?}");
        assert_eq!(report.summary.files, 2, "{paths:?}");
    }
}

#[test]
fn hundreds_of_explicit_paths_select_exactly_their_files() {
    let (_tmp, root) = project();
    let count = 400;
    for index in 0..count {
        put(&root, &format!("d{index:03}/lib.rs"), SAMPLE);
    }
    let chosen: Vec<usize> = (0..count).filter(|index| index % 2 == 0).collect();
    let paths: Vec<PathBuf> = chosen
        .iter()
        .map(|index| root.join(format!("d{index:03}")))
        .collect();
    let expected: BTreeSet<String> = chosen
        .iter()
        .map(|index| format!("d{index:03}/lib.rs"))
        .collect();

    let report = run_ok(&scoped(&root, paths));
    assert_eq!(
        reported(&report),
        expected,
        "exactly the files of the {} chosen directories must be reported",
        chosen.len()
    );
    assert_eq!(
        usize::try_from(report.summary.files).ok(),
        Some(chosen.len()),
        "summary.files counts only the files inside the scopes"
    );
}
