//! Shared setup: analyze one OCaml source with or without a sibling interface.

use sloplint_core::facts::FileFacts;
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

/// Analyzes `source` as `path` with no sibling interface, requires a clean
/// parse, and returns what `read` extracts from its facts.
pub fn with_lone_facts<R>(path: &str, source: &str, read: impl FnOnce(&FileFacts<'_>) -> R) -> R {
    with_impl(path, source, false, read)
}

/// Analyzes `source` as `path` beside a sibling interface, requires a clean
/// parse, and returns what `read` extracts from its facts.
pub fn with_paired_facts<R>(path: &str, source: &str, read: impl FnOnce(&FileFacts<'_>) -> R) -> R {
    with_impl(path, source, true, read)
}

fn with_impl<R>(
    path: &str,
    source: &str,
    has_interface: bool,
    read: impl FnOnce(&FileFacts<'_>) -> R,
) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Ocaml,
        path,
        source,
        is_test_file: false,
        has_interface,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "{path} parses cleanly");
    read(&analysis.facts)
}
