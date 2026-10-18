//! Cross-cutting invariants: determinism, ratchet stability, path scope,
//! strictness monotonicity, option-merge, and suppression behavior.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sloplint::{RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::Severity;

/// Copies a directory tree into a fresh temp dir and returns its root.
fn copy_tree(src: &Path) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    copy_into(src, tmp.path());
    let root = tmp.path().to_path_buf();
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
fn findings(
    root: &Path,
    strictness: Strictness,
    mode: Mode,
    jobs: usize,
) -> BTreeSet<(String, u32, String, String)> {
    findings_run(root, strictness, mode, jobs, false)
}

/// Runs sloplint on `root` with the cache on and returns `(path, line, rule, severity)` tuples.
fn findings_cached(
    root: &Path,
    strictness: Strictness,
    mode: Mode,
    jobs: usize,
) -> BTreeSet<(String, u32, String, String)> {
    findings_run(root, strictness, mode, jobs, true)
}

/// Shared implementation for [`findings`] and [`findings_cached`].
fn findings_run(
    root: &Path,
    strictness: Strictness,
    mode: Mode,
    jobs: usize,
    cache: bool,
) -> BTreeSet<(String, u32, String, String)> {
    let report = run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(strictness),
        mode: Some(mode),
        use_cache: cache,
        jobs,
        ..RunOptions::default()
    })
    .unwrap_or_else(|e| panic!("run {}: {e}", root.display()));
    report
        .findings
        .into_iter()
        .map(|f| {
            (
                f.path.to_string(),
                f.line,
                f.rule.to_string(),
                sev(f.severity),
            )
        })
        .collect()
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
        cwd: root.clone(),
        root: Some(root.clone()),
        strictness: Some(Strictness::Strict),
        write_baseline: true,
        use_cache: false,
        ..RunOptions::default()
    });
    assert!(report.is_ok(), "baseline write failed: {:?}", report.err());
    for entry in walk_source_files(&root) {
        let text = std::fs::read_to_string(&entry).unwrap_or_else(|e| panic!("read: {e}"));
        std::fs::write(&entry, format!("\n{text}")).unwrap_or_else(|e| panic!("shift: {e}"));
    }
    let report = run(&RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Ratchet),
        use_cache: false,
        ..RunOptions::default()
    })
    .unwrap_or_else(|e| panic!("ratchet: {e}"));
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
    let scoped = {
        let report = run(&RunOptions {
            cwd: root.clone(),
            root: Some(root.clone()),
            paths: vec![root.join(path)],
            strictness: Some(Strictness::Strict),
            mode: Some(Mode::Threshold),
            use_cache: false,
            jobs: 2,
            ..RunOptions::default()
        })
        .unwrap_or_else(|e| panic!("scoped run: {e}"));
        report
            .findings
            .into_iter()
            .map(|f| {
                (
                    f.path.to_string(),
                    f.line,
                    f.rule.to_string(),
                    sev(f.severity),
                )
            })
            .collect::<BTreeSet<_>>()
    };
    let expected: BTreeSet<_> = all.iter().filter(|(p, ..)| p == path).cloned().collect();
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
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    write(
        root,
        "sloplint.toml",
        "[rules.\"complexity/cognitive-depth\"]\ndeny_at = 13\n",
    );
    write(
        root,
        "src/deep.rs",
        "fn deep(v: u8) {\n    if v > 0 {\n        if v > 1 {\n            if v > 2 {\n                if v > 3 {\n                    if v > 4 {\n                        if v > 5 {\n                            if v > 6 {\n                                if v > 7 {\n                                    if v > 8 {\n                                        if v > 9 {\n                                            if v > 10 {\n                                                if v > 11 {\n                                                    return;\n                                                }\n                                            }\n                                        }\n                                    }\n                                }\n                            }\n                        }\n                    }\n                }\n            }\n        }\n    }\n}\n",
    );
    let report = run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        ..RunOptions::default()
    })
    .unwrap_or_else(|e| panic!("run: {e}"));
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
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    write(
        root,
        "src/lib.rs",
        "/// Process the order.\n// sloplint-allow(slop/echo-comment): name is the contract\npub fn process_order(id: &str, account: &str, qty: u64) -> Result<u64, Box<dyn std::error::Error>> {\n    Ok(0)\n}\n",
    );
    let report = run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        ..RunOptions::default()
    })
    .unwrap_or_else(|e| panic!("run: {e}"));
    assert!(
        report
            .findings
            .iter()
            .all(|f| &*f.rule != "slop/echo-comment"),
        "suppressed finding reported: {:?}",
        report.findings.iter().map(|f| &f.rule).collect::<Vec<_>>()
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| &*f.rule == "agent/stringly-api"),
        "unsuppressed finding missing"
    );
}

#[test]
fn invalid_and_unused_suppressions_report() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    write(
        root,
        "src/lib.rs",
        "// sloplint-allow(slop/echo-comment)\npub fn suppressed_reason() {}\n// sloplint-allow(grep/generic-name): placeholder\npub fn kept() {}\n",
    );
    let report = run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Standard),
        mode: Some(Mode::Threshold),
        use_cache: false,
        ..RunOptions::default()
    })
    .unwrap_or_else(|e| panic!("run: {e}"));
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
