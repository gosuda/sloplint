//! The shipped binary's exit codes and commands, and the byte-order-mark boundary of source
//! reading. Each case runs on a small temp project with the cache off.

use std::path::Path;

use sloplint::RunOptions;
use sloplint_core::config::{Mode, Strictness};

use super::{base, filt, put, run_ok, tmp};

/// One shipped-binary run in `root`: exit code, stdout, and stderr.
fn shipped(root: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sloplint"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn {args:?}: {e}"));
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// A047: exit codes 0/1/2 and `--fail-on never`.
#[test]
fn boundary_cli_exit_codes() {
    let (_t, root) = tmp();
    put(&root, "sloplint.toml", "");
    put(&root, "src/lib.rs", "pub fn handle(a: bool, b: bool) {}\n");
    let strict = ["check", "--no-cache", "--strictness", "strict"];
    let (code, stdout, _) = shipped(&root, &strict);
    assert_eq!(code, Some(1), "a deny finding fails the default gate");
    assert!(
        stdout.contains("[complexity/boolean-params]"),
        "the deny finding is reported: {stdout}"
    );
    let never = [strict.as_slice(), &["--fail-on", "never"]].concat();
    assert_eq!(shipped(&root, &never).0, Some(0), "--fail-on never passes");
    let warn_only = shipped(&root, &["check", "--no-cache"]);
    assert_eq!(warn_only.0, Some(0), "warn findings pass the default gate");
    let (code, stdout, _) = shipped(&root, &["check", "--bogus-flag"]);
    assert_eq!(code, Some(2), "a usage error exits 2");
    assert!(stdout.is_empty(), "a usage error prints nothing on stdout");
}

// A047: the baseline suppresses the denies it records, and `init` with and without a
// configuration already present.
#[test]
fn boundary_cli_baseline_and_init_commands() {
    let (_t, root) = tmp();
    put(&root, "sloplint.toml", "");
    put(&root, "src/lib.rs", "pub fn handle(a: bool, b: bool) {}\n");
    let strict = ["check", "--no-cache", "--strictness", "strict"];
    assert_eq!(
        shipped(&root, &strict).0,
        Some(1),
        "the strict gate fails before a baseline exists"
    );
    assert_eq!(
        shipped(&root, &["baseline", "--no-cache", "--strictness", "strict"]).0,
        Some(0),
        "baseline exits 0"
    );
    let ratchet = [strict.as_slice(), &["--mode", "ratchet"]].concat();
    assert_eq!(
        shipped(&root, &ratchet).0,
        Some(0),
        "the baseline suppresses the denies it recorded"
    );
    let (code, stdout, _) = shipped(&root, &["init"]);
    assert_eq!(code, Some(2), "init refuses to overwrite a configuration");
    assert!(stdout.is_empty(), "a refused init prints nothing on stdout");
    let kept = std::fs::read_to_string(root.join("sloplint.toml"))
        .unwrap_or_else(|e| panic!("read config: {e}"));
    assert_eq!(kept, "", "a refused init leaves the configuration alone");
    let (_fresh, empty) = tmp();
    assert_eq!(
        shipped(&empty, &["init"]).0,
        Some(0),
        "init writes a config"
    );
    assert!(
        empty.join("sloplint.toml").is_file(),
        "init created sloplint.toml"
    );
}

// A047: a UTF-8 byte-order mark is stripped before analysis, so it moves no column.
#[test]
fn boundary_bom_prefix_does_not_shift_findings() {
    let (_t, root) = tmp();
    std::fs::create_dir_all(root.join("src")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    std::fs::write(root.join("src/a.rs"), b"\xef\xbb\xbfpub fn handle() {}\n")
        .unwrap_or_else(|e| panic!("write: {e}"));
    let r = run_ok(&RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        rule_filter: filt(&["grep/generic-name", "meta/*"]),
        ..base(&root)
    });
    let found: Vec<_> = r
        .findings
        .iter()
        .map(|f| (&*f.rule, f.line, f.column))
        .collect();
    assert_eq!(
        found,
        [("grep/generic-name", 1, 8)],
        "the mark must neither shift the column nor add a finding"
    );
    assert_eq!(
        r.summary.skipped, 0,
        "a marked file is analyzed, not skipped"
    );
}
