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

/// Every entry under root: regular files with their bytes, directories with
/// a marker, and symlinks with their targets. A `check` that leaves an empty
/// directory or a symlink behind changes the snapshot, so the tree
/// comparison cannot pass it as clean.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    const DIR_MARK: &[u8] = b"<dir>";
    const OTHER_MARK: &[u8] = b"<other>";
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
        for entry in entries {
            let entry = entry.unwrap_or_else(|e| panic!("entry: {e}"));
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap_or_else(|e| panic!("prefix: {e}"))
                .to_path_buf();
            let kind = entry.file_type().unwrap_or_else(|e| panic!("kind: {e}"));
            if kind.is_dir() {
                out.push((rel, DIR_MARK.to_vec()));
                stack.push(path);
            } else if kind.is_file() {
                out.push((rel, read(&path)));
            } else if kind.is_symlink() {
                let target = std::fs::read_link(&path)
                    .unwrap_or_else(|e| panic!("read_link {}: {e}", path.display()));
                out.push((rel, target.as_os_str().as_encoded_bytes().to_vec()));
            } else {
                out.push((rel, OTHER_MARK.to_vec()));
            }
        }
    }
    out.sort();
    out
}

#[cfg(unix)]
fn nlink(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("metadata {}: {e}", path.display()))
        .nlink()
}

/// An empty directory or a symlink left behind must change the snapshot: the
/// tree comparison guards `check`, so entries `snapshot` cannot see would let
/// a writing `check` pass as clean.
#[test]
fn snapshot_sees_new_directories_and_symlinks() {
    let (_tmp, root) = tempdir();
    seed(&root);
    let before = snapshot(&root);
    std::fs::create_dir_all(root.join("empty-dir"))
        .unwrap_or_else(|e| panic!("mkdir empty-dir: {e}"));
    #[cfg(unix)]
    std::os::unix::fs::symlink("sample.rs", root.join("link.rs"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));
    let after = snapshot(&root);
    assert_ne!(
        after, before,
        "an empty directory or symlink must change the snapshot"
    );
}

/// The baseline is replaced atomically, never rewritten in place: a linked
/// reader keeps seeing the complete previous baseline (A009). Hard-link
/// preservation therefore holds for every path `baseline` does not replace:
/// `check` never touches sources, so their link counts hold.
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

    write(&root, "extra.rs", "pub fn manage(flag: bool) {}\n");
    run_ok(&writing(&root));
    let second = read(&baseline);
    assert_ne!(second, first, "new findings must change the record");
    assert_eq!(
        read(&sibling),
        first,
        "a linked reader keeps seeing the complete previous baseline"
    );
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
    let first = cli(&root, &["check", "--no-cache"]);
    let second = cli(&root, &["check", "--no-cache"]);
    assert!(
        first.status.success(),
        "a clean check run must succeed, not just exit alike: {}",
        text(&first.stderr)
    );
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
        (first.status.code(), second.status.code()),
        (first.status.code(), first.status.code()),
        "repeated checks must exit alike"
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
    let first = run(&["check", "--no-cache", "--mode", "ratchet"]);
    let second = run(&["check", "--no-cache", "--mode", "ratchet"]);
    assert!(
        first.status.success(),
        "a stable ratchet run must succeed, not just exit alike: {}",
        text(&first.stderr)
    );
    assert_eq!(
        (first.status.code(), second.status.code()),
        (first.status.code(), first.status.code()),
        "repeated ratchet runs must exit alike"
    );
    assert_eq!(
        first.stdout, second.stdout,
        "repeated ratchet runs must print identical output"
    );
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
