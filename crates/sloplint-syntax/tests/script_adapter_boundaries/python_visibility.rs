//! Python function-head facts: visibility, forced signatures, entry points,
//! overload stubs, deprecation, decoration, trait items, and test marking.

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"from typing import Protocol, overload


def top():
    return 1


def _hidden():
    return 2


def main():
    return top()


class Service(Protocol):
    def run(self):
        return 1


class Plain:
    def method(self):
        return 2

    def __len__(self):
        return 0


@overload
def pick(first: int) -> int: ...


@overload
def pick(first: str) -> str: ...


def pick(first):
    return first
"#;

const EXTRA: &str = r#"import functools


@functools.lru_cache
def cached(x):
    return x


def test_parse():
    assert True
"#;

fn facts_of(source: &str, path: &str) -> Vec<(String, Visibility, bool, bool, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "parses");
    analysis
        .facts
        .functions
        .iter()
        .map(|f| {
            (
                f.name.to_owned(),
                f.visibility,
                f.signature_forced,
                f.is_overload,
                f.in_test,
            )
        })
        .collect()
}

fn row(
    rows: &[(String, Visibility, bool, bool, bool)],
    name: &str,
) -> (Visibility, bool, bool, bool) {
    rows.iter()
        .find(|r| r.0 == name)
        .map(|r| (r.1, r.2, r.3, r.4))
        .unwrap_or_else(|| panic!("has function {name}"))
}

#[test]
fn module_level_public_function_is_exported() {
    let rows = facts_of(SOURCE, "case.py");
    assert_eq!(
        row(&rows, "top"),
        (Visibility::Exported, false, false, false)
    );
}

#[test]
fn underscore_function_is_private() {
    let rows = facts_of(SOURCE, "case.py");
    assert_eq!(
        row(&rows, "_hidden"),
        (Visibility::Private, false, false, false)
    );
}

#[test]
fn main_is_an_entry_point() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source: SOURCE,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    let found = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("has main");
    assert!(found.entry_point);
    assert_eq!(found.visibility, Visibility::Exported);
}

#[test]
fn dunder_method_is_forced_but_visible() {
    let rows = facts_of(SOURCE, "case.py");
    assert_eq!(
        row(&rows, "__len__"),
        (Visibility::Exported, true, false, false)
    );
}

#[test]
fn overload_stubs_are_marked() {
    let rows = facts_of(SOURCE, "case.py");
    let stubs = rows.iter().filter(|r| r.0 == "pick").collect::<Vec<_>>();
    assert_eq!(stubs.len(), 3);
    assert!(stubs.iter().take(2).all(|r| r.3));
}

#[test]
fn protocol_method_is_a_trait_item() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source: SOURCE,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    let run = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "run")
        .expect("has run");
    assert!(run.is_trait_item);
    let method = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "method")
        .expect("has method");
    assert!(!method.is_trait_item);
}

#[test]
fn decorated_function_is_marked_decorated() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source: EXTRA,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    let cached = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "cached")
        .expect("has cached");
    assert!(cached.is_decorated);
    assert!(!cached.is_overload);
    assert!(!cached.is_deprecated);
}

#[test]
fn test_function_in_test_file_is_marked() {
    let rows = facts_of(EXTRA, "tests/test_parse.py");
    assert_eq!(
        row(&rows, "test_parse"),
        (Visibility::Exported, false, false, true)
    );
    let rows = facts_of(EXTRA, "case.py");
    assert_eq!(
        row(&rows, "test_parse"),
        (Visibility::Exported, false, false, false)
    );
}
