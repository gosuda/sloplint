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
fn field_name_walk_still_flags() {
    assert_eq!(dynamics(NAME_WALK).len(), 1);
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

const MODULE_TABLE_LOOP: &str = r#"import sys
for package in ("urllib3", "idna"):
    locals()[package] = __import__(package)
"#;

const PAIR_COMPREHENSION: &str = r#"import hashlib
MAP = {
    n: getattr(hashlib, a, None)
    for n, a in ((1, "md5"), (2, "sha1"))
}
"#;

const CALL_COMPREHENSION: &str = r#"import hashlib

def pairs():
    return ((1, "md5"),)

MAP = {n: getattr(hashlib, a, None) for n, a in pairs()}
"#;

const CLASS_ATTR_TABLE: &str = r#"class Response:
    __attrs__: list[str] = ["a", "b"]

    def state(self):
        return {attr: getattr(self, attr, None) for attr in self.__attrs__}
"#;

const CODES_TABLE: &str = r#"import types

_codes = {404: ("not_found",), 418: ("im_a_teapot",)}
codes = types.SimpleNamespace()

def init():
    for code, titles in _codes.items():
        for title in titles:
            setattr(codes, title, code)
            setattr(codes, title.upper(), code)
"#;

const DOUBLE_TABLE: &str = r#"import types

_codes = {404: ("not_found",)}
_codes = {}

def init():
    for code, titles in _codes.items():
        setattr(codes, title, code)
"#;

const SORTED_TABLE: &str = r#"import box as box

BOXES = ["A", "B"]

for name in sorted(BOXES):
    print(getattr(box, name))
"#;

const CONST_NAME: &str = r#"GIBBERISH = "aihwerij"

def probe(obj):
    return hasattr(obj, GIBBERISH)
"#;

const DOUBLE_CONST: &str = r#"GIBBERISH = "abc"
GIBBERISH = "def"

def probe(obj):
    return hasattr(obj, GIBBERISH)
"#;

#[test]
fn module_table_loop_stays_silent() {
    assert_eq!(dynamics(MODULE_TABLE_LOOP), Vec::<String>::new());
}

#[test]
fn literal_pair_comprehension_stays_silent() {
    assert_eq!(dynamics(PAIR_COMPREHENSION), Vec::<String>::new());
}

#[test]
fn comprehension_over_call_result_still_flags() {
    assert_eq!(dynamics(CALL_COMPREHENSION).len(), 1);
}

#[test]
fn class_attr_table_loop_stays_silent() {
    assert_eq!(dynamics(CLASS_ATTR_TABLE), Vec::<String>::new());
}

#[test]
fn dict_items_table_loop_stays_silent() {
    assert_eq!(dynamics(CODES_TABLE), Vec::<String>::new());
}

#[test]
fn table_assigned_twice_still_flags() {
    assert_eq!(dynamics(DOUBLE_TABLE).len(), 1);
}

#[test]
fn sorted_table_loop_stays_silent() {
    assert_eq!(dynamics(SORTED_TABLE), Vec::<String>::new());
}

#[test]
fn const_string_hasattr_stays_silent() {
    assert_eq!(dynamics(CONST_NAME), Vec::<String>::new());
}

#[test]
fn const_assigned_twice_still_flags() {
    assert_eq!(dynamics(DOUBLE_CONST).len(), 1);
}

const MODULE_CALL_LOOP: &str = r#"def names():
    return ("a", "b")


for name in names():
    getattr(guest, name)
"#;

const FIELD_OF_LOOP_TARGET: &str = r#"for package in ("urllib3", "idna"):
    getattr(mod, package.owner)
"#;

const CALL_WITH_ARGS: &str = r#"for title in ("a", "b"):
    getattr(codes, title.replace("a", "b"))
"#;

#[test]
fn module_loop_over_call_result_still_flags() {
    assert_eq!(dynamics(MODULE_CALL_LOOP).len(), 1);
}

#[test]
fn attribute_field_of_loop_target_still_flags() {
    assert_eq!(dynamics(FIELD_OF_LOOP_TARGET).len(), 1);
}

#[test]
fn attribute_call_with_arguments_still_flags() {
    assert_eq!(dynamics(CALL_WITH_ARGS).len(), 1);
}
const PD01: &str = "def get(obj, attr):\n    return getattr(obj, f'get_{attr}')\n";
const PD02: &str = "name = 'default'\n\ndef fetch(obj, name):\n    return getattr(obj, name)\n";
const PD03: &str = "class Config:\n    fields = ('id', 'name')\n\nclass Serializer:\n    def dump(self, obj, fields):\n        return {f: getattr(obj, f) for f in fields}\n";
const PD04: &str = "FIELDS = ['id', 'name']\n\ndef register(extra):\n    FIELDS.append(extra)\n\ndef dump(obj):\n    for k in FIELDS:\n        getattr(obj, k)\n";
const PD05: &str = "MODE = 'fast'\n\ndef set_mode(value):\n    global MODE\n    MODE = value\n\ndef run(obj):\n    return getattr(obj, MODE)\n";
const PD06: &str =
    "import sys\nname = 'default'\nfor name in request_fields:\n    print(getattr(sys, name))\n";
const PD07: &str = "def probe(obj, rows):\n    for n in ('a', 'b'):\n        print(n)\n    n = rows[0]\n    return getattr(obj, n)\n";
const PD08: &str = "class Form:\n    fields = ('name', 'email')\n\n    def __init__(self, fields):\n        self.fields = fields\n\n    def dump(self, obj):\n        for f in self.fields:\n            getattr(obj, f)\n";
const PD13: &str = "CACHE = {}\n\ndef put(k, v):\n    CACHE[k] = v\n\ndef get(obj):\n    return getattr(obj, CACHE['x'])\n";
const PD10_SRC: &str =
    "def load(obj):\n    return getattr(obj,  # field\n                   'id')\n";
const PD12A: &str = "for name in ('a', 'b'):\n    for obj in objs:\n        getattr(obj, name)\n";
const PD12B: &str = "def outer(objs):\n    for name in ('a', 'b'):\n        def inner(o):\n            return getattr(o, name)\n";
#[test]
fn pd01_fstring_still_flags() {
    assert_eq!(dynamics(PD01).len(), 1);
}
#[test]
fn pd02_param_shadows_table_still_flags() {
    assert_eq!(dynamics(PD02).len(), 1);
}
#[test]
fn pd03_param_shadows_class_table_still_flags() {
    assert_eq!(dynamics(PD03).len(), 1);
}
#[test]
fn pd04_mutated_table_still_flags() {
    assert_eq!(dynamics(PD04).len(), 1);
}
#[test]
fn pd05_global_rebinding_still_flags() {
    assert_eq!(dynamics(PD05).len(), 1);
}
#[test]
fn pd06_for_rebinds_const_still_flags() {
    assert_eq!(dynamics(PD06).len(), 1);
}
#[test]
fn pd07_reassigned_loop_var_still_flags() {
    assert_eq!(dynamics(PD07).len(), 1);
}
#[test]
fn pd08_instance_store_still_flags() {
    assert_eq!(dynamics(PD08).len(), 1);
}
#[test]
fn pd13_empty_dict_still_flags() {
    assert_eq!(dynamics(PD13).len(), 1);
}
#[test]
fn pd10_comment_in_args_stays_silent() {
    assert_eq!(dynamics(PD10_SRC), Vec::<String>::new());
}
#[test]
fn pd12_nested_loops_stay_silent() {
    assert_eq!(dynamics(PD12A), Vec::<String>::new());
}
#[test]
fn pd12_closure_over_loop_stays_silent() {
    assert_eq!(dynamics(PD12B), Vec::<String>::new());
}
#[test]
fn pd09_many_constants_complete() {
    let mut source = String::new();
    for i in 0..10000 {
        source.push_str(&format!("C{i} = {i}\n"));
    }
    source.push_str("def f(o, x):\n    return getattr(o, x)\n");
    assert_eq!(dynamics(&source).len(), 1);
}
