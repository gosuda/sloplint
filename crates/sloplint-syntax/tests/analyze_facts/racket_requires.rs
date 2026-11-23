use sloplint_core::lang::Lang;

use crate::support::run_clean;

const REQUIRE_SHAPES: &str = concat!(
    "#lang racket\n",
    "(require \"machine.rkt\"\n",
    "         \"../lang/typecheck.rkt\"\n",
    "         rackunit\n",
    "         (only-in \"verify.rkt\" verify-EENI*)\n",
    "         (prefix-in m: \"machine.rkt\")\n",
    "         (lib \"racket/match.rkt\"))\n",
);

fn modules_of(source: &str) -> Vec<String> {
    let analysis = run_clean(Lang::Racket, "sdsl/ifc/test.rkt", source);
    analysis
        .facts
        .imports
        .iter()
        .map(|import| import.module.to_string())
        .collect()
}

#[test]
fn relative_string_requires_record_and_lib_and_symbols_stay_out() {
    let modules = modules_of(REQUIRE_SHAPES);
    assert_eq!(
        modules,
        vec![
            "machine.rkt".to_string(),
            "../lang/typecheck.rkt".to_string(),
            "verify.rkt".to_string(),
            "machine.rkt".to_string(),
        ]
    );
}

#[test]
fn requires_in_test_files_are_marked_in_test() {
    for path in [
        "sdsl/ifc/test.rkt",
        "sdsl/ifc/tests.rkt",
        "sdsl/ifc/test.rktl",
        "sdsl/ifc/tests.rktl",
        "sdsl/ifc/test.rktd",
        "sdsl/ifc/tests.rktd",
    ] {
        let analysis = run_clean(Lang::Racket, path, REQUIRE_SHAPES);
        assert!(
            analysis.facts.imports.iter().all(|import| import.in_test),
            "requires from {path} must be test imports"
        );
    }
    let analysis = run_clean(Lang::Racket, "sdsl/ifc/machine.rkt", REQUIRE_SHAPES);
    assert!(
        analysis.facts.imports.iter().all(|import| !import.in_test),
        "requires from a library path must not be test imports"
    );
}

