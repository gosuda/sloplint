//! Per-rule fixture runner: every `tests/fixtures/<family>/<rule>/` directory
//! is a mini project root whose `expected.txt` lists the findings the rule
//! must produce under the strict profile.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sloplint::{RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::Severity;
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

/// The fixture tree root inside the crate.
fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Every leaf fixture directory (one holding an `expected.txt`).
fn fixture_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    collect_dirs(&fixtures_root(), &mut dirs);
    dirs
}

fn collect_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.join("expected.txt").exists() {
                out.push(path);
            } else {
                collect_dirs(&path, out);
            }
        }
    }
    out.sort();
}

/// The `<family>/<rule>` filter a fixture dir name maps to; a trailing
/// `-near-miss` still runs the named rule so the negative case is enforced.
fn rule_filter_for(dir: &Path) -> String {
    let rel = dir
        .strip_prefix(fixtures_root())
        .unwrap_or(dir)
        .to_string_lossy();
    let rel = rel.replace('\\', "/");
    let rel = rel.split("__").next().unwrap_or(&rel);
    rel.replace("-near-miss", "").to_string()
}

/// `(path, line, severity, rule)` tuples parsed from an `expected.txt`.
fn expected(dir: &Path) -> BTreeSet<(String, u32, String, String)> {
    let text = std::fs::read_to_string(dir.join("expected.txt"))
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let mut set = BTreeSet::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (loc, rest) = line
            .split_once(' ')
            .unwrap_or_else(|| panic!("{}:{}: missing space", dir.display(), i + 1));
        let (sev, rule) = rest
            .split_once(' ')
            .unwrap_or_else(|| panic!("{}:{}: missing rule", dir.display(), i + 1));
        let (path, lno) = loc
            .rsplit_once(':')
            .unwrap_or_else(|| panic!("{}:{}: missing line", dir.display(), i + 1));
        let lno: u32 = lno
            .parse()
            .unwrap_or_else(|e| panic!("{}:{}: {e}", dir.display(), i + 1));
        assert!(
            sev == "warn" || sev == "deny",
            "{}:{}: bad severity {sev}",
            dir.display(),
            i + 1
        );
        set.insert((path.to_string(), lno, sev.to_string(), rule.to_string()));
    }
    set
}

/// Runs the fixture's single rule under `strict` and returns the same tuples.
fn actual(dir: &Path, filter: &str) -> BTreeSet<(String, u32, String, String)> {
    let pattern = RulePattern::parse(filter).unwrap_or_else(|e| panic!("{filter}: {e}"));
    let report = run(&RunOptions {
        cwd: dir.to_path_buf(),
        root: Some(dir.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![pattern]),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    })
    .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    report
        .findings
        .iter()
        .map(|finding| {
            (
                finding.path.to_string(),
                finding.line,
                severity_name(finding.severity).to_string(),
                finding.rule.to_string(),
            )
        })
        .collect()
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Warn => "warn",
        Severity::Deny => "deny",
    }
}

fn diff(
    a: &BTreeSet<(String, u32, String, String)>,
    b: &BTreeSet<(String, u32, String, String)>,
) -> Vec<String> {
    a.difference(b)
        .map(|(p, l, s, r)| format!("{p}:{l} {s} {r}"))
        .collect()
}

#[test]
fn fixtures_match_expected() {
    let mut failures = Vec::new();
    for dir in fixture_dirs() {
        let filter = rule_filter_for(&dir);
        let want = expected(&dir);
        let got = actual(&dir, &filter);
        let missing = diff(&want, &got);
        let unexpected = diff(&got, &want);
        if !missing.is_empty() || !unexpected.is_empty() {
            let mut report = format!("{filter}:");
            for line in &missing {
                report.push_str(&format!("\n  missing   {line}"));
            }
            for line in &unexpected {
                report.push_str(&format!("\n  unexpected {line}"));
            }
            failures.push(report);
        }
    }
    assert!(
        failures.is_empty(),
        "{} fixture(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
