//! Cross-cutting invariants: determinism, ratchet stability, path scope,
//! strictness monotonicity, option-merge, and suppression behavior.

use sloplint::{Report, RunError, RunOptions, run};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::{Finding, Severity};
use sloplint_core::{Config, ConfigError, RuleFilter, RulePattern, RuleSet};

type Keys = BTreeSet<(String, u32, String, String)>;

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

/// Copies a directory tree into a fresh temp dir and returns its root.
fn copy_tree(src: &Path) -> (tempfile::TempDir, PathBuf) {
    let (tmp, root) = tempdir();
    copy_into(src, &root);
    (tmp, root)
}

fn copy_into(src: &Path, dst: &Path) {
    for entry in std::fs::read_dir(src).unwrap_or_else(|e| panic!("{}: {e}", src.display())) {
        let entry = entry.unwrap_or_else(|e| panic!("dir entry: {e}"));
        let target = dst.join(entry.file_name());
        let file_type = entry
            .file_type()
            .unwrap_or_else(|e| panic!("file type: {e}"));
        if file_type.is_dir() {
            if entry.file_name() == ".sloplint" {
                continue;
            }
            std::fs::create_dir_all(&target).unwrap_or_else(|e| panic!("mkdir: {e}"));
            copy_into(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap_or_else(|e| panic!("copy: {e}"));
        }
    }
}

/// Runs sloplint on `root` without a cache and returns `(path, line, rule, severity)` tuples.
fn findings(root: &Path, strictness: Strictness, mode: Mode, jobs: usize) -> Keys {
    findings_run(root, strictness, mode, jobs, false)
}

/// Runs sloplint on `root` with the cache on and returns `(path, line, rule, severity)` tuples.
fn findings_cached(root: &Path, strictness: Strictness, mode: Mode, jobs: usize) -> Keys {
    findings_run(root, strictness, mode, jobs, true)
}

/// Shared implementation for [`findings`] and [`findings_cached`].
fn findings_run(
    root: &Path,
    strictness: Strictness,
    mode: Mode,
    jobs: usize,
    cached: bool,
) -> Keys {
    let report = run_ok(&RunOptions {
        strictness: Some(strictness),
        mode: Some(mode),
        use_cache: cached,
        jobs,
        ..opts(root)
    });
    report.findings.iter().map(key).collect()
}

fn key(f: &Finding) -> (String, u32, String, String) {
    (
        f.path.to_string(),
        f.line,
        f.rule.to_string(),
        sev(f.severity),
    )
}

fn sev(s: Severity) -> String {
    match s {
        Severity::Warn => "warn".to_string(),
        Severity::Deny => "deny".to_string(),
    }
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap_or(&path.clone()))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", path.display()));
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

#[test]
fn determinism_across_jobs_and_cache() {
    let (_tmp, root) = copy_tree(&fixtures_root());
    let cold = findings(&root, Strictness::Strict, Mode::Threshold, 1);
    let jobs8 = findings(&root, Strictness::Strict, Mode::Threshold, 8);
    assert_eq!(cold, jobs8, "jobs=1 and jobs=8 disagree");
    let cached = findings_cached(&root, Strictness::Strict, Mode::Threshold, 8);
    let warm = findings_cached(&root, Strictness::Strict, Mode::Threshold, 8);
    assert_eq!(cold, cached, "cold and warm cache disagree");
    assert_eq!(cold, warm, "cache and no-cache disagree");
}

#[test]
fn ratchet_is_stable_across_line_shifts() {
    let (_tmp, root) = copy_tree(&fixtures_root());
    let before = findings(&root, Strictness::Strict, Mode::Threshold, 2);
    let count = before.len() as u64;
    let report = run(&RunOptions {
        strictness: Some(Strictness::Strict),
        write_baseline: true,
        ..opts(&root)
    });
    assert!(report.is_ok(), "baseline write failed: {:?}", report.err());
    for entry in walk_source_files(&root) {
        let text = std::fs::read_to_string(&entry).unwrap_or_else(|e| panic!("read: {e}"));
        std::fs::write(&entry, format!("\n{text}")).unwrap_or_else(|e| panic!("shift: {e}"));
    }
    let report = run_ok(&RunOptions {
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Ratchet),
        ..opts(&root)
    });
    assert_eq!(report.findings.len(), 0, "shifted lines re-reported");
    assert_eq!(
        u64::from(report.summary.baselined),
        count,
        "baselined count drifted"
    );
}

