//! A wildcard arm is enum-like with two or more named sibling variants.
//!
//! The adapter reports the scrutinee shape, the count of distinct named sibling
//! variants, and whether a sibling is a nullable constructor. A single named variant is
//! a one-variant extract, and any `Some`/`None`/`Ok`/`Err` sibling is an
//! option-or-result match, so neither reads as enum-like.

use sloplint_core::facts::ScrutineeKind;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

type Row = (bool, ScrutineeKind, u32, bool);

fn arms_of(source: &str) -> Vec<Row> {
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
        "match source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .wildcard_arms
        .iter()
        .map(|arm| {
            (
                arm.enum_like,
                arm.scrutinee,
                arm.variant_count,
                arm.has_nullable_ctor,
            )
        })
        .collect()
}

fn arms(body: &str) -> Vec<Row> {
    arms_of(&format!(
        "fn f(v: Status) -> u8 {{\n    match v {{\n{body}    }}\n}}\n"
    ))
}

fn arms_in(header: &str, scrutinee: &str, body: &str) -> Vec<Row> {
    arms_of(&format!(
        "{header} {{\n    match {scrutinee} {{\n{body}    }}\n}}\n"
    ))
}

fn row(like: bool, scrutinee: ScrutineeKind, count: u32, nullable: bool) -> Row {
    (like, scrutinee, count, nullable)
}

const IDENT: ScrutineeKind = ScrutineeKind::Ident;

#[test]
fn two_named_variants_make_the_match_enum_like() {
    assert_eq!(
        arms("        Status::Active => 1,\n        Status::Idle => 2,\n        _ => 0,\n"),
        [row(true, IDENT, 2, false)]
    );
}

#[test]
fn tuple_struct_patterns_count_by_variant() {
    assert_eq!(
        arms("        Point(x, y) => x,\n        Line(a) => a,\n        _ => 0,\n"),
        [row(true, IDENT, 2, false)]
    );
}

#[test]
fn one_named_variant_is_an_extract_not_a_catch_all() {
    assert_eq!(
        arms("        Shape::Unit => 1,\n        _ => 0,\n"),
        [row(false, IDENT, 1, false)]
    );
    assert_eq!(
        arms("        Circle { r } => r,\n        _ => 0,\n"),
        [row(false, IDENT, 1, false)]
    );
}

#[test]
fn nullable_constructors_skip_the_count() {
    assert_eq!(
        arms("        Some(x) => x,\n        _ => 0,\n"),
        [row(false, IDENT, 0, true)]
    );
    assert_eq!(
        arms("        Ok(a) | Err(a) => a,\n        _ => 0,\n"),
        [row(false, IDENT, 0, true)]
    );
    assert_eq!(
        arms("        Some(x) => x,\n        Shape::Unit => 1,\n        _ => 0,\n"),
        [row(false, IDENT, 1, true)]
    );
}

#[test]
fn literals_do_not_make_the_match_enum_like() {
    assert_eq!(
        arms("        1 => 10,\n        2 => 20,\n        _ => 0,\n"),
        [row(false, IDENT, 0, false)]
    );
}

#[test]
fn guards_do_not_make_the_match_enum_like() {
    assert_eq!(
        arms("        x if x > 1 => 1,\n        _ => 0,\n"),
        [row(false, IDENT, 0, false)]
    );
    assert_eq!(
        arms("        _ if ready() => 1,\n        _ => 0,\n"),
        [row(false, IDENT, 0, false)]
    );
}

#[test]
fn guarded_variant_still_counts_toward_enum_like() {
    assert_eq!(
        arms(
            "        Status::Active if flag() => 1,\n        Status::Idle => 2,\n        _ => 0,\n"
        ),
        [row(true, IDENT, 2, false)],
        "guarded sibling pattern names its variant"
    );
}

#[test]
fn guard_condition_paths_do_not_count_as_variants() {
    assert_eq!(
        arms("        n if n == Foo::BAR => 1,\n        _ => 0,\n"),
        [row(false, IDENT, 0, false)],
        "guard condition must stay excluded from the variant set"
    );
}

#[test]
fn comment_before_guard_still_counts_variant() {
    assert_eq!(
        arms(
            "        Shape::Circle(r) /* large only */ if r > 1.0 => 1,\n        Shape::Square(s) => 2,\n        _ => 0,\n"
        ),
        [row(true, IDENT, 2, false)],
        "comment between pattern and guard is not a second pattern"
    );
}

#[test]
fn scrutinee_shape_is_reported() {
    let body = "        Status::Active => 1,\n        Status::Idle => 2,\n        _ => 0,\n";
    assert_eq!(arms(body), [row(true, ScrutineeKind::Ident, 2, false)]);
    assert_eq!(
        arms_in("fn f(s: Wrapper) -> u8", "s.status", body),
        [row(true, ScrutineeKind::Field, 2, false)]
    );
    assert_eq!(
        arms_in("fn f() -> u8", "Status::Active", body),
        [row(true, ScrutineeKind::Path, 2, false)]
    );
    assert_eq!(
        arms_in("fn f() -> u8", "compute()", body),
        [row(true, ScrutineeKind::Other, 2, false)]
    );
}

#[test]
fn scoped_constants_do_not_make_the_match_enum_like() {
    assert_eq!(
        arms("        i32::MIN => 1,\n        i32::MAX => 2,\n        _ => 0,\n"),
        [row(false, IDENT, 0, false)],
        "all-caps scoped paths are associated constants, not variants"
    );
}

#[test]
fn mixed_variant_and_constant_counts_only_the_variant() {
    assert_eq!(
        arms("        Status::Active => 1,\n        Status::MAX => 2,\n        _ => 0,\n"),
        [row(false, IDENT, 1, false)],
        "the constant sibling drops out so one variant is an extract"
    );
}

#[test]
fn variants_from_two_types_are_not_enum_like() {
    assert_eq!(
        arms("        Event::Click(_) => 1,\n        Command::Quit => 2,\n        _ => 0,\n"),
        [row(false, IDENT, 1, false)],
        "sibling variants must share one type prefix to read as enum-like"
    );
    assert_eq!(
        arms(
            "        Event::Click => 1,\n        Event::Idle => 2,\n        Command::Quit => 3,\n        _ => 0,\n"
        ),
        [row(true, IDENT, 2, false)],
        "the widest single-type group decides"
    );
}

#[test]
fn bare_uppercase_identifiers_count_as_variants() {
    let source = "fn f(s: Status) -> u8 {\n    use Status::*;\n    match s {\n        Active => 1,\n        Idle => 2,\n        _ => 0,\n    }\n}\n";
    assert_eq!(
        arms_of(source),
        [row(true, IDENT, 2, false)],
        "uppercase bare siblings name two variants of one import"
    );
    let source =
        "fn f(s: Status) -> u8 {\n    match s {\n        other => 1,\n        _ => 0,\n    }\n}\n";
    assert_eq!(
        arms_of(source),
        [row(false, IDENT, 0, false)],
        "a lowercase bare sibling stays a binding, not a variant"
    );
    let source =
        "fn f(s: Status) -> u8 {\n    match s {\n        MAX => 1,\n        _ => 0,\n    }\n}\n";
    assert_eq!(
        arms_of(source),
        [row(false, IDENT, 0, false)],
        "an all-caps bare sibling stays a constant, not a variant"
    );
}
