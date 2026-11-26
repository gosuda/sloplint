//! Opt-in writes: `check` changes nothing, `baseline` and `init` write exactly
//! one admitted path each and refuse the rest, and ratchet runs are stable.
//!
//! A012 gaps: a baseline shared through hard links lost its link count on
//! rewrite, repeated records were never compared byte for byte, and the
//! publication contract lived only in prose.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sloplint::{Report, RunOptions, run};
use sloplint_core::config::{Mode, Strictness};

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";
const BASELINE: &str = "sloplint-baseline.json";
const CONFIG: &str = "sloplint.toml";

fn tempdir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|e| panic!("canonicalize: {e}"));
    (tmp, root)
}

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir: {e}"));
    }
    std::fs::write(path, body).unwrap_or_else(|e| panic!("write {rel}: {e}"));
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn seed(root: &Path) {
    write(root, "sample.rs", SAMPLE);
    write(root, CONFIG, "");
}

fn set_baseline(root: &Path, name: &str) {
    write(root, CONFIG, &format!("[project]\nbaseline = {name:?}\n"));
}

fn opts(root: &Path) -> RunOptions {
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

fn writing(root: &Path) -> RunOptions {
    RunOptions {
        write_baseline: true,
        ..opts(root)
    }
}

fn run_ok(options: &RunOptions) -> Report {
    run(options).unwrap_or_else(|e| panic!("run failed: {e}"))
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sloplint"))
}

fn cli(root: &Path, args: &[&str]) -> Output {
    bin()
        .current_dir(root)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn {args:?}: {e}"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[derive(Debug, Eq, PartialEq)]
enum EntryContents {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
    Special(std::fs::FileType),
}

/// Every entry under root, without traversing symbolic links.
fn snapshot(root: &Path) -> Vec<(PathBuf, EntryContents)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
        for entry in entries {
            let entry = entry.unwrap_or_else(|e| panic!("entry: {e}"));
            let path = entry.path();
            let kind = entry.file_type().unwrap_or_else(|e| panic!("kind: {e}"));
            let contents = if kind.is_dir() {
                stack.push(path.clone());
                EntryContents::Directory
            } else if kind.is_file() {
                EntryContents::File(read(&path))
            } else if kind.is_symlink() {
                EntryContents::Symlink(
                    std::fs::read_link(&path).unwrap_or_else(|e| panic!("read_link: {e}")),
                )
            } else {
                EntryContents::Special(kind)
            };
            let rel = path
                .strip_prefix(root)
                .unwrap_or_else(|e| panic!("prefix: {e}"));
            out.push((rel.to_path_buf(), contents));
        }
    }
    out.sort_by(|(left, _), (right, _)| left.cmp(right));
    out
}

#[test]
fn snapshot_detects_empty_directories() {
    let (_tmp, root) = tempdir();
    let before = snapshot(&root);
    std::fs::create_dir(root.join("empty")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    assert_ne!(
        snapshot(&root),
        before,
        "an empty directory is a tree change"
    );
    let directory = snapshot(&root);
    std::fs::remove_dir(root.join("empty")).unwrap_or_else(|e| panic!("remove directory: {e}"));
    write(&root, "empty", "<dir>");
    assert_ne!(
        snapshot(&root),
        directory,
        "a file containing a marker is still distinct from a directory"
    );
}

#[test]
#[cfg(unix)]
fn snapshot_records_symbolic_links_without_following_them() {
    let (_tmp, root) = tempdir();
    let (_outside, outside) = tempdir();
    write(&outside, "data", "first");
    let before = snapshot(&root);
    std::os::unix::fs::symlink(&outside, root.join("link"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));
    let linked = snapshot(&root);
    assert_ne!(linked, before, "a symbolic link is a tree change");
    write(&outside, "data", "second");
    assert_eq!(
        snapshot(&root),
        linked,
        "link targets stay outside the snapshot"
    );
    std::fs::remove_file(root.join("link")).unwrap_or_else(|e| panic!("remove link: {e}"));
    std::os::unix::fs::symlink(outside.join("data"), root.join("link"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));
    assert_ne!(
        snapshot(&root),
        linked,
        "retargeting a symbolic link is a tree change"
    );
}

#[derive(serde::Deserialize)]
struct CliDocument {
    summary: CliSummary,
}

#[derive(serde::Deserialize)]
struct CliSummary {
    files: usize,
    findings: usize,
    baselined: usize,
}

fn successful_summary(output: &Output) -> CliSummary {
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    sonic_rs::from_slice::<CliDocument>(&output.stdout)
        .unwrap_or_else(|e| panic!("successful output is not a JSON report: {e}"))
        .summary
}

#[cfg(unix)]
fn nlink(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("metadata {}: {e}", path.display()))
        .nlink()
}

