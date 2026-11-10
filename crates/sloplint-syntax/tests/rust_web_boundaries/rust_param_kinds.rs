//! Rust parameter primitive kinds, observed through `analyze` only.
//!
//! The adapter normalizes each parameter type once: references, mutable references, and
//! lifetimes strip away, leaving the bare type name to classify. Untyped closure
//! parameters stay `Other`, and Rust never reports defaults, keyword-only, or variadic
//! parameters.

use sloplint_core::facts::PrimitiveKind;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn params(source: &str, index: usize) -> Vec<(String, PrimitiveKind)> {
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
        "param source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis.facts.functions[index]
        .params
        .iter()
        .map(|p| (p.name.to_string(), p.primitive_kind))
        .collect()
}

fn kinds(source: &str) -> Vec<(String, PrimitiveKind)> {
    params(source, 0)
}

#[test]
fn scalar_types_classify() {
    let source =
        "fn f(a: bool, b: i32, c: u64, d: usize, e: f64, g: Vec<u8>, h: Option<bool>) {}\n";
    assert_eq!(
        kinds(source),
        vec![
            ("a".to_string(), PrimitiveKind::Bool),
            ("b".to_string(), PrimitiveKind::Int),
            ("c".to_string(), PrimitiveKind::Int),
            ("d".to_string(), PrimitiveKind::Int),
            ("e".to_string(), PrimitiveKind::Float),
            ("g".to_string(), PrimitiveKind::Other),
            ("h".to_string(), PrimitiveKind::Other),
        ]
    );
}

#[test]
fn references_and_lifetimes_strip_to_the_bare_type() {
    let source = "fn f(a: &str, b: &mut bool, c: &'x str, d: String, e: &mut Vec<u8>) {}\n";
    assert_eq!(
        kinds(source),
        vec![
            ("a".to_string(), PrimitiveKind::Str),
            ("b".to_string(), PrimitiveKind::Bool),
            ("c".to_string(), PrimitiveKind::Str),
            ("d".to_string(), PrimitiveKind::Str),
            ("e".to_string(), PrimitiveKind::Other),
        ]
    );
}

#[test]
fn self_is_not_a_counted_parameter() {
    let source = "struct S;\nimpl S {\nfn m(&self, flag: bool) {}\n}\n";
    assert_eq!(
        params(source, 0),
        vec![("flag".to_string(), PrimitiveKind::Bool)]
    );
}

#[test]
fn closure_parameters_read_the_same_types() {
    let source = "fn f() {\nlet g = |x: i32, y| x;\n}\n";
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
        "closure source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    assert_eq!(analysis.facts.functions.len(), 2);
    assert_eq!(
        params(source, 1),
        vec![
            ("x".to_string(), PrimitiveKind::Int),
            ("y".to_string(), PrimitiveKind::Other),
        ]
    );
}
