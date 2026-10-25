//! A `pub use self::m::...` re-export names the same child module as `pub use m::...`, so
//! the barrel-file rule can chase `self::` re-exports to the file that defines them.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn targets(source: &str) -> Vec<Vec<String>> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "barrel source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .barrel
        .expect("a file of only `mod` and `pub use` items is a barrel")
        .targets
}

fn candidates(module: &str) -> Vec<String> {
    vec![format!("src/{module}.rs"), format!("src/{module}/mod.rs")]
}

#[test]
fn self_prefixed_paths_resolve_to_the_child_module() {
    let source = "mod a;\nmod b;\nmod c;\npub use self::a::Thing;\npub use self::b::{One, Two};\npub use self::{c::Three};\n";
    assert_eq!(
        targets(source),
        vec![candidates("a"), candidates("b"), candidates("c")],
        "`self::a::Thing`, `self::b::{{..}}`, and `self::{{c::..}}` all start at their module"
    );
}

#[test]
fn self_prefix_changes_nothing_about_the_candidates() {
    let plain = targets("mod a;\npub use a::Thing;\n");
    let prefixed = targets("mod a;\npub use self::a::Thing;\n");
    assert_eq!(
        plain, prefixed,
        "`self::` is optional in the re-export path"
    );
}

#[test]
fn paths_to_undeclared_modules_have_no_candidates() {
    let found = targets("mod a;\npub use self::missing::Thing;\n");
    assert_eq!(
        found,
        vec![Vec::<String>::new()],
        "only modules declared in this file are candidates"
    );
}
