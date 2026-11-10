use sloplint_core::lang::ALL_LANGS;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

#[test]
fn every_published_language_parses_and_analyzes() {
    let mut parsers = Parsers::new();
    let custom = CustomRules::compile(&[]).expect("an empty rule set compiles");
    for lang in ALL_LANGS {
        let input = FileInput {
            lang: *lang,
            path: "src/sample",
            source: "",
            role: CodeRole::Library,
        };
        let analysis = analyze(&mut parsers, &input, &custom);
        assert!(
            analysis.parse_error.is_none(),
            "{lang:?} must parse an empty file"
        );
        assert!(
            analysis.custom_hits.is_empty(),
            "{lang:?} must run no custom rules against an empty rule set"
        );
    }
}
