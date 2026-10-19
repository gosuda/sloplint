use std::collections::{BTreeSet, HashMap};

use proptest::prelude::*;
use sloplint_core::{Baseline, BaselineEntry, Finding, Severity};

fn finding(rule: &str, path: &str, line: u32, context: &str) -> Finding {
    Finding {
        rule: rule.into(),
        severity: Severity::Warn,
        path: path.into(),
        line,
        column: 1,
        message: String::new(),
        fix: String::new(),
        context: context.into(),
        fingerprint: Box::from(""),
    }
}

fn reference_fingerprints(findings: &[Finding]) -> Vec<String> {
    let mut seen: HashMap<(String, String, String), u32> = HashMap::new();
    findings
        .iter()
        .map(|finding| {
            let normalized = finding
                .context
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let counter = seen
                .entry((
                    finding.rule.to_string(),
                    finding.path.to_string(),
                    normalized.clone(),
                ))
                .or_insert(0);
            let occurrence = *counter;
            *counter += 1;
            let digest = blake3::hash(
                format!(
                    "{}\0{}\0{}\0{}",
                    finding.rule, finding.path, normalized, occurrence
                )
                .as_bytes(),
            );
            digest.to_hex().as_str()[..32].to_string()
        })
        .collect()
}

fn assigned(mut findings: Vec<Finding>) -> Vec<Finding> {
    Baseline::assign_fingerprints(&mut findings);
    findings
}

#[test]
fn fingerprints_equal_the_values_written_by_the_released_binary() {
    let rule = "grep/contiguous-log-literal";
    let findings = assigned(vec![
        finding(rule, "src/a.ts", 2, "console.error(\"y\");"),
        finding(rule, "src/a.ts", 3, "console.error(\"y\");"),
        finding(rule, "src/a.ts", 4, "console.error( \"y\" );"),
        finding(rule, "src/a.ts", 5, "console.error(  \"y\"  );"),
        finding(rule, "src/a.ts", 6, "console.error(\"z\");"),
        finding(rule, "src/b.ts", 2, "console.error(\"y\");"),
        finding(rule, "src/b.ts", 3, "console.error(\"\u{e9}\");"),
    ]);
    let actual: Vec<&str> = findings.iter().map(|f| &*f.fingerprint).collect();
    assert_eq!(
        actual,
        [
            "24bf446a6d8f099663b2c252fc6091a8",
            "934aefca9dec94bf2e5ed1a4e790c18b",
            "7f4bcaa169f3c20f792c698f5e0572f7",
            "d74ecd1d6fded3c051acf6dcb58ae35d",
            "8f1ab3fbc128f7f87fc9560a0ba472fe",
            "7098a9c9552a150119fd001afd8a8692",
            "ddae3e9e5a5e9a211ddcef62614c206b",
        ]
    );
}

#[test]
fn duplicate_ordinals_count_per_rule_path_and_normalized_context() {
    let mut input = Vec::new();
    for line in 0..120 {
        input.push(finding("r/one", "a.rs", line, "call(  x )"));
        if line % 3 == 0 {
            input.push(finding("r/one", "a.rs", line, "call( x )"));
            input.push(finding("r/two", "a.rs", line, "call( x )"));
            input.push(finding("r/one", "b.rs", line, "call( x )"));
        }
    }
    let expected = reference_fingerprints(&input);
    let actual: Vec<String> = assigned(input)
        .iter()
        .map(|f| f.fingerprint.to_string())
        .collect();
    assert_eq!(actual, expected);
    let distinct: BTreeSet<&String> = actual.iter().collect();
    assert_eq!(distinct.len(), actual.len(), "every ordinal must differ");
}

#[test]
fn irregular_whitespace_normalizes_to_one_context() {
    let findings = assigned(vec![
        finding("r", "p", 1, "a b"),
        finding("r", "p", 2, "  a\t\tb  "),
        finding("r", "p", 3, "a\u{a0}b"),
        finding("r", "p", 4, "a\u{3000}\n b"),
        finding("r", "p", 5, "ab"),
        finding("r", "p", 6, ""),
        finding("r", "p", 7, "   "),
    ]);
    let expected = reference_fingerprints(&findings);
    let fingerprints: Vec<String> = findings.iter().map(|f| f.fingerprint.to_string()).collect();
    assert_eq!(fingerprints, expected);
    assert_eq!(
        fingerprints.iter().collect::<BTreeSet<_>>().len(),
        findings.len(),
        "the four `a b` variants share a counter, `ab` and the blank pair differ by context"
    );
    let blank = reference_fingerprints(&[finding("r", "p", 1, ""), finding("r", "p", 2, "")]);
    assert_eq!(fingerprints[5], blank[0]);
    assert_eq!(fingerprints[6], blank[1]);
}

