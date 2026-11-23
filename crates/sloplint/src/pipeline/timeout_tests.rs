//! A parse the budget cut reports the timeout diagnostic, not a syntax
//! error, and stays in the run's report but out of the persistent cache:
//! the timeout reflects host load as much as the source, so the next run
//! of the unchanged file parses it again.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use sloplint_core::lang::Lang;
use sloplint_core::{CodeRole, Config, Pos, RuleFilter, RuleSet};
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

use super::{Analyzed, Pipeline, Probe, file_output, store_if_touched};
use crate::cache::{self, Loaded};
use crate::roles::RoleSets;

const HEALTHY: &str = "def answer : Nat := 42\n";

/// A file with no facts and no comments, so its timed-out analysis has
/// exactly the shape `analyze` returns past the budget.
const BLANK: &str = "\n";

fn with_pipeline(root: &Path, run: impl FnOnce(&Pipeline<'_>)) {
    let config = Config::default();
    let rules = RuleSet::new(&config, &RuleFilter::default()).expect("default rules build");
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let roles = RoleSets::from_config(&config).expect("default roles build");
    let loaded = Loaded::empty();
    let manifests = BTreeSet::new();
    let shadowed = HashSet::new();
    run(&Pipeline {
        rules: &rules,
        custom: &custom,
        config: &config,
        root,
        roles: &roles,
        cache: &loaded,
        manifests: &manifests,
        shadowed: &shadowed,
        active_rules: OnceLock::new(),
    });
}

fn input<'s>(rel: &'s str, source: &'s str) -> FileInput<'s> {
    FileInput {
        lang: Lang::Lean,
        path: rel,
        source,
        role: CodeRole::Library,
    }
}

/// What `analyze` returns once the shared deadline cancels the parse.
fn timed_out(analysis: Analysis<'_>) -> Analysis<'_> {
    Analysis {
        parse_error: Some(Pos { line: 1, byte: 0 }),
        timed_out: true,
        ..analysis
    }
}

/// One freshly analyzed file, as `process` hands it to the store.
fn fresh(pipe: &Pipeline<'_>, rel: &str, source: &str, analysis: &Analysis<'_>) -> Analyzed {
    let probe = Probe {
        text: source,
        hash: cache::source_hash(source),
    };
    Analyzed {
        rel: rel.to_string(),
        output: file_output(pipe, rel, &probe, analysis),
        from_cache: false,
        source: None,
    }
}

#[test]
fn a_timed_out_file_stays_out_of_the_cache_so_the_next_run_parses_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let base = std::fs::canonicalize(tmp.path()).expect("canonicalize");
    let root = base.join("project");
    std::fs::create_dir(&root).expect("project dir");
    let store = cache::open(Some(base.join("cache")), &root, &Config::default(), &[])
        .expect("cache opens outside the project root");
    with_pipeline(&root, |pipe| {
        let healthy_input = input("healthy.lean", HEALTHY);
        let healthy = analyze(&mut Parsers::new(), &healthy_input, pipe.custom);
        let slow_input = input("slow.lean", BLANK);
        let slow = timed_out(analyze(&mut Parsers::new(), &slow_input, pipe.custom));
        let files = [
            fresh(pipe, "healthy.lean", HEALTHY, &healthy),
            fresh(pipe, "slow.lean", BLANK, &slow),
        ];
        assert!(
            files[1].output.timed_out,
            "the output carries the analysis' timeout status"
        );
        store_if_touched(&store, &files, &Loaded::empty());
    });
    let reloaded = store.load();
    assert!(
        reloaded
            .take("healthy.lean", &cache::source_hash(HEALTHY))
            .is_some(),
        "a completed analysis is cached"
    );
    assert!(
        reloaded
            .take("slow.lean", &cache::source_hash(BLANK))
            .is_none(),
        "a timed-out analysis must not be cached: the next run must parse the unchanged file again"
    );
}

#[test]
fn a_timed_out_file_reports_the_timeout_diagnostic_not_a_syntax_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    with_pipeline(tmp.path(), |pipe| {
        let slow_input = input("slow.lean", BLANK);
        let slow = timed_out(analyze(&mut Parsers::new(), &slow_input, pipe.custom));
        let output = fresh(pipe, "slow.lean", BLANK, &slow).output;
        assert!(!output.parsed, "a timed-out file counts as unparsed");
        let [finding] = output.findings.as_slice() else {
            panic!(
                "a timed-out file keeps exactly one finding: {:?}",
                output.findings
            );
        };
        assert_eq!(&*finding.rule, "meta/parse-error");
        assert_eq!(finding.line, 1, "the timeout anchors at the file start");
        assert!(
            finding.message.starts_with("parse timed out"),
            "the diagnostic names the timeout, not a syntax error: {}",
            finding.message
        );
    });
}
