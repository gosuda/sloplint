//! A file that fails to parse is judged by its syntax error alone.
//!
//! A058: every `sloplint-allow` in an unparseable file also produced a
//! `meta/unused-suppression`, because no rule ran to use it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sloplint::{Report, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};

const PARSE_ERROR: &str = "meta/parse-error";
const INVALID: &str = "meta/invalid-suppression";
const UNUSED: &str = "meta/unused-suppression";
const ALLOW: &str = "// sloplint-allow(complexity/nesting-depth): reviewed\n";

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

fn options(root: &Path) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    }
}

fn rules_by_path(report: &Report) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = report
        .findings
        .iter()
        .map(|f| (f.path.to_string(), f.rule.to_string()))
        .collect();
    found.sort();
    found
}

fn expected(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(path, rule)| ((*path).to_string(), (*rule).to_string()))
        .collect()
}

fn cli(root: &Path, cache_home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(root)
        .args(args)
        .env("XDG_CACHE_HOME", cache_home)
        .env("HOME", cache_home)
        .env("LOCALAPPDATA", cache_home)
        .output()
        .unwrap_or_else(|e| panic!("spawn {args:?}: {e}"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn an_unparseable_file_reports_only_its_syntax_error() {
    let (_tmp, root) = project();
    put(&root, "broken.rs", &format!("{ALLOW}fn (\n"));

    let report = run(&options(&root)).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert_eq!(
        rules_by_path(&report),
        expected(&[("broken.rs", PARSE_ERROR)]),
        "{:#?}",
        report.findings
    );
}

#[test]
fn the_same_suppression_in_a_parseable_file_is_still_unused() {
    let (_tmp, root) = project();
    put(&root, "ok.rs", &format!("{ALLOW}pub fn tidy() {{}}\n"));

    let report = run(&options(&root)).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert_eq!(
        rules_by_path(&report),
        expected(&[("ok.rs", UNUSED)]),
        "a parseable file keeps its unused-suppression finding: {:#?}",
        report.findings
    );
}

#[test]
fn a_malformed_suppression_in_an_unparseable_file_is_still_reported() {
    let (_tmp, root) = project();
    put(
        &root,
        "broken.rs",
        "// sloplint-allow(complexity/nesting-depth)\nfn (\n",
    );

    let report = run(&options(&root)).unwrap_or_else(|e| panic!("run failed: {e}"));
    assert_eq!(
        rules_by_path(&report),
        expected(&[("broken.rs", INVALID), ("broken.rs", PARSE_ERROR)]),
        "{:#?}",
        report.findings
    );
}

#[test]
fn a_cached_unparseable_file_stays_free_of_unused_suppression_findings() {
    let (_tmp, root) = project();
    put(&root, "broken.rs", &format!("{ALLOW}fn (\n"));
    let cache_home = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let args = ["check", "--format", "json", "--fail-on", "never"];

    let cold = cli(&root, cache_home.path(), &args);
    let warm = cli(&root, cache_home.path(), &args);
    for (what, out) in [("cold", &cold), ("warm", &warm)] {
        let stdout = text(&out.stdout);
        assert!(out.status.success(), "{what}: {stdout}");
        assert!(stdout.contains(PARSE_ERROR), "{what}: {stdout}");
        assert!(!stdout.contains(UNUSED), "{what}: {stdout}");
    }
    assert_eq!(
        cold.stdout, warm.stdout,
        "a cache hit must equal the cold run"
    );
}
