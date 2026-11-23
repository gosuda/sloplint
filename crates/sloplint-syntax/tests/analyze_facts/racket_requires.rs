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
    let analysis = run_clean(Lang::Racket, "sdsl/ifc/test.rkt", REQUIRE_SHAPES);
    assert!(
        analysis.facts.imports.iter().all(|import| import.in_test),
        "requires from a test path must be test imports"
    );
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
