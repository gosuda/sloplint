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
fn audit_persistence_baseline_dotdot_is_refused() {
    let outer = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = outer.path().join("proj");
    std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("mkdir: {e}"));
    write(&root, "sample.rs", "pub fn handle(flag: bool) {}\n");
    write(
        &root,
        "sloplint.toml",
        "[project]\nbaseline = \"../outside-marker.txt\"\n",
    );
    let outside = outer.path().join("outside-marker.txt");
    std::fs::write(&outside, "sentinel").unwrap_or_else(|e| panic!("sentinel: {e}"));
    let written = run(&RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        write_baseline: true,
        use_cache: false,
        ..RunOptions::default()
    });
    let Err(error) = written else {
        panic!("dotdot baseline escaped root");
    };
    assert!(
        error.to_string().contains("outside the project root"),
        "unexpected baseline error: {error}"
    );
    assert_eq!(error.exit_code(), 2, "refusal must be a usage error");
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap_or_else(|e| panic!("read: {e}")),
        "sentinel",
        "outside marker overwritten by baseline write"
    );
    let gated = run(&RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        mode: Some(Mode::Ratchet),
        use_cache: false,
        ..RunOptions::default()
    });
    let Err(error) = gated else {
        panic!("dotdot ratchet read escaped root");
    };
    assert!(
        error.to_string().contains("outside the project root"),
        "unexpected ratchet error: {error}"
    );
    assert_eq!(error.exit_code(), 2, "refusal must be a usage error");
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap_or_else(|e| panic!("read: {e}")),
        "sentinel",
        "outside marker overwritten by ratchet read"
    );
}

#[test]
fn audit_persistence_baseline_absolute_is_refused() {
    let outer = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = outer.path().join("proj");
    std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("mkdir: {e}"));
    write(&root, "sample.rs", "pub fn handle(flag: bool) {}\n");
    let outside = outer.path().join("outside-abs.txt");
    std::fs::write(&outside, "sentinel").unwrap_or_else(|e| panic!("sentinel: {e}"));
    let config = format!(
        "[project]\nbaseline = {:?}\n",
        outside.to_str().unwrap_or_else(|| panic!("non-utf8"))
    );
    write(&root, "sloplint.toml", &config);
    let written = run(&RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        write_baseline: true,
        use_cache: false,
        ..RunOptions::default()
    });
    let Err(error) = written else {
        panic!("absolute baseline escaped root");
    };
    assert!(
        error.to_string().contains("outside the project root"),
        "unexpected baseline error: {error}"
    );
    assert_eq!(error.exit_code(), 2, "refusal must be a usage error");
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap_or_else(|e| panic!("read: {e}")),
        "sentinel",
        "outside marker overwritten by absolute baseline"
    );
}

#[cfg(unix)]
#[test]
fn audit_persistence_baseline_parent_symlink_is_refused() {
    let outer = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = outer.path().join("proj");
    std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("mkdir: {e}"));
    let outside = outer.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap_or_else(|e| panic!("mkdir: {e}"));
    std::os::unix::fs::symlink(&outside, root.join("link"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));
    write(&root, "sample.rs", "pub fn handle(flag: bool) {}\n");
    write(
        &root,
        "sloplint.toml",
        "[project]\nbaseline = \"link/baseline.json\"\n",
    );
    let written = run(&RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        write_baseline: true,
        use_cache: false,
        ..RunOptions::default()
    });
    let Err(error) = written else {
        panic!("parent symlink baseline escaped root");
    };
    assert!(
        error.to_string().contains("outside the project root"),
        "unexpected baseline error: {error}"
    );
    assert_eq!(error.exit_code(), 2, "refusal must be a usage error");
    assert!(
        !outside.join("baseline.json").exists(),
        "baseline escaped through parent symlink"
    );
}

#[cfg(unix)]
#[test]
fn audit_persistence_baseline_leaf_symlink_is_refused() {
    let outer = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = outer.path().join("proj");
    std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("mkdir: {e}"));
    let marker = outer.path().join("marker.txt");
    std::fs::write(&marker, "sentinel").unwrap_or_else(|e| panic!("sentinel: {e}"));
    std::os::unix::fs::symlink(&marker, root.join("baseline-link.json"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));
    write(&root, "sample.rs", "pub fn handle(flag: bool) {}\n");
    write(
        &root,
        "sloplint.toml",
        "[project]\nbaseline = \"baseline-link.json\"\n",
    );
    let written = run(&RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        write_baseline: true,
        use_cache: false,
        ..RunOptions::default()
    });
    let Err(error) = written else {
        panic!("leaf symlink baseline escaped root");
    };
    assert!(
        error.to_string().contains("outside the project root"),
        "unexpected baseline error: {error}"
    );
    assert_eq!(error.exit_code(), 2, "refusal must be a usage error");
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap_or_else(|e| panic!("read: {e}")),
        "sentinel",
        "outside marker overwritten through leaf symlink"
    );
}

