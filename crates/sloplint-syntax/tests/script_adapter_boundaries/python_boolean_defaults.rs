//! Python boolean parameter defaults are `True`/`False` literals whose node
//! kinds are `true`/`false`; matching the default-value text instead would miss
//! every Python boolean default.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"def f(flag=True):
    return flag


def g(x: bool = False):
    return x


def h(label="x", limit=10, other=True):
    return label
"#;

fn boolean_defaults(function: &str) -> Vec<(String, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source: SOURCE,
        is_test_file: false,
        has_interface: false,
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
        .map(|p| (p.name.to_owned(), p.bool_default))
        .collect()
}

fn flags(items: &[(&str, bool)]) -> Vec<(String, bool)> {
    items
        .iter()
        .map(|(name, flag)| ((*name).to_owned(), *flag))
        .collect()
}

#[test]
fn untyped_boolean_defaults_are_flags() {
    assert_eq!(boolean_defaults("f"), flags(&[("flag", true)]));
}

#[test]
fn typed_boolean_defaults_are_flags() {
    assert_eq!(boolean_defaults("g"), flags(&[("x", true)]));
}

#[test]
fn non_boolean_defaults_are_not_flags() {
    assert_eq!(
        boolean_defaults("h"),
        flags(&[("label", false), ("limit", false), ("other", true)])
    );
}
