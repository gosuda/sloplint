//! Body-less `mod` items are not barrels, observed through `analyze` only.
//!
//! A file that only declares child modules and re-exports them is the idiomatic Rust
//! module tree, not a barrel indirection, so it yields no barrel. A file of only
//! `pub use` items still reads as a barrel, with no resolvable file candidates.

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn barrel(source: &str) -> Option<Vec<Vec<String>>> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "barrel source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis.facts.barrel.map(|barrel| barrel.targets)
}

#[test]
fn body_less_mod_files_are_not_barrels() {
    assert_eq!(
        barrel(
            "mod a;\nmod b;\nmod c;\npub use self::a::Thing;\npub use self::b::{One, Two};\npub use self::{c::Three};\n"
        ),
        None,
        "a mod-tree re-export file is idiomatic, not a barrel"
    );
    assert_eq!(
        barrel("mod a;\npub use a::Thing;\n"),
        None,
        "`self::` adds no candidates to a file the rule already skips"
    );
}

#[test]
fn private_uses_and_mixed_files_are_not_barrels() {
    assert_eq!(
        barrel("mod a;\nuse a::Thing;\n"),
        None,
        "a non-re-export use is not a barrel either"
    );
    assert_eq!(
        barrel("pub fn f() {}\npub use a::Thing;\n"),
        None,
        "code beside the re-exports is not a barrel"
    );
}

#[test]
fn pure_reexport_files_keep_empty_candidates() {
    assert_eq!(
        barrel("pub use a::Thing;\npub use b::{One, Two};\n"),
        Some(vec![Vec::new(), Vec::new()]),
        "with no child modules in the file nothing resolves to a file"
    );
}
