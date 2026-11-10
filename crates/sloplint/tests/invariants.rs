//! Cross-cutting invariants: option-merge, persistence refusal, rule filters, and
//! configuration loading. Corpus invariants and suppression oracles live in child modules.

#[path = "invariants/cache_filters.rs"]
mod cache_filters;
#[path = "invariants/config_loading.rs"]
mod config_loading;
#[path = "invariants/corpus_invariants.rs"]
mod corpus_invariants;
#[path = "invariants/custom_files.rs"]
mod custom_files;
#[path = "invariants/isolated_cache.rs"]
mod isolated_cache;
#[path = "invariants/ocaml_interface.rs"]
mod ocaml_interface;
#[path = "invariants/persistence.rs"]
mod persistence;
#[path = "invariants/ratchet_identity.rs"]
mod ratchet_identity;
#[path = "invariants/rule_filters.rs"]
mod rule_filters;
#[path = "support/spawn.rs"]
mod spawn;
#[path = "invariants/suppression_oracles.rs"]
mod suppression_oracles;
#[path = "invariants/suppression_usage.rs"]
mod suppression_usage;

use sloplint::{Report, RunOptions, run};
use std::path::{Path, PathBuf};

use sloplint_core::config::{Mode, Strictness};
use sloplint_core::{RuleFilter, RulePattern};

const SAMPLE: &str = "pub fn handle(flag: bool) {}\n";

/// A fresh temp dir and its path.
fn tempdir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn mkdir(path: &Path) {
    std::fs::create_dir_all(path).unwrap_or_else(|e| panic!("mkdir: {e}"));
}

/// Whole-root run options for `root` with the cache off; callers add only their overrides.
fn opts(root: &Path) -> RunOptions {
    RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        use_cache: false,
        ..RunOptions::default()
    }
}

/// [`opts`] with the default profile and threshold gate named explicitly.
fn standard(root: &Path) -> RunOptions {
    RunOptions {
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        ..opts(root)
    }
}

fn run_ok(options: &RunOptions) -> Report {
    run(options).unwrap_or_else(|e| panic!("run {options:?}: {e}"))
}

fn fired(report: &Report, rule: &str) -> bool {
    report.findings.iter().any(|f| &*f.rule == rule)
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap_or(&path.clone()))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", path.display()));
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

/// A `proj` root holding `sample.rs`, inside a temp dir whose other entries lie outside the root.
fn outer_project() -> (tempfile::TempDir, PathBuf) {
    let (outer, base) = tempdir();
    let root = base.join("proj");
    mkdir(&root);
    write(&root, "sample.rs", SAMPLE);
    (outer, root)
}

fn set_baseline(root: &Path, baseline: &str) {
    let config = format!("[project]\nbaseline = {baseline:?}\n");
    write(root, "sloplint.toml", &config);
}

fn baseline_write(root: &Path) -> RunOptions {
    RunOptions {
        write_baseline: true,
        ..opts(root)
    }
}

fn ratchet_gate(root: &Path) -> RunOptions {
    RunOptions {
        mode: Some(Mode::Ratchet),
        ..opts(root)
    }
}

fn plant(path: &Path) {
    std::fs::write(path, "sentinel").unwrap_or_else(|e| panic!("sentinel: {e}"));
}

fn assert_sentinel(path: &Path, message: &str) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read: {e}"));
    assert_eq!(text, "sentinel", "{message}");
}

/// `options` must fail as a usage error because a persisted path leaves the project root.
fn assert_refused(options: &RunOptions, what: &str) {
    let Err(error) = run(options) else {
        panic!("{what} escaped root");
    };
    assert!(
        error.to_string().contains("outside the project root"),
        "unexpected error for {what}: {error}"
    );
    assert_eq!(error.exit_code(), 2, "refusal must be a usage error");
}

#[cfg(unix)]
fn symlink(target: impl AsRef<Path>, link: impl AsRef<Path>) {
    std::os::unix::fs::symlink(target, link).unwrap_or_else(|e| panic!("symlink: {e}"));
}

fn audit_filter_of(selectors: &[&str]) -> RuleFilter {
    RuleFilter(
        selectors
            .iter()
            .map(|s| RulePattern::parse(s).unwrap_or_else(|e| panic!("parse: {e}")))
            .collect(),
    )
}

fn audit_filter_opts(root: &Path, selectors: &[&str]) -> RunOptions {
    RunOptions {
        rule_filter: audit_filter_of(selectors),
        ..standard(root)
    }
}

fn custom_rule(id: &str) -> String {
    format!(
        "[[custom_rules]]\nid = \"{id}\"\nlanguage = \"rust\"\nseverity = \"minor\"\nmessage = \"{id} hit\"\nwhy = \"test why\"\nfix = \"test fix\"\nquery = \"(call_expression) @match\"\n"
    )
}
