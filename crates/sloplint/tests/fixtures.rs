//! Per-rule fixture runner: every `tests/fixtures/<family>/<fixture>/` directory
//! is a mini project root whose `expected.txt` lists the findings the rule
//! must produce under the strict profile.
//!
//! A fixture is named `<rule>[-near-miss][__<language>]` inside its family
//! directory. An empty `expected.txt` is a valid negative case only when the
//! run proves the sources were analyzed, so each fixture also has to show that
//! its rule id exists, that every recognized source file in the directory was
//! analyzed and none skipped, and that no parse error hid the rule. The directory name must
//! also match what `expected.txt` proves: a positive fixture expects a finding of its own rule
//! and a near-miss never does, so a fixture that only proves silence cannot sit in a positive
//! directory.

use std::collections::BTreeSet;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};

use sloplint::{RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::Severity;
use sloplint_core::lang::Lang;
use sloplint_core::rules::{CATALOG, RuleFilter};
use sloplint_core::suppress::RulePattern;
use sloplint_core::text::normalize_rel_path;

type Key = (String, u32, String, String);

const PARSE_ERROR: &str = "meta/parse-error";
const UNUSED_SUPPRESSION: &str = "meta/unused-suppression";
const NEAR_MISS: &str = "-near-miss";
const METADATA: [&str; 2] = ["expected.txt", "sloplint.toml"];

/// The fixture tree root inside the crate.
fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// One fixture directory with everything its name and contents promise.
struct Fixture {
    /// `<family>/<directory>` with `/` separators, for messages.
    name: String,
    dir: PathBuf,
    /// The rule id the directory name selects.
    rule: String,
    near_miss: bool,
    /// Recognized source files, relative to `dir` with `/` separators.
    sources: BTreeSet<String>,
}

/// Directory entries in name order; an unreadable directory or entry fails the run.
fn entries(dir: &Path) -> Vec<DirEntry> {
    let mut found: Vec<DirEntry> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap_or_else(|e| panic!("{}: {e}", dir.display())))
        .collect();
    found.sort_by_key(DirEntry::file_name);
    found
}

/// The directories inside `dir`; anything else there would be silently ignored, so it fails.
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    entries(dir)
        .into_iter()
        .map(|entry| {
            let path = entry.path();
            let kind = entry
                .file_type()
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(
                kind.is_dir(),
                "{}: only fixture directories belong here",
                path.display()
            );
            path
        })
        .collect()
}

/// Every recognized source file under `dir`; any other file would never be analyzed, so it fails.
fn collect_sources(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    for entry in entries(dir) {
        let path = entry.path();
        let kind = entry
            .file_type()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        if kind.is_dir() {
            collect_sources(root, &path, out);
            continue;
        }
        assert!(kind.is_file(), "{}: not a regular file", path.display());
        let Some(rel) = path.strip_prefix(root).ok().and_then(normalize_rel_path) else {
            panic!("{}: not a UTF-8 path under the fixture", path.display());
        };
        if dir == root && METADATA.contains(&rel.as_str()) {
            continue;
        }
        assert!(
            Lang::from_path(&rel).is_some(),
            "{}: `{rel}` is neither fixture metadata nor a recognized source file",
            root.display()
        );
        out.insert(rel);
    }
}

impl Fixture {
    fn load(family: &Path, dir: PathBuf) -> Fixture {
        let leaf = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("{}: not a UTF-8 name", dir.display()))
            .to_string();
        let family_name = family
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("{}: not a UTF-8 name", family.display()));
        let name = format!("{family_name}/{leaf}");
        assert!(
            dir.join("expected.txt").is_file(),
            "{name}: no expected.txt, so its sources would never run"
        );
        let (stem, language) = match leaf.split_once("__") {
            Some((stem, language)) => (stem, Some(language)),
            None => (leaf.as_str(), None),
        };
        let (rule_name, near_miss) = match stem.strip_suffix(NEAR_MISS) {
            Some(rule_name) => (rule_name, true),
            None => (stem, false),
        };
        let rule = format!("{family_name}/{rule_name}");
        assert!(
            CATALOG.iter().any(|info| info.id == rule),
            "{name}: `{rule}` is not a catalog rule"
        );
        let mut sources = BTreeSet::new();
        collect_sources(&dir, &dir, &mut sources);
        assert!(!sources.is_empty(), "{name}: no source file to analyze");
        if let Some(language) = language {
            let present: BTreeSet<String> = sources
                .iter()
                .filter_map(|rel| Lang::from_path(rel))
                .map(|lang| format!("{lang:?}").to_lowercase())
                .collect();
            assert!(
                present.contains(language),
                "{name}: suffix `{language}` names no source language in the directory ({present:?})"
            );
        }
        Fixture {
            name,
            dir,
            rule,
            near_miss,
            sources,
        }
    }
}

