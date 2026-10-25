//! Shared setup: analyze one source through the public API.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

/// Analyzes `source` as `lang` at `path` with no custom rules.
pub fn run<'s>(lang: Lang, path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// Analyzes `source` and requires that it parses without a syntax error.
pub fn run_clean<'s>(lang: Lang, path: &'s str, source: &'s str) -> Analysis<'s> {
    let analysis = run(lang, path, source);
    assert!(
        analysis.parse_error.is_none(),
        "{path} must parse cleanly, first error at {:?}",
        analysis.parse_error
    );
    analysis
}
