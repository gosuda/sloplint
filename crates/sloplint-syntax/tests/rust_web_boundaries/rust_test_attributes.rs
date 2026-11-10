//! Rust test exemptions: only attributes that mean `test` put a function in test scope.
//!
//! A function is in test when its own attribute list names a test attribute:
//! a bare or fully-scoped `test` path with no arguments, or a `cfg(test)`
//! gate. Any other attribute whose tokens merely contain the word "test"
//! marks production code.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn flags(source: &str) -> Vec<(String, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "case.rs",
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "attribute sources parse, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name.to_owned(), f.in_test))
        .collect()
}

#[test]
fn test_meaning_attributes_mark_their_function() {
    let source = "#[test]\nfn plain() {}\n#[tokio::test]\nasync fn tokio() {}\n#[some::path::test]\nfn scoped() {}\n#[cfg(test)]\nfn gated() {}\n";
    assert_eq!(
        flags(source),
        [
            ("plain", true),
            ("tokio", true),
            ("scoped", true),
            ("gated", true),
        ]
        .into_iter()
        .map(|(name, in_test)| (name.to_owned(), in_test))
        .collect::<Vec<_>>(),
    );
}

#[test]
fn test_word_tokens_in_other_attributes_stay_production() {
    let source = "#[cfg(not(test))]\nfn gated_out() {}\n#[allow(clippy::test_helper)]\nfn helper() {}\n#[serde(rename = \"test_case\")]\nfn renamed() {}\nfn bare() {}\n";
    assert_eq!(
        flags(source),
        [
            ("gated_out", false),
            ("helper", false),
            ("renamed", false),
            ("bare", false),
        ]
        .into_iter()
        .map(|(name, in_test)| (name.to_owned(), in_test))
        .collect::<Vec<_>>(),
    );
}

#[test]
fn module_cfg_paths_agree_with_the_function_predicate() {
    let source = "#[cfg(test)]\nmod tests {\n    fn gated() {}\n}\n#[cfg(not(test))]\nmod prod {\n    fn shipped() {}\n}\n";
    assert_eq!(
        flags(source),
        [("gated", true), ("shipped", false)]
            .into_iter()
            .map(|(name, in_test)| (name.to_owned(), in_test))
            .collect::<Vec<_>>(),
    );
}
