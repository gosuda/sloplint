//! Python dynamic symbols: proxy forwarding, literal loops, string modules,
//! and `.name` walks stay silent while parameter-driven names still flag.

use sloplint_core::facts::AmbientKind;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const PROXY: &str = r#"class Lazy:
    def __getattr__(self, name: str):
        return getattr(self._file, name)
"#;

const PARAM: &str = r#"def load(ssl, candidate):
    return getattr(ssl, candidate)
"#;

const LITERAL_LOOP: &str = r#"import importlib


def load(names):
    for lib in ("os", "sys"):
        importlib.import_module(lib)
"#;

const STRING_MODULE: &str = r#"import importlib


def load():
    return importlib.import_module("os")
"#;

const LITERAL_IMPORT: &str = r#"def show(content):
    return __import__("pydoc").pager(content)
"#;

const NAME_WALK: &str = r#"def value(obj, field):
    return getattr(obj, field.name)
"#;

const ENV_STORE: &str = r#"import os


def enable(key, val):
    os.environ[key] = val
    return os.environ.get(key)
"#;

fn dynamics(source: &str) -> Vec<String> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "parses");
    analysis
        .facts
        .dynamic_symbols
        .iter()
        .map(|d| d.what.to_owned())
        .collect()
}

fn ambient_kinds(source: &str) -> Vec<AmbientKind> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "parses");
    analysis.facts.ambient.iter().map(|a| a.kind).collect()
}

#[test]
fn proxy_forwarding_of_name_param_stays_silent() {
    assert_eq!(dynamics(PROXY), Vec::<String>::new());
}

#[test]
fn parameter_driven_getattr_still_flags() {
    assert_eq!(dynamics(PARAM).len(), 1);
}

#[test]
fn literal_loop_import_stays_silent() {
    assert_eq!(dynamics(LITERAL_LOOP), Vec::<String>::new());
}

#[test]
fn string_module_import_stays_silent() {
    assert_eq!(dynamics(STRING_MODULE), Vec::<String>::new());
}

#[test]
fn literal_dunder_import_stays_silent() {
    assert_eq!(dynamics(LITERAL_IMPORT), Vec::<String>::new());
}

#[test]
fn field_name_walk_stays_silent() {
    assert_eq!(dynamics(NAME_WALK), Vec::<String>::new());
}

#[test]
fn environ_store_is_a_global_write() {
    let kinds = ambient_kinds(ENV_STORE);
    assert!(kinds.contains(&AmbientKind::GlobalWrite));
}
