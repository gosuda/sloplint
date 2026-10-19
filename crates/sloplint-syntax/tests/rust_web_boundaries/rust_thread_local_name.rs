//! A `thread_local!` global is named after the first `static` it declares, not after
//! whichever identifier a depth-first search over the macro's tokens meets last.

use sloplint_core::facts::AmbientKind;
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn globals(source: &str) -> Vec<String> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "thread_local source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalMutable)
        .map(|a| a.name.to_string())
        .collect()
}

#[test]
fn the_first_declared_static_names_the_global() {
    let source = "use std::cell::RefCell;\n\nthread_local! {\n    static CACHE: RefCell<Vec<u8>> = RefCell::new(Vec::new());\n    pub static COUNT: RefCell<u32> = RefCell::new(0);\n}\n";
    assert_eq!(globals(source), ["CACHE"]);
}

#[test]
fn parenthesized_and_const_initialized_statics_are_named() {
    let source =
        "use std::cell::Cell;\n\nthread_local!(static ONE: Cell<u8> = const { Cell::new(0) });\n";
    assert_eq!(globals(source), ["ONE"]);
}

#[test]
fn lifetimes_in_types_are_not_declarations() {
    let source = "use std::cell::Cell;\n\nstruct Config;\n\nthread_local! {\n    static NAME: Cell<Option<&'static Config>> = Cell::new(None);\n}\n";
    assert_eq!(
        globals(source),
        ["NAME"],
        "`'static Config` declares nothing"
    );
}

#[test]
fn a_macro_without_statics_falls_back_to_its_own_name() {
    assert_eq!(globals("thread_local! {}\n"), ["thread_local"]);
}

#[test]
fn function_local_thread_locals_are_not_globals() {
    let source = "use std::cell::Cell;\n\nfn f() {\n    thread_local! {\n        static X: Cell<u8> = Cell::new(0);\n    }\n}\n";
    let found = globals(source);
    assert!(
        found.is_empty(),
        "a function-local thread_local! is not ambient state, found {found:?}"
    );
}