#[cfg(unix)]
#[test]
fn audit_persistence_baseline_dangling_link_is_refused() {
    let outer = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = outer.path().join("proj");
    std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("mkdir: {e}"));
    let outside = outer.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap_or_else(|e| panic!("mkdir: {e}"));
    std::os::unix::fs::symlink(&outside, root.join("link"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));
    std::os::unix::fs::symlink("link/missing", root.join("cand"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));
    write(&root, "sample.rs", "pub fn handle(flag: bool) {}\n");
    write(&root, "sloplint.toml", "[project]\nbaseline = \"cand\"\n");
    let written = run(&RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        write_baseline: true,
        use_cache: false,
        ..RunOptions::default()
    });
    let Err(error) = written else {
        panic!("dangling link baseline escaped root");
    };
    assert!(
        error.to_string().contains("outside the project root"),
        "unexpected baseline error: {error}"
    );
    assert_eq!(error.exit_code(), 2, "refusal must be a usage error");
    assert!(
        !outside.join("missing").exists(),
        "baseline write escaped through dangling link"
    );
}

#[test]
fn audit_persistence_baseline_nested_in_root_round_trips() {
    let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = tmp.path();
    write(root, "sample.rs", "pub fn handle(flag: bool) {}\n");
    std::fs::create_dir_all(root.join("nested")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    write(
        root,
        "sloplint.toml",
        "[project]\nbaseline = \"nested/baseline.json\"\n",
    );
    run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        write_baseline: true,
        use_cache: false,
        ..RunOptions::default()
    })
    .unwrap_or_else(|e| panic!("baseline write: {e}"));
    assert!(
        root.join("nested/baseline.json").is_file(),
        "nested in-root baseline was not written"
    );
    let gated = run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        mode: Some(Mode::Ratchet),
        use_cache: false,
        ..RunOptions::default()
    })
    .unwrap_or_else(|e| panic!("ratchet: {e}"));
    assert_eq!(gated.findings.len(), 0, "in-root baseline hid nothing");
    write(root, "extra.rs", "pub fn manage(flag: bool) {}\n");
    let gated = run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        mode: Some(Mode::Ratchet),
        use_cache: false,
        ..RunOptions::default()
    })
    .unwrap_or_else(|e| panic!("ratchet: {e}"));
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
    let outer = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = outer.path().join("proj");
    std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("mkdir: {e}"));
    write(&root, "sample.rs", "pub fn handle(flag: bool) {}\n");
    let outside = outer.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap_or_else(|e| panic!("mkdir: {e}"));
    std::os::unix::fs::symlink(&outside, root.join(".sloplint"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));
    let report = run(&RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        use_cache: true,
        ..RunOptions::default()
    });
    assert!(
        report.is_ok(),
        "symlinked cache dir must warn, not fail: {report:?}"
    );
    assert!(
        !outside.join("cache-v1.bin").exists(),
        "cache escaped through symlinked directory"
    );
}

#[cfg(unix)]
#[test]
fn audit_persistence_cache_gitignore_symlink_is_preserved() {
    let outer = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = outer.path().join("proj");
    std::fs::create_dir_all(root.join(".sloplint")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    write(&root, "sample.rs", "pub fn handle(flag: bool) {}\n");
    let marker = outer.path().join("marker.txt");
    std::os::unix::fs::symlink(&marker, root.join(".sloplint/.gitignore"))
        .unwrap_or_else(|e| panic!("symlink: {e}"));
    let report = run(&RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        use_cache: true,
        ..RunOptions::default()
    });
    assert!(
        report.is_ok(),
        "gitignore symlink must warn, not fail: {report:?}"
    );
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
    let outer = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let root = outer.path().join("proj");
    std::fs::create_dir_all(root.join(".sloplint")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    write(&root, "sample.rs", "pub fn handle(flag: bool) {}\n");
    let marker = outer.path().join("target.txt");
    std::fs::write(&marker, "sentinel").unwrap_or_else(|e| panic!("sentinel: {e}"));
    let planted = root
        .join(".sloplint")
        .join(format!("cache-v1.bin.{}.tmp", std::process::id()));
    std::os::unix::fs::symlink(&marker, &planted).unwrap_or_else(|e| panic!("symlink: {e}"));
    let report = run(&RunOptions {
        cwd: root.clone(),
        root: Some(root.clone()),
        use_cache: true,
        ..RunOptions::default()
    });
    assert!(
        report.is_ok(),
        "planted temp symlink must warn, not fail: {report:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap_or_else(|e| panic!("read: {e}")),
        "sentinel",
        "outside marker overwritten through temp symlink"
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
