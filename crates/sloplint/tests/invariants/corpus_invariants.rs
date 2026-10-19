//! Determinism, ratchet stability, path scope, and strictness monotonicity over two corpora.
//!
//! The copied fixture tree gives breadth. The synthetic project gives what that tree cannot:
//! a root `sloplint.toml` (the fixture tree has none, so per-fixture overrides never apply to
//! it) and a file that crosses the size thresholds of the profiles. Every test insists that its
//! corpus yields findings, so disabling every rule cannot make an invariant pass vacuously.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sloplint::{Report, RunOptions};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::{Finding, Severity};

use super::{opts, run_ok, tempdir, write};

/// World rules compute a file's findings from other files, so scoping must not change them.
const WORLD_RULES: [&str; 3] = [
    "grep/homonym-definitions",
    "slop/barrel-file",
    "agent/colocated-tests",
];

/// Root config of the synthetic corpus. The threshold only changes a result when the root
/// `sloplint.toml` is honored: the preset needs two boolean parameters before it fires.
const SYNTHETIC_CONFIG: &str = "[rules.\"complexity/boolean-params\"]\nwarn_at = 1\n";

/// A labelled corpus; the temp dir is held so the root outlives the test body.
type Corpus = (&'static str, tempfile::TempDir, PathBuf);

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
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
            std::fs::create_dir_all(&target).unwrap_or_else(|e| panic!("mkdir: {e}"));
            copy_into(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap_or_else(|e| panic!("copy: {e}"));
        }
    }
}

/// One unique single-line function per line, so a long file adds no repeated-block findings.
fn numbered_functions(count: usize) -> String {
    (0..count)
        .map(|n| format!("fn item_{n}() {{}}\n"))
        .collect()
}

/// A small project for what the copied fixture tree cannot show: `src/big.rs` crosses the
/// file-size thresholds of the profiles, and `src/flags.rs` is reported only through the root
/// override.
fn synthetic_corpus() -> (tempfile::TempDir, PathBuf) {
    let (tmp, root) = tempdir();
    write(&root, "sloplint.toml", SYNTHETIC_CONFIG);
    write(&root, "src/flags.rs", "pub fn toggle(flag: bool) {}\n");
    write(&root, "src/big.rs", &numbered_functions(811));
    (tmp, root)
}

/// The corpora every invariant here runs on.
fn corpora() -> [Corpus; 2] {
    let (fixtures_tmp, fixtures) = copy_tree(&fixtures_root());
    let (synthetic_tmp, synthetic) = synthetic_corpus();
    [
        ("fixtures", fixtures_tmp, fixtures),
        ("synthetic", synthetic_tmp, synthetic),
    ]
}

/// Whole-root threshold run at `strictness` with `jobs` workers and the cache off.
fn at(root: &Path, strictness: Strictness, jobs: usize) -> RunOptions {
    RunOptions {
        strictness: Some(strictness),
        mode: Some(Mode::Threshold),
        jobs,
        ..opts(root)
    }
}

/// Every finding field in finding order, the rule rationales, and every summary count must match.
fn assert_same_report(left: &Report, right: &Report, what: &str) {
    let first = left
        .findings
        .iter()
        .zip(&right.findings)
        .position(|(l, r)| l != r);
    assert!(
        left.findings == right.findings,
        "{what}: findings differ ({} vs {}, first difference at {first:?})",
        left.findings.len(),
        right.findings.len()
    );
    assert_eq!(
        left.rules_fired, right.rules_fired,
        "{what}: rule rationales differ"
    );
    assert_eq!(
        format!("{:?}", left.summary),
        format!("{:?}", right.summary),
        "{what}: summaries differ"
    );
}

fn walk_source_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let entry = entry.unwrap_or_else(|e| panic!("entry: {e}"));
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_source_files(&path));
        } else if path
            .to_str()
            .is_some_and(|rel| sloplint_core::lang::Lang::from_path(rel).is_some())
        {
            out.push(path);
        }
    }
    out
}

/// Puts a blank line before every source file, moving each finding one line down.
fn shift_every_source_file(root: &Path) {
    for entry in walk_source_files(root) {
        let text = std::fs::read_to_string(&entry).unwrap_or_else(|e| panic!("read: {e}"));
        std::fs::write(&entry, format!("\n{text}")).unwrap_or_else(|e| panic!("shift: {e}"));
    }
}

#[test]
fn determinism_across_jobs() {
    for (label, _tmp, root) in corpora() {
        let reference = run_ok(&at(&root, Strictness::Strict, 1));
        assert!(
            !reference.findings.is_empty(),
            "{label}: the corpus produced no finding"
        );
        let jobs8 = run_ok(&at(&root, Strictness::Strict, 8));
        assert_same_report(&reference, &jobs8, &format!("{label}: jobs 1 vs 8"));
    }
}

#[test]
fn synthetic_corpus_exercises_root_config_and_file_size() {
    let (_tmp, root) = synthetic_corpus();
    let report = run_ok(&at(&root, Strictness::Standard, 2));
    assert_eq!(
        rule_paths(&report),
        [
            ("agent/file-token-budget", "src/big.rs", 1),
            ("complexity/boolean-params", "src/flags.rs", 1),
        ],
        "the long file must cross the size threshold and the root override must reach one parameter"
    );
}

