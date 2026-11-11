//! Python boolean parameter defaults are `True`/`False` literals whose node
//! kinds are `true`/`false`; matching the default-value text instead would miss
//! every Python boolean default.

use sloplint_core::facts::PrimitiveKind;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"def f(flag=True):
    return flag


def g(x: bool = False):
    return x


def h(label="x", limit=10, other=True):
    return label
"#;

fn boolean_defaults(function: &str) -> Vec<(String, bool, bool, PrimitiveKind)> {
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
                p.has_default,
                p.bool_default,
                p.primitive_kind,
            )
        })
        .collect()
}

fn flags(items: &[(&str, bool, bool, PrimitiveKind)]) -> Vec<(String, bool, bool, PrimitiveKind)> {
    items
        .iter()
        .map(|(name, has, flag, prim)| ((*name).to_owned(), *has, *flag, *prim))
        .collect()
}

#[test]
fn untyped_boolean_defaults_are_flags() {
    assert_eq!(
        boolean_defaults("f"),
        flags(&[("flag", true, true, PrimitiveKind::Other)])
    );
}

#[test]
fn typed_boolean_defaults_are_flags() {
    assert_eq!(
        boolean_defaults("g"),
        flags(&[("x", true, true, PrimitiveKind::Bool)])
    );
}

#[test]
fn non_boolean_defaults_are_not_flags() {
    assert_eq!(
        boolean_defaults("h"),
        flags(&[
            ("label", true, false, PrimitiveKind::Other),
            ("limit", true, false, PrimitiveKind::Other),
            ("other", true, true, PrimitiveKind::Other),
        ])
    );
}