/// The baseline is replaced atomically, never rewritten in place: a linked
/// reader keeps seeing the complete previous baseline (A009), with one fewer
/// link to that old inode. `check` never touches sources or their link counts.
#[test]
fn recorded_baseline_replaces_atomically_preserving_linked_readers() {
    let (_tmp, root) = tempdir();
    seed(&root);
    set_baseline(&root, BASELINE);
    run_ok(&writing(&root));
    let baseline = root.join(BASELINE);
    let first = read(&baseline);
    assert!(!first.is_empty(), "the first record must write content");

    let sibling = root.join("baseline-copy.json");
    std::fs::hard_link(&baseline, &sibling).unwrap_or_else(|e| panic!("hard_link: {e}"));
    #[cfg(unix)]
    assert_eq!(nlink(&baseline), 2, "fixture needs a linked baseline");

    write(&root, "extra.rs", "pub fn manage(flag: bool) {}\n");
    run_ok(&writing(&root));
    let second = read(&baseline);
    assert_ne!(second, first, "new findings must change the record");
    assert_eq!(
        read(&sibling),
        first,
        "a linked reader keeps seeing the complete previous baseline"
    );
    #[cfg(unix)]
    {
        assert_eq!(
            nlink(&sibling),
            1,
            "replacement removes the old baseline link"
        );
        assert_eq!(nlink(&baseline), 1, "the replacement has its own inode");
    }
}

#[test]
fn second_record_is_byte_identical() {
    let (_tmp, root) = tempdir();
    seed(&root);
    set_baseline(&root, BASELINE);
    run_ok(&writing(&root));
    let first = read(&root.join(BASELINE));
    run_ok(&writing(&root));
    let second = read(&root.join(BASELINE));
    assert_eq!(second, first, "recording twice must write identical bytes");
}

#[test]
fn baseline_cli_refuses_non_baseline_file() {
    let (_tmp, root) = tempdir();
    seed(&root);
    set_baseline(&root, BASELINE);
    write(&root, BASELINE, "not a baseline\n");
    let out = cli(&root, &["baseline", "--no-cache"]);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr: {stderr}");
    assert!(
        stderr.contains("is not a sloplint baseline"),
        "stderr: {stderr}"
    );
    assert!(
        text(&out.stdout).is_empty(),
        "refusals print nothing to stdout"
    );
    assert_eq!(
        read(&root.join(BASELINE)),
        b"not a baseline\n",
        "a refused baseline write must leave the file untouched"
    );
    let mut names: Vec<_> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("read_dir: {e}"))
        .map(|e| e.unwrap_or_else(|e| panic!("entry: {e}")).file_name())
        .collect();
    names.sort();
    let mut expected: Vec<_> = [CONFIG, BASELINE, "sample.rs"]
        .iter()
        .map(std::ffi::OsString::from)
        .collect();
    expected.sort();
    assert_eq!(
        names, expected,
        "a refused baseline must leave no temporary file behind"
    );
}

#[test]
fn check_cli_writes_nothing_and_keeps_hard_links() {
    let (_tmp, root) = tempdir();
    seed(&root);
    #[cfg(unix)]
    {
        std::fs::hard_link(root.join("sample.rs"), root.join("sample-copy.rs"))
            .unwrap_or_else(|e| panic!("hard_link: {e}"));
        assert_eq!(nlink(&root.join("sample.rs")), 2, "fixture needs two links");
    }
    let before = snapshot(&root);
    let first = cli(&root, &["check", "--no-cache", "--format", "json"]);
    let second = cli(&root, &["check", "--no-cache", "--format", "json"]);
    let expected_files = if cfg!(unix) { 2 } else { 1 };
    for output in [&first, &second] {
        let summary = successful_summary(output);
        assert_eq!(summary.files, expected_files);
        assert_eq!(summary.findings, expected_files);
        assert_eq!(summary.baselined, 0);
    }
    assert_eq!(
        snapshot(&root),
        before,
        "a check run must leave the project tree byte-identical"
    );
    #[cfg(unix)]
    assert_eq!(
        nlink(&root.join("sample.rs")),
        2,
        "a check run must keep hard link counts"
    );
    assert_eq!(
        first.stdout, second.stdout,
        "repeated checks on a clean tree must print identical output"
    );
}