#[test]
fn ratchet_is_stable_across_line_shifts_and_still_reports_new_findings() {
    for (label, _tmp, root) in corpora() {
        let count = run_ok(&at(&root, Strictness::Strict, 2)).findings.len();
        assert!(count > 0, "{label}: no finding to baseline");
        run_ok(&RunOptions {
            write_baseline: true,
            ..at(&root, Strictness::Strict, 2)
        });
        shift_every_source_file(&root);
        let gate = RunOptions {
            mode: Some(Mode::Ratchet),
            ..at(&root, Strictness::Strict, 2)
        };
        let shifted = run_ok(&gate);
        assert!(
            shifted.findings.is_empty(),
            "{label}: shifted lines were re-reported: {:?}",
            rule_paths(&shifted)
        );
        assert_eq!(
            usize::try_from(shifted.summary.baselined).ok(),
            Some(count),
            "{label}: baselined count drifted"
        );
        assert_eq!(shifted.summary.stale, 0, "{label}: shifting staled entries");
        write(&root, "zz_new/probe.rs", "use zzprobe::*;\n");
        let gated = run_ok(&gate);
        assert_eq!(
            rule_paths(&gated),
            [("grep/no-wildcard-import", "zz_new/probe.rs", 1)],
            "{label}: exactly the new violation must surface"
        );
        assert_eq!(
            usize::try_from(gated.summary.baselined).ok(),
            Some(count),
            "{label}: the new violation changed the baselined count"
        );
        assert_eq!(
            gated.summary.stale, 0,
            "{label}: the new file staled entries"
        );
    }
}

/// `(rule, path, line)` of every finding, in report order.
fn rule_paths(report: &Report) -> Vec<(&str, &str, u32)> {
    report
        .findings
        .iter()
        .map(|f| (&*f.rule, &*f.path, f.line))
        .collect()
}

/// The first finding of each world rule that fired, as `(rule, path)`.
fn world_hits(report: &Report) -> Vec<(&'static str, &str)> {
    WORLD_RULES
        .into_iter()
        .filter_map(|rule| {
            let hit = report.findings.iter().find(|f| &*f.rule == rule)?;
            Some((rule, &*hit.path))
        })
        .collect()
}

#[test]
fn scope_limits_findings_without_changing_them() {
    let mut exercised = BTreeSet::new();
    for (label, _tmp, root) in corpora() {
        let all = run_ok(&at(&root, Strictness::Strict, 2));
        let Some(first) = all.findings.first() else {
            panic!("{label}: the corpus produced no finding");
        };
        let hits = world_hits(&all);
        exercised.extend(hits.iter().map(|(rule, _)| *rule));
        let mut files = BTreeSet::from([&*first.path]);
        files.extend(hits.iter().map(|(_, path)| *path));
        for file in files {
            let expected: Vec<&Finding> =
                all.findings.iter().filter(|f| &*f.path == file).collect();
            let scoped = run_ok(&RunOptions {
                paths: vec![root.join(file)],
                ..at(&root, Strictness::Strict, 2)
            });
            let got: Vec<&Finding> = scoped.findings.iter().collect();
            assert_eq!(got, expected, "{label}: scoping to {file} changed it");
        }
        let (dir, _) = first
            .path
            .rsplit_once('/')
            .unwrap_or_else(|| panic!("{label}: {} is not in a directory", first.path));
        let prefix = format!("{dir}/");
        let expected: Vec<&Finding> = all
            .findings
            .iter()
            .filter(|f| f.path.starts_with(&prefix))
            .collect();
        let scoped = run_ok(&RunOptions {
            paths: vec![root.join(dir)],
            ..at(&root, Strictness::Strict, 2)
        });
        let got: Vec<&Finding> = scoped.findings.iter().collect();
        assert_eq!(got, expected, "{label}: scoping to {dir}/ changed it");
    }
    let declared: BTreeSet<&str> = WORLD_RULES.into_iter().collect();
    assert_eq!(
        exercised, declared,
        "the corpora must exercise every world rule the scope invariant names"
    );
}

/// `(path, line, rule)` to severity for the findings outside the `meta/` family.
fn severities(report: &Report) -> BTreeMap<(String, u32, String), Severity> {
    report
        .findings
        .iter()
        .filter(|f| !f.rule.starts_with("meta/"))
        .map(|f| ((f.path.to_string(), f.line, f.rule.to_string()), f.severity))
        .collect()
}

/// How many findings turn from warn to deny between two profiles; a finding that vanishes or
/// loses severity under the stricter profile fails.
fn escalations(
    label: &str,
    looser: &BTreeMap<(String, u32, String), Severity>,
    stricter: &BTreeMap<(String, u32, String), Severity>,
) -> usize {
    let mut escalated = 0;
    for (key, before) in looser {
        let Some(now) = stricter.get(key) else {
            panic!("{label}: {key:?} vanished under a stricter profile");
        };
        assert!(now >= before, "{label}: {key:?} lost severity");
        if now > before {
            escalated += 1;
        }
    }
    escalated
}

#[test]
fn strictness_profiles_are_monotone() {
    let profiles = [
        Strictness::Minimal,
        Strictness::Basic,
        Strictness::Standard,
        Strictness::Strict,
    ];
    for (label, _tmp, root) in corpora() {
        let runs: Vec<_> = profiles
            .iter()
            .map(|&strictness| severities(&run_ok(&at(&root, strictness, 2))))
            .collect();
        let escalated: usize = runs
            .iter()
            .zip(runs.iter().skip(1))
            .map(|(looser, stricter)| escalations(label, looser, stricter))
            .sum();
        let sizes: Vec<usize> = runs.iter().map(BTreeMap::len).collect();
        assert!(
            sizes.last() > sizes.first(),
            "{label}: strict reports no more than minimal ({sizes:?})"
        );
        assert!(
            escalated > 0,
            "{label}: no finding escalated, so the severity half of the invariant proved nothing"
        );
    }
}
