//! A closed Rust `impl` does not leak its container into later items.
//!
//! The trait declaration itself has no body and yields no `Function` fact, so
//! the only `render` fact is the impl method with `interface_bound` set. A
//! public free function after the impl must read as public and not
//! interface-bound, which proves the container stack popped on exit.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = "pub trait Renderer {\n\tfn render(&self, verbose: bool);\n}\npub struct Console;\nimpl Renderer for Console {\n\tfn render(&self, verbose: bool) {}\n}\npub fn toggle(flag: bool) {}\n";

#[test]
fn a_public_free_function_after_a_trait_impl_is_neither_interface_bound_nor_inherits_the_impl() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source: SOURCE,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "impl container source parses, first error at {:?}",
        analysis.parse_error
    );
    let names: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        ["render", "toggle"],
        "the trait declaration yields no function; only the impl method and the free function remain"
    );
    let render = &analysis.facts.functions[0];
    assert!(
        render.interface_bound,
        "the impl method keeps its interface-bound mark"
    );
    let toggle = &analysis.facts.functions[1];
    assert!(toggle.public, "the free function after the impl is public");
    assert!(
        !toggle.interface_bound,
        "the free function is not interface-bound"
    );
    let params: Vec<(&str, Option<&str>)> = toggle.params.iter().map(|p| (p.name, p.ty)).collect();
    assert_eq!(
        params,
        [("flag", Some("bool"))],
        "the free function keeps its bool param"
    );
}
