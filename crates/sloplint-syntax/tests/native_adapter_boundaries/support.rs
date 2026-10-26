//! Shared setup: analyze one source through the public API and read owned values from its facts.

use sloplint_core::facts::FileFacts;
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

/// The C-family grammars that share one declarator model, each with a path that selects it.
pub const C_FAMILY: [(Lang, &str); 3] = [
    (Lang::C, "case.c"),
    (Lang::Cpp, "case.cpp"),
    (Lang::ObjC, "case.m"),
];

/// Analyzes `source` as `lang`, requires a clean parse, and returns what `read` extracts.
pub fn with_facts<R>(
    lang: Lang,
    path: &str,
    source: &str,
    read: impl FnOnce(&FileFacts<'_>) -> R,
) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "{path} parses cleanly");
    read(&analysis.facts)
}