fn walk_source_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let entry = entry.unwrap_or_else(|e| panic!("entry: {e}"));
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != ".sloplint" {
                out.extend(walk_source_files(&path));
            }
        } else if path
            .to_str()
            .is_some_and(|rel| sloplint_core::lang::Lang::from_path(rel).is_some())
        {
            out.push(path);
        }
    }
    out
}

#[test]
fn scope_limits_findings_without_changing_them() {
    let (_tmp, root) = copy_tree(&fixtures_root());
    let all = findings(&root, Strictness::Strict, Mode::Threshold, 2);
    let Some((path, ..)) = all.iter().next() else {
        return;
    };
    let report = run_ok(&RunOptions {
        paths: vec![root.join(path)],
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        jobs: 2,
        ..opts(&root)
    });
    let scoped: Keys = report.findings.iter().map(key).collect();
    let expected: Keys = all.iter().filter(|(p, ..)| p == path).cloned().collect();
    assert_eq!(scoped, expected, "scoped run changed findings for {path}");
}

#[test]
fn strictness_profiles_are_monotone() {
    let (_tmp, root) = copy_tree(&fixtures_root());
    let mut previous: BTreeSet<(String, u32, String)> = BTreeSet::new();
    let mut severities: BTreeMap<(String, u32, String), u8> = BTreeMap::new();
    for strictness in [
        Strictness::Minimal,
        Strictness::Basic,
        Strictness::Standard,
        Strictness::Strict,
    ] {
        let current = findings(&root, strictness, Mode::Threshold, 2)
            .into_iter()
            .filter(|(_, _, rule, _)| !rule.starts_with("meta/"))
            .collect::<BTreeSet<_>>();
        let keys: BTreeSet<(String, u32, String)> = current
            .iter()
            .map(|(p, l, r, _)| (p.clone(), *l, r.clone()))
            .collect();
        assert!(
            previous.is_subset(&keys),
            "{strictness:?} dropped findings present in the looser profile"
        );
        for (path, line, rule, sev_name) in &current {
            let level = if sev_name == "deny" { 2 } else { 1 };
            let key = (path.clone(), *line, rule.clone());
            if let Some(&prior) = severities.get(&key) {
                assert!(level >= prior, "{key:?} severity dropped at {strictness:?}");
            }
            severities.insert(key, level);
        }
        previous = keys;
    }
}

#[test]
fn rule_option_override_merges_with_preset() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "sloplint.toml",
        "[rules.\"complexity/cognitive-depth\"]\ndeny_at = 13\n",
    );
    write(
        &root,
        "src/deep.rs",
        "fn deep(v: u8) {\n    if v > 0 {\n        if v > 1 {\n            if v > 2 {\n                if v > 3 {\n                    if v > 4 {\n                        if v > 5 {\n                            if v > 6 {\n                                if v > 7 {\n                                    if v > 8 {\n                                        if v > 9 {\n                                            if v > 10 {\n                                                if v > 11 {\n                                                    return;\n                                                }\n                                            }\n                                        }\n                                    }\n                                }\n                            }\n                        }\n                    }\n                }\n            }\n        }\n    }\n}\n",
    );
    let report = run_ok(&standard(&root));
    let cognitive: Vec<_> = report
        .findings
        .iter()
        .filter(|f| &*f.rule == "complexity/cognitive-depth")
        .collect();
    assert_eq!(cognitive.len(), 1, "expected one cognitive-depth finding");
    assert_eq!(
        cognitive[0].severity,
        Severity::Warn,
        "preset warn_at survived the deny_at override"
    );
}

