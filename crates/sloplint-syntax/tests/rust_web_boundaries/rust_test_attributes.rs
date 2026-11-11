//! Rust test markers: only attributes that mean `test` open a Test region.
//!
//! A function opens a Test region when its own attribute list names a test
//! attribute: a bare or fully-scoped `test` path with no arguments, or a
//! `cfg(test)` gate. Any other attribute whose tokens merely contain the word
//! "test" marks production code. The attribute may sit on the same line as
//! the item or above it with only blank lines and comments between them.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn analyze_with(role: CodeRole, source: &str) -> sloplint_syntax::Analysis<'_> {
    let input = FileInput {
        lang: Lang::Rust,
        path: "case.rs",
        source,
        role,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &NO_CUSTOM);
    assert!(
        analysis.parse_error.is_none(),
        "attribute sources parse, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
}

fn flags(source: &str) -> Vec<(String, bool)> {
    let analysis = analyze_with(CodeRole::Library, source);
    analysis
        .facts
        .functions
        .iter()
        .map(|f| {
            let marked = analysis.facts.regions.iter().any(|region| {
                region.role == CodeRole::Test
                    && region.start_line <= f.pos.line
                    && f.pos.line <= region.end_line
            });
            (f.name.to_owned(), marked)
        })
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
        .map(|(name, marked)| (name.to_owned(), marked))
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
        .map(|(name, marked)| (name.to_owned(), marked))
        .collect::<Vec<_>>(),
    );
}

#[test]
fn attribute_on_the_same_line_as_the_item_marks_it() {
    assert_eq!(
        flags("#[tokio::test] async fn same_line() {}\n"),
        [("same_line".to_owned(), true)]
    );
    assert_eq!(
        flags("#[test] fn same_line_plain() {}\n"),
        [("same_line_plain".to_owned(), true)]
    );
}

#[test]
fn blank_lines_and_comments_between_attribute_and_item_still_mark_it() {
    let source = "#[test]\n\n// a comment\n\nfn spaced() {}\n";
    assert_eq!(flags(source), [("spaced".to_owned(), true)]);
    let source = "#[allow(dead_code)]\n#[test]\nfn layered() {}\n";
    assert_eq!(flags(source), [("layered".to_owned(), true)]);
}

#[test]
fn module_cfg_paths_agree_with_the_function_predicate() {
    let source = "#[cfg(test)]\nmod tests {\n    fn gated() {}\n}\n#[cfg(not(test))]\nmod prod {\n    fn shipped() {}\n}\n";
    assert_eq!(
        flags(source),
        [("gated", true), ("shipped", false)]
            .into_iter()
            .map(|(name, marked)| (name.to_owned(), marked))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn cfg_test_module_opens_a_test_region() {
    let analysis = analyze_with(
        CodeRole::Library,
        "#[cfg(test)]\nmod tests {\n    fn gated() {}\n}\n",
    );
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
    assert!(
        analysis.facts.regions.iter().any(|region| {
            region.role == CodeRole::Test && region.start_line == 1 && region.end_line == 4
        }),
        "the region covers the module, got {:?}",
        analysis.facts.regions
    );
}

#[test]
fn bench_attribute_opens_a_bench_region_without_a_case() {
    let analysis = analyze_with(CodeRole::Library, "#[bench]\nfn speed() {}\n");
    assert!(
        analysis.facts.regions.iter().any(|region| {
            region.role == CodeRole::Bench && region.start_line <= 2 && 2 <= region.end_line
        }),
        "the bench region covers the function, got {:?}",
        analysis.facts.regions
    );
    assert_eq!(analysis.facts.role, CodeRole::Library);
}

#[test]
fn test_attribute_opens_a_test_region() {
    let analysis = analyze_with(CodeRole::Test, "#[test]\nfn checks() {}\n");
    assert!(
        analysis
            .facts
            .regions
            .iter()
            .any(|region| region.role == CodeRole::Test),
        "the test function opens a Test region, got {:?}",
        analysis.facts.regions
    );
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn test_attributes_with_arguments_mark_test_functions() {
    let source = "#[tokio::test(flavor = \"multi_thread\")]\nasync fn fetch() {}\n#[test_case(1, 2)]\nfn casey() {}\n";
    assert_eq!(
        flags(source),
        [("fetch".to_owned(), true), ("casey".to_owned(), false)]
    );
}

#[test]
fn cfg_all_with_test_marks_the_module() {
    let gated = "#[cfg(all(test, feature = \"net\"))]\nmod net_tests {\n    fn ok() {}\n}\n";
    let analysis = analyze_with(CodeRole::Library, gated);
    assert!(
        analysis.facts.regions.iter().any(|region| {
            region.role == CodeRole::Test && region.start_line == 1 && region.end_line == 4
        }),
        "the region covers the all(test) module, got {:?}",
        analysis.facts.regions
    );
    let open = "#[cfg(any(test, feature = \"net\"))]\nmod net_tests {\n    fn ok() {}\n}\n";
    let analysis = analyze_with(CodeRole::Library, open);
    assert!(
        analysis.facts.regions.is_empty(),
        "any(test) marks nothing, got {:?}",
        analysis.facts.regions
    );
}

#[test]
fn proptest_macro_with_test_fn_marks_a_test_region() {
    let source = "proptest! {\n    #[test]\n    fn holds(a in 0u8..10) {}\n}\n";
    let analysis = analyze_with(CodeRole::Library, source);
    assert!(
        analysis.facts.has_inline_tests,
        "a Test region sets the inline marker"
    );
    assert!(
        analysis.facts.regions.iter().any(|region| {
            region.role == CodeRole::Test && region.start_line == 1 && region.end_line == 4
        }),
        "the region covers the proptest block, got {:?}",
        analysis.facts.regions
    );
    let analysis = analyze_with(CodeRole::Test, "proptest! {}\n");
    assert!(
        analysis.facts.regions.iter().any(|region| {
            region.role == CodeRole::Test && region.start_line == 1 && region.end_line == 1
        }),
        "an empty proptest block still marks its span, got {:?}",
        analysis.facts.regions
    );
    assert_eq!(analysis.facts.role, CodeRole::Test);
}

#[test]
fn bare_proptest_and_quickcheck_blocks_open_test_regions() {
    for source in [
        "proptest! {}\n",
        "quickcheck! {\n    fn prop(xs: Vec<u8>) -> bool {\n        xs.is_empty()\n    }\n}\n",
    ] {
        let analysis = analyze_with(CodeRole::Library, source);
        assert!(
            analysis.facts.regions.iter().any(|region| {
                region.role == CodeRole::Test && region.start_line <= 1 && 1 <= region.end_line
            }),
            "a property macro opens a Test region with no inner test fn, got {:?}",
            analysis.facts.regions
        );
        assert!(
            analysis.facts.has_inline_tests,
            "a Test region sets the inline marker"
        );
    }
}

#[test]
fn test_marks_set_the_function_in_test_flag() {
    let source = "#[test]\nfn checked() {}\nmod tests {\n    fn helper() {}\n}\nfn shipped() {}\n";
    let analysis = analyze_with(CodeRole::Library, source);
    let marks: Vec<(String, bool)> = analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name.to_owned(), f.in_test))
        .collect();
    assert_eq!(
        marks,
        [
            ("checked".to_owned(), true),
            ("helper".to_owned(), true),
            ("shipped".to_owned(), false),
        ]
    );
}