/// Every fixture under the tree; an unreadable or malformed tree fails instead of shrinking the corpus.
fn fixtures() -> Vec<Fixture> {
    subdirs(&fixtures_root())
        .into_iter()
        .flat_map(|family| {
            subdirs(&family)
                .into_iter()
                .map(move |dir| Fixture::load(&family, dir))
        })
        .collect()
}

/// `(path, line, severity, rule)` tuples parsed from an `expected.txt`.
fn expected(dir: &Path) -> BTreeSet<Key> {
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

/// Runs the fixture's rule, plus `meta/parse-error`, under `strict`.
///
/// The parse-error rule rides along so a syntax error cannot silently switch every other rule
/// off for its file: it then shows up as an unexpected finding unless `expected.txt` lists it.
fn run_fixture(fixture: &Fixture) -> sloplint::Report {
    let mut selectors = vec![fixture.rule.as_str()];
    if fixture.rule != PARSE_ERROR {
        selectors.push(PARSE_ERROR);
    }
    if fixture.rule == UNUSED_SUPPRESSION {
        selectors.push("grep/generic-name");
    }
    let patterns = selectors
        .into_iter()
        .map(|selector| RulePattern::parse(selector).unwrap_or_else(|e| panic!("{selector}: {e}")))
        .collect();
    run(&RunOptions {
        cwd: fixture.dir.clone(),
        root: Some(fixture.dir.clone()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(patterns),
        use_cache: false,
        jobs: 2,
        ..RunOptions::default()
    })
    .unwrap_or_else(|e| panic!("{}: {e}", fixture.name))
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Warn => "warn",
        Severity::Deny => "deny",
    }
}

fn diff(a: &BTreeSet<Key>, b: &BTreeSet<Key>) -> Vec<String> {
    a.difference(b)
        .map(|(p, l, s, r)| format!("{p}:{l} {s} {r}"))
        .collect()
}

/// A positive fixture must expect a finding of its own rule and a near-miss must not: the
/// directory name is only a promise until `expected.txt` agrees with it.
fn kind_problem(fixture: &Fixture, want: &BTreeSet<Key>) -> Option<String> {
    let lists_own_rule = want.iter().any(|(_, _, _, rule)| *rule == fixture.rule);
    match (fixture.near_miss, lists_own_rule) {
        (false, false) => Some(format!(
            "positive fixture lists no `{}` finding in expected.txt; a fixture that only proves silence belongs in a near-miss directory",
            fixture.rule
        )),
        (true, true) => Some(format!(
            "near-miss fixture lists a `{}` finding in expected.txt; a fixture that proves the rule fires belongs in a positive directory",
            fixture.rule
        )),
        _ => None,
    }
}

/// What is wrong with one fixture, as report lines; empty when its run matches `expected.txt`,
/// its directory name agrees with what it proves, and every recognized source file was analyzed.
fn problems(fixture: &Fixture) -> Vec<String> {
    let report = run_fixture(fixture);
    let want = expected(&fixture.dir);
    let got: BTreeSet<Key> = report
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
        .collect();
    let mut found: Vec<String> = diff(&want, &got)
        .into_iter()
        .map(|line| format!("missing   {line}"))
        .collect();
    found.extend(
        diff(&got, &want)
            .into_iter()
            .map(|line| format!("unexpected {line}")),
    );
    found.extend(kind_problem(fixture, &want));
    let summary = &report.summary;
    if usize::try_from(summary.files).ok() != Some(fixture.sources.len()) {
        found.push(format!(
            "analyzed {} file(s) but the directory holds {} recognized source file(s): {:?}",
            summary.files,
            fixture.sources.len(),
            fixture.sources
        ));
    }
    if summary.skipped != 0 {
        found.push(format!("skipped {} file(s)", summary.skipped));
    }
    found
}

#[test]
fn fixtures_match_expected() {
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no fixture directory was found");
    let failures: Vec<String> = fixtures
        .iter()
        .filter_map(|fixture| {
            let found = problems(fixture);
            (!found.is_empty()).then(|| format!("{}:\n  {}", fixture.name, found.join("\n  ")))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} fixture(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_catalog_rule_has_positive_and_near_miss_fixtures() {
    let fixtures = fixtures();
    let mut gaps = Vec::new();
    for info in CATALOG {
        let of_rule = || fixtures.iter().filter(|fixture| fixture.rule == info.id);
        if !of_rule().any(|fixture| !fixture.near_miss) {
            gaps.push(format!("{}: no positive fixture", info.id));
        }
        if !of_rule().any(|fixture| fixture.near_miss) {
            gaps.push(format!("{}: no near-miss fixture", info.id));
        }
    }
    assert!(
        gaps.is_empty(),
        "fixture corpus does not cover the catalog:\n{}",
        gaps.join("\n")
    );
}