#[test]
fn suppressions_drop_only_their_findings() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        "/// Process the order.\n// sloplint-allow(slop/echo-comment): name is the contract\npub fn process_order(id: &str, account: &str, qty: u64) -> Result<u64, Box<dyn std::error::Error>> {\n    Ok(0)\n}\n",
    );
    let report = run_ok(&standard(&root));
    assert!(
        !fired(&report, "slop/echo-comment"),
        "suppressed finding reported: {:?}",
        report.findings.iter().map(|f| &f.rule).collect::<Vec<_>>()
    );
    assert!(
        fired(&report, "agent/stringly-api"),
        "unsuppressed finding missing"
    );
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

/// A cached run must succeed: a persistence failure warns instead of failing the run.
#[cfg(unix)]
fn assert_cache_warns(root: &Path, what: &str) {
    let report = run(&RunOptions {
        use_cache: true,
        ..opts(root)
    });
    assert!(report.is_ok(), "{what} must warn, not fail: {report:?}");
}

#[test]
fn audit_persistence_baseline_dotdot_is_refused() {
    let (outer, root) = outer_project();
    set_baseline(&root, "../outside-marker.txt");
    let outside = outer.path().join("outside-marker.txt");
    plant(&outside);
    assert_refused(&baseline_write(&root), "dotdot baseline write");
    assert_sentinel(&outside, "outside marker overwritten by baseline write");
    assert_refused(&ratchet_gate(&root), "dotdot ratchet read");
    assert_sentinel(&outside, "outside marker overwritten by ratchet read");
}

#[test]
fn audit_persistence_baseline_absolute_is_refused() {
    let (outer, root) = outer_project();
    let outside = outer.path().join("outside-abs.txt");
    plant(&outside);
    let target = outside.to_str().unwrap_or_else(|| panic!("non-utf8"));
    set_baseline(&root, target);
    assert_refused(&baseline_write(&root), "absolute baseline write");
    assert_sentinel(&outside, "outside marker overwritten by absolute baseline");
}

#[cfg(unix)]
#[test]
fn audit_persistence_baseline_parent_symlink_is_refused() {
    let (outer, root) = outer_project();
    let outside = outer.path().join("outside");
    mkdir(&outside);
    symlink(&outside, root.join("link"));
    set_baseline(&root, "link/baseline.json");
    assert_refused(&baseline_write(&root), "parent symlink baseline write");
    assert!(
        !outside.join("baseline.json").exists(),
        "baseline escaped through parent symlink"
    );
}

#[cfg(unix)]
#[test]
fn audit_persistence_baseline_leaf_symlink_is_refused() {
    let (outer, root) = outer_project();
    let marker = outer.path().join("marker.txt");
    plant(&marker);
    symlink(&marker, root.join("baseline-link.json"));
    set_baseline(&root, "baseline-link.json");
    assert_refused(&baseline_write(&root), "leaf symlink baseline write");
    assert_sentinel(&marker, "outside marker overwritten through leaf symlink");
}

#[cfg(unix)]
#[test]
fn audit_persistence_baseline_dangling_link_is_refused() {
    let (outer, root) = outer_project();
    let outside = outer.path().join("outside");
    mkdir(&outside);
    symlink(&outside, root.join("link"));
    symlink("link/missing", root.join("cand"));
    set_baseline(&root, "cand");
    assert_refused(&baseline_write(&root), "dangling link baseline write");
    assert!(
        !outside.join("missing").exists(),
        "baseline write escaped through dangling link"
    );
}

