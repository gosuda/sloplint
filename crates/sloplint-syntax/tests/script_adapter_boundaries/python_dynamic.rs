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

const METHOD_EVAL: &str = r#"def score(model, df):
    model.eval()
    return df.eval("a + b")
"#;

const BARE_EXEC: &str = r#"def run(code):
    exec(code)
"#;

const DICT_LOOP: &str = r#"def load(mod):
    for k in {"a": 1, "b": 2}:
        getattr(mod, k)
"#;

const COMMENT_TUPLE: &str =
    "def load(mod):\n    for n in (\"a\",  # first\n        \"b\"):\n        getattr(mod, n)\n";

const IDENT_LOOP: &str = r#"def load(mod, mapping):
    for k in mapping:
        getattr(mod, k)
"#;

#[test]
fn method_eval_stays_silent() {
    assert_eq!(
        dynamics(METHOD_EVAL),
        Vec::<String>::new(),
        "model.eval is a method, not a builtin"
    );
}

#[test]
fn bare_exec_still_flags() {
    assert_eq!(
        dynamics(BARE_EXEC),
        vec!["exec".to_owned()],
        "bare exec keeps its symbol"
    );
}

#[test]
fn dict_key_loop_stays_silent() {
    assert_eq!(
        dynamics(DICT_LOOP),
        Vec::<String>::new(),
        "string dict keys count as literal"
    );
}

#[test]
fn commented_tuple_loop_stays_silent() {
    assert_eq!(
        dynamics(COMMENT_TUPLE),
        Vec::<String>::new(),
        "comments are not tuple members"
    );
}

#[test]
fn identifier_loop_still_flags() {
    assert_eq!(
        dynamics(IDENT_LOOP).len(),
        1,
        "non-literal collection keeps the symbol"
    );
}

#[test]
fn alternating_outer_and_nested_getattr_flags_both_sites() {
    let reps = 2000usize;
    let mut source = String::from("def F(o, x):\n");
    for i in 0..reps {
        source.push_str(&format!(
            "    getattr(o, x)\n    def g{i}():\n        getattr(o, x)\n"
        ));
    }
    let found = dynamics(&source);
    assert_eq!(
        found.len(),
        2 * reps,
        "each outer and each nested getattr flags once with no literal targets"
    );
    assert!(
        found.iter().all(|w| w == "getattr with computed name"),
        "every flag names the computed getattr"
    );
}