#[test]
fn ratchet_cli_runs_are_stable() {
    let (_tmp, root) = tempdir();
    seed(&root);
    set_baseline(&root, BASELINE);
    let recorded = cli(&root, &["baseline", "--no-cache"]);
    assert!(
        recorded.status.success(),
        "baseline: {}",
        text(&recorded.stderr)
    );
    let run = |args: &[&str]| cli(&root, args);
    let first = run(&[
        "check",
        "--no-cache",
        "--mode",
        "ratchet",
        "--format",
        "json",
    ]);
    let second = run(&[
        "check",
        "--no-cache",
        "--mode",
        "ratchet",
        "--format",
        "json",
    ]);
    for output in [&first, &second] {
        let summary = successful_summary(output);
        assert_eq!(summary.files, 1);
        assert_eq!(summary.findings, 0);
        assert_eq!(summary.baselined, 1);
    }
    assert_eq!(
        first.stdout, second.stdout,
        "repeated ratchet runs must print identical output"
    );
}

#[test]
fn default_agent_output_is_stable_for_threshold_and_ratchet_runs() {
    let (_tmp, root) = tempdir();
    seed(&root);
    set_baseline(&root, BASELINE);
    let recorded = cli(&root, &["baseline", "--no-cache"]);
    assert!(recorded.status.success(), "{}", text(&recorded.stderr));
    for (mode, expected_baselined) in [("threshold", 0), ("ratchet", 1)] {
        let args = ["check", "--no-cache", "--mode", mode];
        let first = cli(&root, &args);
        let second = cli(&root, &args);
        for output in [&first, &second] {
            assert!(output.status.success(), "{}", text(&output.stderr));
            let stdout = text(&output.stdout);
            let summary = stdout
                .lines()
                .find(|line| line.starts_with("summary: strictness=standard files=1 "))
                .unwrap_or_else(|| {
                    panic!("default output must contain a completed agent summary: {stdout}")
                });
            let expected = format!("baselined={expected_baselined}");
            let baseline_fields: Vec<_> = summary
                .split_whitespace()
                .filter(|field| field.starts_with("baselined="))
                .collect();
            assert_eq!(
                baseline_fields,
                [expected.as_str()],
                "{mode} mode must report exactly one {expected} field, got: {summary}"
            );
        }
        assert_eq!(first.stdout, second.stdout, "default output in {mode} mode");
    }
}

#[test]
fn help_states_the_publication_contract() {
    let (_tmp, root) = tempdir();
    for (args, needles) in [
        (
            &["check", "--help"][..],
            &["writes nothing", "`fix` field", "help annotation"][..],
        ),
        (
            &["baseline", "--help"][..],
            &["refuses", "atomically", "in the project tree", "--no-cache"][..],
        ),
        (&["init", "--help"][..], &["Refuses"][..]),
    ] {
        let out = cli(&root, args);
        assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
        let stdout = text(&out.stdout);
        for needle in needles {
            assert!(
                stdout.contains(needle),
                "{args:?} help lacks {needle:?}:\n{stdout}"
            );
        }
    }
}

#[test]
fn readme_states_what_sloplint_writes() {
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md"))
        .unwrap_or_else(|e| panic!("read README: {e}"));
    for needle in [
        "What sloplint writes",
        "writes nothing to the project tree",
        "never done by `check`",
        "writes exactly one file in the project tree",
        "unless `--no-cache` is passed",
        "with one fewer link",
        "link counts hold",
        "byte-identical",
        "`fix` field",
        "help annotation",
    ] {
        assert!(readme.contains(needle), "README lacks {needle:?}");
    }
}