#[test]
fn reassignment_is_idempotent_and_overwrites_stale_fingerprints() {
    let mut findings = vec![
        finding("r", "p", 1, "x"),
        finding("r", "p", 2, "x"),
        finding("r", "q", 3, "x"),
    ];
    findings[1].fingerprint = Box::from("stale");
    Baseline::assign_fingerprints(&mut findings);
    let first: Vec<Box<str>> = findings.iter().map(|f| f.fingerprint.clone()).collect();
    Baseline::assign_fingerprints(&mut findings);
    let second: Vec<Box<str>> = findings.iter().map(|f| f.fingerprint.clone()).collect();
    assert_eq!(first, second);
    assert!(
        first.iter().all(|f| f.len() == 32),
        "every fingerprint must be 32 hex chars, got {first:?}"
    );
    for fingerprint in &first {
        assert!(
            fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "{fingerprint} must be lowercase hex"
        );
    }
    Baseline::assign_fingerprints(&mut []);
}

fn colliding_finding() -> impl Strategy<Value = Finding> {
    (
        prop::sample::select(vec!["a/x", "b/y"]),
        prop::sample::select(vec!["src/a.rs", "src/b.rs"]),
        prop::sample::select(vec![
            "foo()",
            "  foo()  ",
            "foo(\t)",
            "foo(  )",
            "foo( )",
            "foo(\u{a0})",
            "",
            "   ",
            "\u{e9}",
        ]),
    )
        .prop_map(|(rule, path, context)| finding(rule, path, 1, context))
}

proptest! {
    #[test]
    fn assignment_agrees_with_an_owned_key_model(
        findings in proptest::collection::vec(colliding_finding(), 0..40),
    ) {
        let expected = reference_fingerprints(&findings);
        let actual: Vec<String> = assigned(findings)
            .iter()
            .map(|f| f.fingerprint.to_string())
            .collect();
        prop_assert_eq!(actual, expected);
    }
}

fn entry(path: &str, rule: &str, fingerprint: &str) -> BaselineEntry {
    BaselineEntry {
        path: path.into(),
        rule: rule.into(),
        fingerprint: fingerprint.into(),
    }
}

fn with_fingerprint(mut finding: Finding, fingerprint: &str) -> Finding {
    finding.fingerprint = fingerprint.into();
    finding
}

#[test]
fn filter_counts_duplicates_and_stale_entries_by_scope() {
    let baseline = Baseline {
        entries: BTreeSet::from([
            entry("src/a.rs", "r/one", "aa"),
            entry("src/a.rs", "r/one", "bb"),
            entry("src/b.rs", "r/one", "aa"),
            entry("other/c.rs", "r/one", "aa"),
        ]),
    };
    let findings = vec![
        with_fingerprint(finding("r/one", "src/a.rs", 1, "x"), "aa"),
        with_fingerprint(finding("r/one", "src/a.rs", 2, "y"), "aa"),
        with_fingerprint(finding("r/one", "src/a.rs", 3, "z"), "cc"),
        with_fingerprint(finding("r/two", "src/a.rs", 4, "w"), "aa"),
    ];
    let (kept, baselined, stale) = baseline.filter(findings, &|path| path.starts_with("src/"));
    assert_eq!(
        baselined, 2,
        "both findings with an identical identity are baselined"
    );
    assert_eq!(
        stale, 2,
        "src/a.rs bb and src/b.rs aa are in scope and unmatched; other/c.rs is out of scope"
    );
    let lines: Vec<u32> = kept.iter().map(|f| f.line).collect();
    assert_eq!(lines, [3, 4]);
    assert_eq!(&*kept[0].fingerprint, "cc");
    assert_eq!(&*kept[1].rule, "r/two");
}

