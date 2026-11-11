//! Python parameter facts: keyword-only position, variadic splats, defaults,
//! and primitive kinds.

use sloplint_core::facts::PrimitiveKind;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"def f(a, *args, b, c: bool = False, **kw):
    return a


def g(x: bool = False, y: str = "s", z: int = 3):
    return x


def h(first, /):
    return first
"#;

fn params_of(function: &str) -> Vec<(String, bool, bool, bool, PrimitiveKind, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source: SOURCE,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.py parses");
    let found = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == function)
        .unwrap_or_else(|| panic!("case.py has function {function}"));
    found
        .params
        .iter()
        .map(|p| {
            (
                p.name.to_owned(),
                p.keyword_only,
                p.is_variadic,
                p.has_default,
                p.primitive_kind,
                p.bool_default,
            )
        })
        .collect()
}

#[test]
fn star_args_are_variadic() {
    let params = params_of("f");
    let args = params.iter().find(|p| p.0 == "args").expect("has args");
    assert!(args.2);
    assert!(!args.1);
}

#[test]
fn names_after_bare_star_are_keyword_only() {
    let params = params_of("f");
    let b = params.iter().find(|p| p.0 == "b").expect("has b");
    assert!(b.1);
    assert!(!b.3);
    let c = params.iter().find(|p| p.0 == "c").expect("has c");
    assert!(c.1);
    assert!(c.3);
    assert!(c.5);
    assert_eq!(c.4, PrimitiveKind::Bool);
}

#[test]
fn double_star_kwargs_are_variadic() {
    let params = params_of("f");
    let kw = params.iter().find(|p| p.0 == "kw").expect("has kw");
    assert!(kw.2);
}

#[test]
fn typed_defaults_report_primitive_kinds() {
    let params = params_of("g");
    let kinds = params
        .iter()
        .map(|p| (p.0.clone(), p.4))
        .collect::<Vec<_>>();
    assert!(kinds.contains(&("x".to_owned(), PrimitiveKind::Bool)));
    assert!(kinds.contains(&("y".to_owned(), PrimitiveKind::Str)));
    assert!(kinds.contains(&("z".to_owned(), PrimitiveKind::Int)));
}

#[test]
fn positional_only_marker_resets_keyword_only() {
    let params = params_of("h");
    let first = params.iter().find(|p| p.0 == "first").expect("has first");
    assert!(!first.1);
}
