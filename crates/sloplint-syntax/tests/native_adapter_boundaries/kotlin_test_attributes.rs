//! Kotlin test exemptions: one child module per mechanism, so each fix lands with
//! its own case module while the crate links a single integration-test executable.
//!
//! A function is a test when its own annotation list names a test annotation, or when
//! the file is classified as a test file.

use crate::support::with_facts;
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const ATTRIBUTED: &str = "class Widget {\n    @Test\n    fun flag() { }\n    @ParameterizedTest\n    fun check() { }\n    @RepeatedTest\n    fun repeated() { }\n    @TestFactory\n    fun factory() { }\n    @TestTemplate\n    fun templated() { }\n    @org.junit.jupiter.api.Test\n    fun qualified() { }\n    @Test(timeout = 1000)\n    fun withArgs() { }\n    @Deprecated(\"use flag\")\n    fun deprecated_entry() { }\n    fun unannotated() { }\n}\n";

fn flags(source: &str) -> Vec<(String, bool)> {
    with_facts(Lang::Kotlin, "case.kt", source, |facts| {
        facts
            .functions
            .iter()
            .map(|f| (f.name.to_owned(), f.in_test))
            .collect()
    })
}

#[test]
fn every_test_annotation_marks_only_its_function() {
    assert_eq!(
        flags(ATTRIBUTED),
        [
            ("flag", true),
            ("check", true),
            ("repeated", true),
            ("factory", true),
            ("templated", true),
            ("qualified", true),
            ("withArgs", true),
            ("deprecated_entry", false),
            ("unannotated", false),
        ]
        .into_iter()
        .map(|(name, in_test)| (name.to_owned(), in_test))
        .collect::<Vec<_>>(),
    );
}

#[test]
fn test_file_marks_all_functions_in_test() {
    let source = "fun deprecated_entry() { }\nfun check() { }\n";
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Kotlin,
        path: "case_test.kt",
        source,
        is_test_file: true,
        has_interface: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "parses cleanly");
    let in_test: Vec<_> = analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name.to_owned(), f.in_test))
        .collect();
    assert_eq!(
        in_test,
        vec![
            ("deprecated_entry".to_string(), true),
            ("check".to_string(), true)
        ]
    );
}