#[test]
fn audit_persistence_baseline_nested_in_root_round_trips() {
    let (_tmp, root) = tempdir();
    write(&root, "sample.rs", SAMPLE);
    mkdir(&root.join("nested"));
    set_baseline(&root, "nested/baseline.json");
    run_ok(&baseline_write(&root));
    assert!(
        root.join("nested/baseline.json").is_file(),
        "nested in-root baseline was not written"
    );
    let gated = run_ok(&ratchet_gate(&root));
    assert_eq!(gated.findings.len(), 0, "in-root baseline hid nothing");
    write(&root, "extra.rs", "pub fn manage(flag: bool) {}\n");
    let gated = run_ok(&ratchet_gate(&root));
    assert!(
        gated.findings.iter().all(|f| f.path.as_ref() == "extra.rs"),
        "ratchet reported stale or missing paths: {:?}",
        gated.findings.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
    assert!(
        !gated.findings.is_empty(),
        "new in-root violation was hidden by the baseline"
    );
}

#[cfg(unix)]
#[test]
fn audit_persistence_cache_dir_symlink_stays_outside() {
    let (outer, root) = outer_project();
    let outside = outer.path().join("outside");
    mkdir(&outside);
    symlink(&outside, root.join(".sloplint"));
    assert_cache_warns(&root, "symlinked cache dir");
    assert!(
        !outside.join("cache-v1.bin").exists(),
        "cache escaped through symlinked directory"
    );
}

#[cfg(unix)]
#[test]
fn audit_persistence_cache_gitignore_symlink_is_preserved() {
    let (outer, root) = outer_project();
    mkdir(&root.join(".sloplint"));
    let marker = outer.path().join("marker.txt");
    symlink(&marker, root.join(".sloplint/.gitignore"));
    assert_cache_warns(&root, "gitignore symlink");
    assert!(
        !marker.exists(),
        "outside file created through dangling gitignore symlink"
    );
    assert!(
        std::fs::symlink_metadata(root.join(".sloplint/.gitignore"))
            .unwrap_or_else(|e| panic!("metadata: {e}"))
            .file_type()
            .is_symlink(),
        "gitignore symlink was replaced"
    );
}

#[cfg(unix)]
#[test]
fn audit_persistence_cache_temp_symlink_is_preserved() {
    let (outer, root) = outer_project();
    mkdir(&root.join(".sloplint"));
    let marker = outer.path().join("target.txt");
    plant(&marker);
    let planted = root
        .join(".sloplint")
        .join(format!("cache-v1.bin.{}.tmp", std::process::id()));
    symlink(&marker, planted);
    assert_cache_warns(&root, "planted temp symlink");
    assert_sentinel(&marker, "outside marker overwritten through temp symlink");
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

#[test]
fn audit_filter_rejects_unknown_selectors() {
    let (_tmp, root) = tempdir();
    write(&root, "sample.rs", SAMPLE);
    for (selectors, want) in [
        (vec!["grep/generic-nam"], "grep/generic-nam"),
        (vec!["typo/*"], "typo/*"),
        (vec!["grep/generic-name", "typo/*"], "typo/*"),
    ] {
        let Err(error) = run(&audit_filter_opts(&root, &selectors)) else {
            panic!("unknown selector {want} was accepted");
        };
        assert_eq!(error.exit_code(), 2, "unknown rule must exit 2");
        let RunError::Config(ConfigError::UnknownRule { id, .. }) = error else {
            panic!("unexpected error for {want}");
        };
        assert_eq!(id, want, "rejected selector mismatch");
    }
}

#[test]
fn audit_filter_ruleset_new_rejects_unknown() {
    let bad = audit_filter_of(&["grep/generic-nam"]);
    let Err(ConfigError::UnknownRule { id, .. }) = RuleSet::new(&Config::default(), &bad) else {
        panic!("RuleSet::new accepted an unknown filter");
    };
    assert_eq!(id, "grep/generic-nam", "rejected selector mismatch");
}

#[test]
fn audit_filter_accepts_valid_selectors() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "sample.rs",
        "pub fn handle(flag: bool, other: bool) { foo(); }\nfn foo() {}\n",
    );
    let call_rule = "[[custom_rules]]\nid = \"custom/no-call\"\nlanguage = \"rust\"\nseverity = \"warn\"\nmessage = \"call found\"\nwhy = \"test why\"\nfix = \"test fix\"\nquery = \"(call_expression) @match\"\n";
    write(&root, "sloplint.toml", call_rule);
    let check = |selector: &str| run_ok(&audit_filter_opts(&root, &[selector]));
    for (selector, rule, kind) in [
        ("grep/generic-name", "grep/generic-name", "exact"),
        ("grep/*", "grep/generic-name", "family"),
        ("custom/no-call", "custom/no-call", "custom"),
    ] {
        assert!(
            fired(&check(selector), rule),
            "{kind} filter missed its rule"
        );
    }
    let off = check("grep/multiline-argument");
    assert!(
        off.findings
            .iter()
            .all(|f| &*f.rule == "grep/multiline-argument"),
        "off-rule filter leaked other rules"
    );
    write(
        &root,
        "sloplint.toml",
        &format!("{call_rule}[rules.\"custom/no-call\"]\nlevel = \"off\"\n"),
    );
    let disabled = check("custom/no-call");
    assert!(
        disabled.findings.is_empty(),
        "disabled custom rule should report nothing but still be accepted"
    );
}

