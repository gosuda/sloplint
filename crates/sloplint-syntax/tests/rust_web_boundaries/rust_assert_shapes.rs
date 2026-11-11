//! Rust assertion shapes, observed through `analyze` only.
//!
//! Only `assert!` and `debug_assert!` shape the operand: a bare name, a top-level
//! `==` or `!=`, or anything else. `unreachable!` and `assert_eq!` yield no assertion
//! fact, and a trailing comma marks the message.

use sloplint_core::facts::AssertShape;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

type Row = (u32, bool, bool, AssertShape);

fn assertions(source: &str) -> Vec<Row> {
    let wrapped = format!("fn f() {{\n{source}}}\n");
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source: &wrapped,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "assertion source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .assertions
        .iter()
        .map(|a| (a.pos.line, a.has_message, a.comparison, a.shape))
        .collect()
}

#[test]
fn bare_names_and_top_level_comparisons_shape() {
    assert_eq!(
        assertions("assert!(ready);\nassert!(x == 1);\nassert!(left != right);\n"),
        vec![
            (2, false, false, AssertShape::BareName),
            (3, false, true, AssertShape::TopEqNe),
            (4, false, true, AssertShape::TopEqNe),
        ]
    );
}

#[test]
fn calls_and_operators_are_other() {
    assert_eq!(
        assertions("assert!(chain.next().is_none());\nassert!(a && b);\nassert!(!ready);\n"),
        vec![
            (2, false, false, AssertShape::Other),
            (3, false, false, AssertShape::Other),
            (4, false, false, AssertShape::Other),
        ]
    );
}

#[test]
fn nested_comparisons_do_not_shape_the_top_level() {
    assert_eq!(
        assertions("assert!(pair.0 == pair.1);\nassert!(check(a == b));\n"),
        vec![
            (2, false, true, AssertShape::TopEqNe),
            (3, false, false, AssertShape::Other),
        ]
    );
}

#[test]
fn top_level_logic_operators_are_other() {
    assert_eq!(
        assertions("assert!(a == b || c == d);\nassert!(a == b && c != d);\n"),
        vec![
            (2, false, true, AssertShape::Other),
            (3, false, true, AssertShape::Other),
        ]
    );
}

#[test]
fn comma_message_and_debug_assert_shape() {
    assert_eq!(
        assertions("assert!(ready, \"must hold\");\ndebug_assert!(x != y);\n"),
        vec![
            (2, true, false, AssertShape::BareName),
            (3, false, true, AssertShape::TopEqNe),
        ]
    );
}

#[test]
fn unreachable_and_assert_eq_yield_no_fact() {
    assert_eq!(
        assertions("unreachable!(\"here\");\nassert_eq!(a, b);\n"),
        vec![]
    );
}