#[test]
fn provide_and_quoted_require_record_nothing() {
    let analysis = run_clean(
        Lang::Racket,
        "sdsl/ifc/machine.rkt",
        "#lang racket\n(provide machine-step)\n'(require \"ghost.rkt\")\n",
    );
    assert!(
        analysis.facts.imports.is_empty(),
        "provide and quoted datum must not record imports, found {:?}",
        analysis
            .facts
            .imports
            .iter()
            .map(|import| import.module.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn relative_in_rebases_wrapped_specs_against_its_base() {
    let analysis = run_clean(
        Lang::Racket,
        "pkg/test/check.rkt",
        "#lang racket\n(require (relative-in \"../lib/base.rkt\" \"util.rkt\"))\n",
    );
    let modules: Vec<String> = analysis
        .facts
        .imports
        .iter()
        .map(|import| import.module.to_string())
        .collect();
    assert_eq!(
        modules,
        vec!["../lib/util.rkt".to_string()],
        "the base is only the resolution anchor; the wrapped spec resolves against it"
    );
}

#[test]
fn vector_contents_are_datum_not_imports() {
    let analysis = run_clean(
        Lang::Racket,
        "pkg/test/reader.rkt",
        "#lang racket\n(define expected #((require \"../lib/ghost.rkt\")))\n",
    );
    assert!(
        analysis.facts.imports.is_empty(),
        "a require shape inside a vector literal is data, found {:?}",
        analysis
            .facts
            .imports
            .iter()
            .map(|import| import.module.to_string())
            .collect::<Vec<_>>()
    );
    let analysis = run_clean(
        Lang::Racket,
        "pkg/test/reader.rkt",
        "#lang racket\n(define expected `#(a ,(require \"../lib/ghost.rkt\")))\n",
    );
    assert_eq!(
        analysis
            .facts
            .imports
            .iter()
            .map(|import| import.module.to_string())
            .collect::<Vec<_>>(),
        vec!["../lib/ghost.rkt".to_string()],
        "an unquoted require inside a quasiquoted vector is still code"
    );
}

#[test]
fn relative_in_accepts_bare_string_anchors() {
    assert_eq!(
        modules_of(concat!(
            "#lang racket\n",
            "(require (relative-in \"base.rkt\" \"util.rkt\")\n",
            "         (relative-in \"helpers/base.rkt\" \"util.rkt\"))\n",
        )),
        ["util.rkt", "helpers/util.rkt"]
    );
}

#[test]
fn nested_relative_in_keeps_the_outer_anchor() {
    assert_eq!(
        modules_of(concat!(
            "#lang racket\n",
            "(require (relative-in \"../lib/base.rkt\"\n",
            "           (relative-in \"./helpers/base.rkt\" \"util.rkt\")))\n",
        )),
        ["../lib/helpers/util.rkt"]
    );
}

#[test]
fn relative_in_preserves_parent_traversals_above_its_anchor() {
    assert_eq!(
        modules_of(concat!(
            "#lang racket\n",
            "(require (relative-in \"../lib/base.rkt\" \"../../util.rkt\")\n",
            "         (relative-in \"./base.rkt\" \"../other.rkt\")\n",
            "         (relative-in \"../../base.rkt\" \"../third.rkt\"))\n",
        )),
        ["../../util.rkt", "../other.rkt", "../../../third.rkt"]
    );
}

#[test]
fn require_wrappers_visit_only_module_operands() {
    assert_eq!(
        modules_of(concat!(
            "#lang racket\n",
            "(require (filtered-in\n",
            "           (lambda (name) (if (equal? name \"ghost.rkt\") #f name))\n",
            "           \"real.rkt\")\n",
            "         (matching-identifiers-in #rx\"ghost\" \"matched.rkt\")\n",
            "         (only-in \"selected.rkt\" [local external])\n",
            "         (rename-in \"renamed.rkt\" [old new])\n",
            "         (prefix-in p: \"prefixed.rkt\")\n",
            "         (for-meta 1 (combine-in \"meta.rkt\" \"other.rkt\"))\n",
            "         (unknown-transformer \"unknown.rkt\"))\n",
        )),
        [
            "real.rkt",
            "matched.rkt",
            "selected.rkt",
            "renamed.rkt",
            "prefixed.rkt",
            "meta.rkt",
            "other.rkt",
        ]
    );
}

#[test]
fn relative_in_skips_unresolvable_anchors() {
    assert_eq!(
        modules_of(concat!(
            "#lang racket\n",
            "(require (relative-in racket/base \"ghost.rkt\")\n",
            "         (relative-in (lib \"base.rkt\") \"ghost.rkt\")\n",
            "         (relative-in \"/tmp/base.rkt\" \"ghost.rkt\")\n",
            "         \"real.rkt\")\n",
        )),
        ["real.rkt"]
    );
}

#[test]
fn module_wrappers_do_not_import_subtracted_or_local_submodule_paths() {
    assert_eq!(
        modules_of(concat!(
            "#lang racket\n",
            "(require (submod \"nested.rkt\" test)\n",
            "         (submod \".\" local)\n",
            "         (submod \"..\" parent)\n",
            "         (subtract-in \"whole.rkt\" \"excluded.rkt\")\n",
            "         (file \"relative.rkt\")\n",
            "         (file \"/absolute.rkt\")\n",
            "         (file \"~/home.rkt\"))\n",
        )),
        ["nested.rkt", "whole.rkt", "relative.rkt"]
    );
}

#[test]
fn deeply_nested_require_wrappers_preserve_one_import() {
    let depth = 1_024;
    let source = format!(
        "#lang racket\n(require {}\"leaf.rkt\"{})\n",
        "(combine-in ".repeat(depth),
        ")".repeat(depth)
    );
    assert_eq!(modules_of(&source), ["leaf.rkt"]);
}
