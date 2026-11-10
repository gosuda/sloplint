//! Filtered and gated cache runs: filters never poison later runs.

use std::path::PathBuf;
use std::process::{Command, Output};

use super::isolated_cache::{CacheHome, GENERIC_TEXT, sample_project};
use super::{tempdir, write};

/// A cache populated by a `--rule` run must not hide rules from the next unfiltered run.
#[test]
fn audit_filter_cache_stays_unfiltered() {
    let (_tmp, root) = tempdir();
    write(&root, "sloplint.toml", "");
    write(
        &root,
        "sample.rs",
        "/// Handle.\npub fn handle(flag: bool) {}\n",
    );
    let home = CacheHome::new();
    let filtered = home.check(&root, &["--rule", "grep/generic-name"]);
    assert!(
        filtered.contains("[grep/generic-name]"),
        "the filtered run missed its rule"
    );
    assert!(
        !filtered.contains("[slop/echo-comment]"),
        "the filter leaked another rule"
    );
    assert_eq!(
        home.cache_files().len(),
        1,
        "the filtered run stored no cache, so the full run would pass cold"
    );
    let full = home.check(&root, &[]);
    assert!(
        full.contains("[grep/generic-name]"),
        "the full run after the filtered cache missed generic-name"
    );
    assert!(
        full.contains("[slop/echo-comment]"),
        "the filtered cache poisoned the later full run"
    );
}

/// Gate-only flags must not invalidate stored analysis, while a rule level must.
/// The shard bytes stay identical across `--fail-on` and `--min-confidence` runs
/// and change once `grep/generic-name` is disabled.
#[test]
fn gate_only_flags_reuse_stored_shards_while_rule_levels_do_not() {
    let (_tmp, root) = sample_project();
    let home = CacheHome::new();
    let run = |extra: &[&str]| -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sloplint"));
        command
            .current_dir(&root)
            .args(["check", "--strictness", "standard"])
            .args(extra);
        home.isolate(&mut command)
            .output()
            .unwrap_or_else(|e| panic!("spawn sloplint: {e}"))
    };
    let cold = run(&["--fail-on", "never"]);
    assert!(
        cold.status.success(),
        "the cold run must succeed: {:?}: {}",
        cold.status,
        String::from_utf8_lossy(&cold.stderr)
    );
    assert!(
        String::from_utf8_lossy(&cold.stdout).contains(GENERIC_TEXT),
        "the cold run must report the generic-name finding"
    );
    let files = home.cache_files();
    assert_eq!(files.len(), 1, "one project stores one shard: {files:?}");
    let before = std::fs::read(&files[0]).unwrap_or_else(|e| panic!("read shard: {e}"));
    let gated = run(&["--fail-on", "major", "--min-confidence", "high"]);
    assert!(
        gated.status.success(),
        "the gated run must succeed: {:?}: {}",
        gated.status,
        String::from_utf8_lossy(&gated.stderr)
    );
    let after = std::fs::read(&files[0]).unwrap_or_else(|e| panic!("read shard: {e}"));
    assert_eq!(
        before, after,
        "gate-only flags must reuse the stored shard bytes"
    );
    write(
        &root,
        "sloplint.toml",
        "[rules.\"grep/generic-name\"]\nlevel = \"off\"\n",
    );
    let releveled = run(&["--fail-on", "never"]);
    assert!(
        releveled.status.success(),
        "the releveled run must succeed: {:?}: {}",
        releveled.status,
        String::from_utf8_lossy(&releveled.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&releveled.stdout).contains(GENERIC_TEXT),
        "disabling the rule must drop its finding"
    );
    let rotated = std::fs::read(&files[0]).unwrap_or_else(|e| panic!("read shard: {e}"));
    assert_ne!(
        before, rotated,
        "a rule level changes analysis and must not reuse the stored shard bytes"
    );
}

const DEEP_LEVELS: &str = "[rules.\"complexity/cognitive-depth\"]\nlevel = \"minor\"\n[rules.\"complexity/nesting-depth\"]\nlevel = \"minor\"\n";

const DEEP_FN: &str = "pub fn reconcile_ledger(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32) -> i32 {\n    if a > 0 {\n        if b > 0 {\n            if c > 0 {\n                if d > 0 {\n                    if e > 0 {\n                        if f > 0 {\n                            return 1;\n                        }\n                    }\n                }\n            }\n        }\n    }\n    0\n}\n";

fn deep_project() -> (tempfile::TempDir, PathBuf) {
    let (tmp, root) = tempdir();
    write(&root, "sloplint.toml", DEEP_LEVELS);
    write(&root, "src/lib.rs", DEEP_FN);
    (tmp, root)
}

#[test]
fn filtered_cache_keeps_sibling_depth_finding() {
    let (_tmp, root) = deep_project();
    let home = CacheHome::new();
    let cognitive = home.check(&root, &["--rule", "complexity/cognitive-depth"]);
    assert!(
        cognitive.contains("[complexity/cognitive-depth]"),
        "the cognitive run missed its finding: {cognitive}"
    );
    let nesting = home.check(&root, &["--rule", "complexity/nesting-depth"]);
    assert!(
        nesting.contains("[complexity/nesting-depth]"),
        "the cached file hid the nesting finding: {nesting}"
    );
}

#[test]
fn warm_unfiltered_matches_cold_after_filtered_warmup() {
    let (_tmp, root) = deep_project();
    let home = CacheHome::new();
    home.check(&root, &["--rule", "complexity/nesting-depth"]);
    let warm = home.check(&root, &[]);
    let cold = CacheHome::new().check(&root, &[]);
    assert_eq!(
        warm, cold,
        "a filtered warmup changed the later unfiltered run"
    );
}
