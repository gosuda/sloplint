//! A wildcard arm is enum-like when a sibling arm's pattern is a tuple-struct or struct
//! pattern, or spells a `::` path. The pattern is read through its guard wrapper, so a
//! `::` inside an `if` guard says nothing about the matched type.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn enum_like(body: &str) -> Vec<bool> {
    let source = format!("fn f(v: u8) -> u8 {{\n    match v {{\n{body}    }}\n}}\n");
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source: &source,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "match source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .wildcard_arms
        .iter()
        .map(|arm| arm.enum_like)
        .collect()
}

#[test]
fn tuple_struct_patterns_make_the_match_enum_like() {
    assert_eq!(
        enum_like("        Some(x) => x,\n        _ => 0,\n"),
        [true]
    );
    assert_eq!(
        enum_like("        Ok(a) | Err(a) => a,\n        _ => 0,\n"),
        [true],
        "alternatives are read one by one"
    );
}

#[test]
fn struct_patterns_make_the_match_enum_like() {
    assert_eq!(
        enum_like("        Circle { r } => r,\n        _ => 0,\n"),
        [true]
    );
}

#[test]
fn qualified_paths_still_make_the_match_enum_like() {
    assert_eq!(
        enum_like("        Shape::Unit => 1,\n        _ => 0,\n"),
        [true]
    );
}

#[test]
fn literals_do_not_make_the_match_enum_like() {
    assert_eq!(
        enum_like("        1 => 10,\n        2 => 20,\n        _ => 0,\n"),
        [false]
    );
}

#[test]
fn guards_do_not_make_the_match_enum_like() {
    assert_eq!(
        enum_like("        x if x > u8::MAX / 2 => 1,\n        _ => 0,\n"),
        [false],
        "the `::` belongs to the guard, not to the pattern"
    );
    assert_eq!(
        enum_like("        _ if cfg::ready() => 1,\n        _ => 0,\n"),
        [false],
        "a guarded wildcard is not itself an enum pattern"
    );
}
