//! Python call facts: simple paths versus member chains, and binding
//! initializers versus bare statements.

use sloplint_core::facts::ArgShape;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"PALETTE = Palette([
    "red",
    "green",
    "blue",
    "yellow",
    "purple",
    "orange",
])


def render(name):
    greet(name)
    store.get(name)
    Palette([
        "red",
        "green",
        "blue",
        "yellow",
        "purple",
        "orange",
    ])
"#;

fn calls_of() -> Vec<(String, bool, bool, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source: SOURCE,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.py parses");
    analysis
        .facts
        .calls
        .iter()
        .map(|c| {
            (
                c.callee.to_owned(),
                c.is_simple_path,
                c.initializes_binding,
                c.args
                    .iter()
                    .any(|a| matches!(a.shape, ArgShape::Collection)),
            )
        })
        .collect()
}

#[test]
fn bare_call_is_a_simple_path() {
    let calls = calls_of();
    let greet = calls.iter().find(|c| c.0 == "greet").expect("has greet");
    assert!(greet.1);
    assert!(!greet.2);
}

#[test]
fn member_chain_is_not_a_simple_path() {
    let calls = calls_of();
    let chained = calls
        .iter()
        .find(|c| c.0 == "store.get")
        .expect("has store.get");
    assert!(!chained.1);
    assert!(!chained.2);
}

#[test]
fn constant_initializer_marks_binding() {
    let calls = calls_of();
    let init = calls
        .iter()
        .find(|c| c.0 == "Palette" && c.2)
        .expect("has binding Palette call");
    assert!(init.1);
    assert!(init.3);
}

#[test]
fn bare_multiline_call_is_not_a_binding() {
    let calls = calls_of();
    assert!(calls.iter().any(|c| c.0 == "Palette" && !c.2 && c.3));
}
