//! Cross-cutting invariants: option-merge, persistence refusal, rule filters, and
//! configuration loading. Corpus invariants and suppression oracles live in child modules.

#[path = "invariants/corpus_invariants.rs"]
mod corpus_invariants;
#[path = "invariants/isolated_cache.rs"]
mod isolated_cache;
#[path = "invariants/suppression_oracles.rs"]
mod suppression_oracles;

use sloplint::{Report, RunError, RunOptions, run};
use std::path::{Path, PathBuf};

use sloplint_core::config::{Mode, Strictness};
use sloplint_core::finding::Severity;
use sloplint_core::{Config, ConfigError, RuleFilter, RulePattern, RuleSet};

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

#[test]
fn rule_option_override_merges_with_preset() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "sloplint.toml",
        "[rules.\"complexity/cognitive-depth\"]\ndeny_at = 13\n",
    );
    for (count, expected) in [(12, Severity::Warn), (13, Severity::Deny)] {
        let ifs: String = (0..count)
            .map(|n| format!("    if v > {n} {{\n        return;\n    }}\n"))
            .collect();
        write(
            &root,
            "src/deep.rs",
            &format!("fn deep(v: u8) {{\n{ifs}}}\n"),
        );
        let report = run_ok(&standard(&root));
        let cognitive: Vec<_> = report
            .findings
            .iter()
            .filter(|f| &*f.rule == "complexity/cognitive-depth")
            .collect();
        assert_eq!(cognitive.len(), 1, "{count} ifs must fire once");
        assert_eq!(
            cognitive[0].severity, expected,
            "{count} ifs with deny_at 13 must be {expected:?}"
        );
    }
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

fn custom_rule(id: &str) -> String {
    format!(
        "[[custom_rules]]\nid = \"{id}\"\nlanguage = \"rust\"\nseverity = \"warn\"\nmessage = \"{id} hit\"\nwhy = \"test why\"\nfix = \"test fix\"\nquery = \"(call_expression) @match\"\n"
    )
}

#[test]
fn audit_custom_files_hidden_and_ignored_are_loaded() {
    let (_tmp, root) = tempdir();
    write(&root, "src/main.rs", "fn main() { foo(); }\nfn foo() {}\n");
    write(
        &root,
        ".hidden.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    write(
        &root,
        "ignored.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    write(
        &root,
        "ignored-dir/src.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    write(
        &root,
        ".gitignore",
        "ignored-rules.toml\nignored.rs\nignored-dir/\n",
    );
    write(
        &root,
        ".hidden-rules.toml",
        &custom_rule("custom/hidden-hit"),
    );
    write(
        &root,
        "ignored-rules.toml",
        &custom_rule("custom/ignored-hit"),
    );
    write(&root, ".rules/a.toml", &custom_rule("custom/glob-hit"));
    write(
        &root,
        ".rules/nested/deep.toml",
        &custom_rule("custom/deep-hit"),
    );
    write(
        &root,
        "ignored-dir/b.toml",
        &custom_rule("custom/ignored-dir-hit"),
    );
    write(
        &root,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\".hidden-rules.toml\", \"ignored-rules.toml\", \".rules/*.toml\", \".rules/a.toml\", \"ignored-dir/*.toml\", \"empty-dir/*.toml\"]\n",
    );
    let report = run_ok(&standard(&root));
    for id in [
        "custom/hidden-hit",
        "custom/ignored-hit",
        "custom/glob-hit",
        "custom/deep-hit",
        "custom/ignored-dir-hit",
    ] {
        assert!(
            report
                .findings
                .iter()
                .any(|f| &*f.rule == id && &*f.path == "src/main.rs"),
            "explicit selection {id} produced no finding"
        );
    }
    assert!(
        report.findings.iter().all(|f| {
            &*f.path != ".hidden.rs" && &*f.path != "ignored.rs" && &*f.path != "ignored-dir/src.rs"
        }),
        "hidden or ignored sources must stay unlinted"
    );
}

#[test]
fn audit_custom_files_empty_match_is_allowed() {
    let (_tmp, root) = tempdir();
    write(&root, "src/main.rs", "fn main() { foo(); }\nfn foo() {}\n");
    write(
        &root,
        "src/lib.rs",
        "pub fn handle() { foo(); }\nfn foo() {}\n",
    );
    write(
        &root,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\"empty-dir/*.toml\"]\n",
    );
    let report = run_ok(&standard(&root));
    assert!(
        report
            .findings
            .iter()
            .all(|f| !f.rule.starts_with("custom/")),
        "empty selection must yield no custom findings"
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| &*f.rule == "grep/generic-name" && &*f.path == "src/lib.rs"),
        "run must still report ordinary findings alongside the empty selection"
    );
}

#[cfg(unix)]
#[test]
fn audit_custom_files_dir_symlink_is_not_followed() {
    let (_tmp, base) = tempdir();
    write(&base, "outside/evil.toml", &custom_rule("custom/evil-hit"));
    let root = base.join("proj");
    mkdir(&root.join("src"));
    write(&root, "src/main.rs", "fn main() { foo(); }\nfn foo() {}\n");
    write(&root, "ok.toml", &custom_rule("custom/ok-hit"));
    symlink(base.join("outside"), root.join("link"));
    symlink(root.join("ok.toml"), root.join("alias.toml"));
    write(
        &root,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\"ok.toml\", \"alias.toml\", \"link/*.toml\", \"link/evil.toml\"]\n",
    );
    let report = run_ok(&standard(&root));
    assert!(
        report
            .findings
            .iter()
            .any(|f| &*f.rule == "custom/ok-hit" && &*f.path == "src/main.rs"),
        "direct selection must still load"
    );
    assert!(
        !fired(&report, "custom/evil-hit"),
        "symlinked directory must not be followed"
    );
}