#[test]
fn filter_requires_every_identity_component_to_match() {
    let baseline = Baseline {
        entries: BTreeSet::from([entry("src/a.rs", "r/one", "aa")]),
    };
    let findings = vec![
        with_fingerprint(finding("r/one", "src/a.rs", 1, ""), "aa"),
        with_fingerprint(finding("r/two", "src/a.rs", 2, ""), "aa"),
        with_fingerprint(finding("r/one", "src/b.rs", 3, ""), "aa"),
        with_fingerprint(finding("r/one", "src/a.rs", 4, ""), "ab"),
        with_fingerprint(finding("r/one", "src/a.rsx", 5, ""), "aa"),
        with_fingerprint(finding("r/one", "src/a.r", 6, ""), "aa"),
    ];
    let (kept, baselined, stale) = baseline.filter(findings, &|_| true);
    assert_eq!(baselined, 1);
    assert_eq!(stale, 0);
    assert_eq!(
        kept.iter().map(|f| f.line).collect::<Vec<_>>(),
        [2, 3, 4, 5, 6]
    );
}

#[test]
fn filter_handles_empty_baseline_and_empty_findings() {
    let empty = Baseline {
        entries: BTreeSet::new(),
    };
    let findings = vec![with_fingerprint(finding("r", "p", 1, ""), "aa")];
    let (kept, baselined, stale) = empty.filter(findings.clone(), &|_| true);
    assert_eq!((kept, baselined, stale), (findings, 0, 0));

    let one = Baseline {
        entries: BTreeSet::from([entry("p", "r", "aa")]),
    };
    let (kept, baselined, stale) = one.filter(Vec::new(), &|_| true);
    assert_eq!((kept.len(), baselined, stale), (0, 0, 1));
    let (_, _, stale) = one.filter(Vec::new(), &|_| false);
    assert_eq!(stale, 0);
}

#[test]
fn baseline_written_for_findings_consumes_them_after_a_json_round_trip() {
    let findings = assigned(vec![
        finding("r", "a.rs", 1, "call(x)"),
        finding("r", "a.rs", 2, "call(x)"),
        finding("s", "b.rs", 3, "call(y)"),
    ]);
    let json = Baseline::from_findings(&findings).to_json();
    let reloaded = Baseline::from_json(&json).expect("own output parses");
    let (kept, baselined, stale) = reloaded.filter(findings, &|_| true);
    assert!(
        kept.is_empty(),
        "findings baselined from themselves must all be consumed, kept {kept:?}"
    );
    assert_eq!((baselined, stale), (3, 0));
}

fn small_entry() -> impl Strategy<Value = BaselineEntry> {
    (
        prop::sample::select(vec!["src/a", "src/a.rs", "src/ab.rs", "src/b.rs", "z"]),
        prop::sample::select(vec!["r/one", "r/two", "s"]),
        prop::sample::select(vec!["", "0", "00", "01", "10", "ff"]),
    )
        .prop_map(|(path, rule, fingerprint)| entry(path, rule, fingerprint))
}

proptest! {
    #[test]
    fn filter_agrees_with_a_set_lookup_model(
        entries in proptest::collection::vec(small_entry(), 0..24),
        probes in proptest::collection::vec(small_entry(), 0..24),
        scope in prop::sample::select(vec!["src/a", "src/", "z", "nothing"]),
    ) {
        let baseline = Baseline { entries: entries.iter().cloned().collect() };
        let findings: Vec<Finding> = probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let line = u32::try_from(index).expect("small index") + 1;
                with_fingerprint(finding(&probe.rule, &probe.path, line, ""), &probe.fingerprint)
            })
            .collect();

        let mut matched: BTreeSet<&BaselineEntry> = BTreeSet::new();
        let mut expected_kept = Vec::new();
        let mut expected_baselined = 0u32;
        for (probe, finding) in probes.iter().zip(&findings) {
            match baseline.entries.get(probe) {
                Some(found) => {
                    expected_baselined += 1;
                    matched.insert(found);
                }
                None => expected_kept.push(finding.clone()),
            }
        }
        let expected_stale = baseline
            .entries
            .iter()
            .filter(|e| e.path.starts_with(scope) && !matched.contains(e))
            .count();

        let (kept, baselined, stale) = baseline.filter(findings, &|path| path.starts_with(scope));
        prop_assert_eq!(kept, expected_kept);
        prop_assert_eq!(baselined, expected_baselined);
        prop_assert_eq!(stale as usize, expected_stale);
    }
}