#[test]
fn audit_filter_cache_stays_unfiltered() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "sample.rs",
        "pub fn handle(flag: bool, other: bool) {}\n",
    );
    let filtered = run_ok(&RunOptions {
        use_cache: true,
        ..audit_filter_opts(&root, &["grep/generic-name"])
    });
    assert!(
        fired(&filtered, "grep/generic-name"),
        "filtered run missed its rule"
    );
    assert!(
        root.join(".sloplint/cache-v1.bin").exists(),
        "filtered run did not populate the cache; the full run would pass cold"
    );
    let full = run_ok(&RunOptions {
        use_cache: true,
        ..audit_filter_opts(&root, &[])
    });
    assert!(
        fired(&full, "grep/generic-name"),
        "full run after filtered cache missed generic-name"
    );
    assert!(
        fired(&full, "complexity/boolean-params"),
        "filtered cache poisoned the later full run"
    );
}

#[test]
fn invalid_and_unused_suppressions_report() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(slop/echo-comment)\npub fn suppressed_reason() {}\n// sloplint-allow(grep/generic-name): placeholder\npub fn kept() {}\n",
    );
    let report = run_ok(&standard(&root));
    let rules: BTreeSet<_> = report.findings.iter().map(|f| &*f.rule).collect();
    assert!(
        rules.contains("meta/invalid-suppression"),
        "missing reason not flagged"
    );
    assert!(
        rules.contains("meta/unused-suppression"),
        "unused suppression not flagged"
    );
}

#[test]
fn audit_config_missing_explicit_fails() {
    let (_tmp, root) = tempdir();
    write(&root, "src/lib.rs", SAMPLE);
    let missing = root.join("missing.toml");
    let error = run(&RunOptions {
        config: Some(missing.clone()),
        ..standard(&root)
    })
    .expect_err("explicit missing config must fail");
    match &error {
        RunError::PathNotFound { path } => assert_eq!(
            path, &missing,
            "missing explicit config must carry its path"
        ),
        other => panic!("expected PathNotFound, got: {other}"),
    }
    assert_eq!(error.exit_code(), 2, "missing explicit config exits 2");
}

#[test]
fn audit_config_missing_implicit_uses_defaults() {
    let (_tmp, root) = tempdir();
    write(&root, "src/lib.rs", SAMPLE);
    let report = run_ok(&standard(&root));
    assert!(
        fired(&report, "grep/generic-name"),
        "default profile must flag the generic name without a config file: {:?}",
        report.findings.iter().map(|f| &f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn audit_config_present_explicit_changes_level() {
    let (_tmp, root) = tempdir();
    write(&root, "src/lib.rs", SAMPLE);
    write(
        &root,
        "custom.toml",
        "[rules.\"grep/generic-name\"]\nlevel = \"off\"\n",
    );
    let plain = run_ok(&standard(&root));
    let custom = run_ok(&RunOptions {
        config: Some(root.join("custom.toml")),
        ..standard(&root)
    });
    let plain_sev = plain
        .findings
        .iter()
        .find(|f| &*f.rule == "grep/generic-name")
        .map(|f| f.severity);
    assert_eq!(plain_sev, Some(Severity::Warn), "default level is warn");
    assert!(
        !fired(&custom, "grep/generic-name"),
        "explicit config must disable the selected rule"
    );
}
