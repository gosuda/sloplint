//! A closed Rust `impl` does not leak its container into later items.
//!
//! The trait declaration itself has no body and yields no `Function` fact, so
//! the only `render` fact is the impl method with `signature_forced` set. A
//! public free function after the impl must read as exported and not
//! signature-forced, which proves the container stack popped on exit.

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = "pub trait Renderer {\n\tfn render(&self, verbose: bool);\n}\npub struct Console;\nimpl Renderer for Console {\n\tfn render(&self, verbose: bool) {}\n}\npub fn toggle(flag: bool) {}\n";

#[test]
fn a_public_free_function_after_a_trait_impl_is_neither_forced_nor_inherits_the_impl() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source: SOURCE,
        role: CodeRole::Library,
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
        render.signature_forced,
        "the impl method keeps its signature-forced mark"
    );
    assert_eq!(
        render.visibility,
        Visibility::Private,
        "the impl method has no visibility modifier of its own"
    );
    let toggle = &analysis.facts.functions[1];
    assert_eq!(
        toggle.visibility,
        Visibility::Exported,
        "the free function after the impl is public"
    );
    assert!(
        !toggle.signature_forced,
        "the free function is not signature-forced"
    );
    let params: Vec<(&str, Option<&str>)> = toggle.params.iter().map(|p| (p.name, p.ty)).collect();
    assert_eq!(
        params,
        [("flag", Some("bool"))],
        "the free function keeps its bool param"
    );
}