#[test]
fn audit_unused_ratchet_reports_only_new() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(grep/generic-name): keep\npub fn foo() {}\n",
    );
    run_ok(&baseline_write(&root));
    assert_eq!(
        run_ok(&ratchet_gate(&root)).findings.len(),
        0,
        "baseline must hide its unused"
    );
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(grep/generic-name): keep\npub fn foo() {}\n// sloplint-allow(grep/generic-name): keep\npub fn bar() {}\n",
    );
    let gated = run_ok(&ratchet_gate(&root));
    assert_eq!(
        gated.findings.len(),
        1,
        "second identical suppression must surface"
    );
    assert_eq!(gated.summary.baselined, 1, "old unused must stay baselined");
    assert_eq!(
        &*gated.findings[0].rule, "meta/unused-suppression",
        "new finding must be unused"
    );
    assert_eq!(
        gated.findings[0].line, 3,
        "new unused must be the appended one"
    );
    assert!(
        !gated.findings[0].fingerprint.is_empty(),
        "unused must carry a fingerprint"
    );
    let text =
        std::fs::read_to_string(root.join("src/lib.rs")).unwrap_or_else(|e| panic!("read: {e}"));
    write(&root, "src/lib.rs", &format!("\n{text}"));
    let shifted = run_ok(&ratchet_gate(&root));
    assert_eq!(
        shifted.findings.len(),
        1,
        "blank line must not duplicate the unused"
    );
    assert_eq!(
        shifted.findings[0].line, 4,
        "shift must move the new unused"
    );
    assert_eq!(
        shifted.findings[0].fingerprint, gated.findings[0].fingerprint,
        "shift must preserve identity"
    );
}

#[test]
fn audit_unused_ordinary_ordinal_survives_suppression() {
    let (_tmp, root) = tempdir();
    write(
        &root,
        "src/lib.rs",
        "pub fn handle() {}\npub fn handle() {}\n",
    );
    let before: Vec<_> = run_ok(&standard(&root))
        .findings
        .into_iter()
        .filter(|f| &*f.rule == "grep/generic-name")
        .collect();
    assert_eq!(before.len(), 2, "need duplicate ordinals");
    assert_ne!(
        before[0].fingerprint, before[1].fingerprint,
        "duplicates must differ by ordinal"
    );
    write(
        &root,
        "src/lib.rs",
        "// sloplint-allow(grep/generic-name): dup\npub fn handle() {}\npub fn handle() {}\n",
    );
    let after = run_ok(&standard(&root));
    let kept: Vec<_> = after
        .findings
        .iter()
        .filter(|f| &*f.rule == "grep/generic-name")
        .collect();
    assert_eq!(kept.len(), 1, "suppression must drop only the first");
    assert_eq!(
        kept[0].fingerprint, before[1].fingerprint,
        "second ordinal must survive suppression"
    );
}

#[cfg(unix)]
#[test]
fn audit_custom_files_project_root_symlink_is_followed() {
    let (_tmp, base) = tempdir();
    let real = base.join("real");
    mkdir(&real.join("src"));
    write(&real, "src/main.rs", "fn main() { foo(); }\nfn foo() {}\n");
    write(&real, "ok.toml", &custom_rule("custom/ok-hit"));
    write(
        &real,
        "sloplint.toml",
        "[project]\ncustom_rule_files = [\"ok.toml\"]\n",
    );
    let linked = base.join("linked");
    symlink(&real, &linked);
    let report = run_ok(&standard(&linked));
    assert!(
        report
            .findings
            .iter()
            .any(|f| &*f.rule == "custom/ok-hit" && &*f.path == "src/main.rs"),
        "project root symlink must still load its selections"
    );
}

#[test]
fn audit_file_budget_whole_file_ratchet_identity() {
    let (_tmp, root) = tempdir();
    let src = "fn anchor() {}\n".to_owned() + &"fn f() {}\n".repeat(809);
    write(&root, "src/big.rs", &src);
    let threshold = || {
        run_ok(&RunOptions {
            strictness: Some(Strictness::Strict),
            ..audit_filter_opts(&root, &["agent/file-token-budget"])
        })
    };
    let before = threshold();
    assert_eq!(before.findings.len(), 1, "need one file-budget finding");
    run_ok(&RunOptions {
        write_baseline: true,
        strictness: Some(Strictness::Strict),
        ..audit_filter_opts(&root, &["agent/file-token-budget"])
    });
    let text =
        std::fs::read_to_string(root.join("src/big.rs")).unwrap_or_else(|e| panic!("read: {e}"));
    write(&root, "src/big.rs", &format!("\n{text}"));
    let gated = run_ok(&RunOptions {
        mode: Some(Mode::Ratchet),
        strictness: Some(Strictness::Strict),
        ..audit_filter_opts(&root, &["agent/file-token-budget"])
    });
    assert!(
        gated.findings.is_empty(),
        "leading blank line must not re-report"
    );
    assert_eq!(gated.summary.baselined, 1, "baselined count must hold");
    let after = threshold();
    assert_eq!(after.findings[0].line, 1, "shifted finding stays on line 1");
    assert_eq!(after.findings[0].message, "file has 811 lines");
}
