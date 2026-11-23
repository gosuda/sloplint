//! Racket require facts and file roles meet at the public project runner.

use std::path::Path;

use sloplint::{RunOptions, run};
use sloplint_core::config::{Mode, Strictness};
use sloplint_core::rules::RuleFilter;
use sloplint_core::suppress::RulePattern;

const RULE: &str = "agent/colocated-tests";
const LIBRARY: &str = "#lang racket\n(provide exported)\n(define (exported value) value)\n";

fn write(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().expect("file has a parent")).unwrap();
    std::fs::write(path, source).unwrap();
}

fn uncovered(root: &Path) -> Vec<String> {
    let report = run(&RunOptions {
        cwd: root.to_path_buf(),
        root: Some(root.to_path_buf()),
        strictness: Some(Strictness::Strict),
        mode: Some(Mode::Threshold),
        rule_filter: RuleFilter(vec![RulePattern::parse(RULE).unwrap()]),
        use_cache: false,
        jobs: 1,
        ..RunOptions::default()
    })
    .unwrap();
    assert!(
        report.findings.iter().all(|finding| &*finding.rule == RULE),
        "all fixture sources must parse and run cleanly: {:?}",
        report.findings
    );
    report
        .findings
        .iter()
        .map(|finding| finding.path.to_string())
        .collect()
}

#[test]
fn filtered_require_does_not_credit_transformer_strings() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "pkg/lib/real.rkt", LIBRARY);
    write(root.path(), "pkg/lib/ghost.rkt", LIBRARY);
    write(
        root.path(),
        "pkg/test/check.rkt",
        concat!(
            "#lang racket\n",
            "(require (filtered-in\n",
            "           (lambda (name) (if (equal? name \"../lib/ghost.rkt\") #f name))\n",
            "           \"../lib/real.rkt\"))\n",
        ),
    );
    assert_eq!(uncovered(root.path()), ["pkg/lib/ghost.rkt"]);
}

#[test]
fn nested_and_parent_relative_in_cover_the_resolved_library() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "pkg/util.rkt", LIBRARY);
    write(root.path(), "pkg/deep/lib/helpers/nested.rkt", LIBRARY);
    write(root.path(), "pkg/deep/lib/base.rkt", LIBRARY);
    write(
        root.path(),
        "pkg/deep/test/check.rkt",
        concat!(
            "#lang racket\n",
            "(require (relative-in \"../lib/base.rkt\"\n",
            "           \"../../util.rkt\"\n",
            "           (relative-in \"helpers/base.rkt\" \"nested.rkt\")))\n",
        ),
    );
    assert_eq!(uncovered(root.path()), ["pkg/deep/lib/base.rkt"]);
}

#[test]
fn conventional_racket_test_filenames_supply_evidence() {
    for extension in ["rkt", "rktl", "rktd"] {
        for stem in ["test", "tests"] {
            let root = tempfile::tempdir().unwrap();
            write(root.path(), "pkg/lib/covered.rkt", LIBRARY);
            write(root.path(), "pkg/lib/uncovered.rkt", LIBRARY);
            write(
                root.path(),
                &format!("pkg/{stem}.{extension}"),
                "#lang racket\n(require \"lib/covered.rkt\")\n",
            );
            assert_eq!(
                uncovered(root.path()),
                ["pkg/lib/uncovered.rkt"],
                "{stem}.{extension} must be a test file without changing its source language"
            );
        }
    }
}

#[test]
fn racket_module_extensions_keep_dotted_stems() {
    for extension in ["rkt", "rktl", "rktd"] {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), &format!("pkg/api.js.{extension}"), LIBRARY);
        write(root.path(), "pkg/api.rkt", LIBRARY);
        write(
            root.path(),
            "pkg/test/check.rkt",
            &format!("#lang racket\n(require \"../api.js.{extension}\")\n"),
        );
        assert_eq!(uncovered(root.path()), ["pkg/api.rkt"]);
    }
}
