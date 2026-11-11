//! Anonymous callbacks are their own function: control flow inside a callback
//! scores on the callback, callback parameters shadow file-scope names, and
//! writes inside a callback carry that callback as their function.

use sloplint_core::facts::{AmbientKind, Visibility};
use sloplint_core::lang::Lang;

use crate::support::run_clean;

const NESTED: &str = "function outer() {\nitems.forEach((x) => {\nif (x) {\nfor (const y of x) {\nif (y) {\ny();\n}\n}\n}\n});\n}\n";

const NAMED_JS: &str = "const handler = (x) => {\nreturn x;\n};\n";
const NAMED_TS: &str = "const handler = (x: number): number => {\nreturn x;\n};\n";

const SHADOWED: &str =
    "let counter = 0;\nfunction f(xs) {\nxs.forEach((counter) => {\ncounter = 1;\n});\n}\n";
const CAPTURED: &str = "let counter = 0;\nxs.forEach(() => {\ncounter++;\n});\n";
const SHADOWED_TOP: &str = "let counter = 0;\nxs.forEach((counter) => {\ncounter++;\n});\n";

fn nested_scores_on_the_callback(lang: Lang, path: &str) {
    let analysis = run_clean(lang, path, NESTED);
    let functions = &analysis.facts.functions;
    assert_eq!(
        functions.len(),
        2,
        "{path}: outer plus one unnamed callback: {}",
        functions.len()
    );
    let outer = functions
        .iter()
        .find(|f| f.name == "outer")
        .unwrap_or_else(|| panic!("{path} keeps a function named outer"));
    assert_eq!(
        outer.cognitive, 0,
        "{path}: the callback nests, outer stays flat"
    );
    let callback = functions
        .iter()
        .find(|f| f.name.is_empty())
        .unwrap_or_else(|| panic!("{path} emits the callback with an empty name"));
    assert!(
        callback.cognitive > outer.cognitive,
        "{path}: the nested score lands on the callback: {}",
        callback.cognitive
    );
    assert!(
        callback.max_depth >= 2,
        "{path}: the callback keeps its own depth: {}",
        callback.max_depth
    );
    let params: Vec<&str> = callback.params.iter().map(|p| p.name).collect();
    assert_eq!(params, ["x"], "{path}: the callback keeps its parameter");
}

#[test]
fn js_nested_callback_scores_on_the_callback() {
    nested_scores_on_the_callback(Lang::JavaScript, "app.js");
}

#[test]
fn ts_nested_callback_scores_on_the_callback() {
    nested_scores_on_the_callback(Lang::TypeScript, "mod.ts");
}

fn named_handler_keeps_its_name(lang: Lang, path: &str, source: &str) {
    let analysis = run_clean(lang, path, source);
    let functions = &analysis.facts.functions;
    assert_eq!(
        functions.len(),
        1,
        "{path}: one bound closure, no extra fact: {}",
        functions.len()
    );
    assert_eq!(
        functions[0].name, "handler",
        "{path}: the bound closure keeps its name"
    );
}

#[test]
fn js_named_handler_is_unchanged() {
    named_handler_keeps_its_name(Lang::JavaScript, "app.js", NAMED_JS);
}

#[test]
fn ts_named_handler_is_unchanged() {
    named_handler_keeps_its_name(Lang::TypeScript, "mod.ts", NAMED_TS);
}

fn global_writes(lang: Lang, path: &str, source: &str) -> (usize, usize) {
    let analysis = run_clean(lang, path, source);
    let mut scoped = 0;
    let mut ambient = 0;
    for site in &analysis.facts.ambient {
        if site.kind == AmbientKind::GlobalWrite {
            if site.in_function.is_some() {
                scoped += 1;
            } else {
                ambient += 1;
            }
        }
    }
    (scoped, ambient)
}

fn shadowed_param_writes_nothing(lang: Lang, path: &str) {
    let analysis = run_clean(lang, path, SHADOWED);
    let callback = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name.is_empty())
        .unwrap_or_else(|| panic!("{path} emits the callback with an empty name"));
    let params: Vec<&str> = callback.params.iter().map(|p| p.name).collect();
    assert_eq!(
        params,
        ["counter"],
        "{path}: the callback takes the shadowing parameter"
    );
    let (scoped, ambient) = global_writes(lang, path, SHADOWED);
    assert_eq!(
        (scoped, ambient),
        (0, 0),
        "{path}: the parameter shadows the file-scope name"
    );
}

#[test]
fn js_callback_param_shadows_the_global() {
    shadowed_param_writes_nothing(Lang::JavaScript, "app.js");
}

#[test]
fn ts_callback_param_shadows_the_global() {
    shadowed_param_writes_nothing(Lang::TypeScript, "mod.ts");
}

fn captured_mutation_belongs_to_the_callback(lang: Lang, path: &str) {
    let (scoped, ambient) = global_writes(lang, path, CAPTURED);
    assert_eq!(
        (scoped, ambient),
        (1, 0),
        "{path}: the module-scope write lands in the callback"
    );
}

#[test]
fn js_captured_mutation_is_in_function() {
    captured_mutation_belongs_to_the_callback(Lang::JavaScript, "app.js");
}

#[test]
fn ts_captured_mutation_is_in_function() {
    captured_mutation_belongs_to_the_callback(Lang::TypeScript, "mod.ts");
}

const DECORATED_FIELD_JS: &str = "class Svc {\n@dec load = (id) => fetchOne(id);\n}\n";
const PLAIN_FIELD_JS: &str = "class Svc {\nload = (id) => fetchOne(id);\n}\n";
const DECORATED_FIELD_TS: &str =
    "class Svc {\n@Memoize() load = (id: string) => fetchOne(id);\n}\n";
const PLAIN_FIELD_TS: &str = "class Svc {\nload = (id: string) => fetchOne(id);\n}\n";

fn field_arrow(lang: Lang, path: &str, source: &str) -> bool {
    let analysis = run_clean(lang, path, source);
    let function = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "load")
        .unwrap_or_else(|| panic!("{path}: the field arrow keeps its name"));
    function.is_decorated
}

#[test]
fn js_decorated_field_arrow_is_decorated() {
    assert!(
        field_arrow(Lang::JavaScript, "app.js", DECORATED_FIELD_JS),
        "decorators sit on the field, not the arrow"
    );
}

#[test]
fn js_plain_field_arrow_is_not_decorated() {
    assert!(
        !field_arrow(Lang::JavaScript, "app.js", PLAIN_FIELD_JS),
        "the neighbour without a decorator stays plain"
    );
}

#[test]
fn ts_decorated_field_arrow_is_decorated() {
    assert!(
        field_arrow(Lang::TypeScript, "mod.ts", DECORATED_FIELD_TS),
        "decorators sit on the field, not the arrow"
    );
}

#[test]
fn ts_plain_field_arrow_is_not_decorated() {
    assert!(
        !field_arrow(Lang::TypeScript, "mod.ts", PLAIN_FIELD_TS),
        "the neighbour without a decorator stays plain"
    );
}

const NESTED_BOUND: &str = "function outer(xs) {\nconst check = (x) => {\nif (x) {\nif (x.a) {\nif (x.b) {\nreturn 1;\n}\n}\n}\n};\nxs.forEach(check);\n}\n";

fn nested_bound_closure_is_own_function(lang: Lang, path: &str) {
    let analysis = run_clean(lang, path, NESTED_BOUND);
    let functions = &analysis.facts.functions;
    assert_eq!(
        functions.len(),
        2,
        "{path}: a declarator-bound closure is its own function"
    );
    let outer = functions
        .iter()
        .find(|f| f.name == "outer")
        .unwrap_or_else(|| panic!("{path} keeps a function named outer"));
    let check = functions
        .iter()
        .find(|f| f.name == "check")
        .unwrap_or_else(|| panic!("{path} names the bound closure check"));
    assert_eq!(
        check.visibility,
        Visibility::Private,
        "{path}: a block-local binding stays private"
    );
    assert_eq!(
        outer.cognitive, 0,
        "{path}: the nested control flow no longer scores on outer"
    );
    assert!(
        check.cognitive > 0 && check.max_depth >= 2,
        "{path}: the nested control flow scores on the closure"
    );
}

#[test]
fn js_nested_bound_closure_is_own_function() {
    nested_bound_closure_is_own_function(Lang::JavaScript, "app.js");
}

#[test]
fn ts_nested_bound_closure_is_own_function() {
    nested_bound_closure_is_own_function(Lang::TypeScript, "mod.ts");
}

const BOUND_SHADOWED: &str = "let counter = 0;\nfunction f(xs) {\nconst bump = (counter) => {\ncounter = 1;\n};\nbump(xs);\n}\n";

fn bound_param_writes_nothing(lang: Lang, path: &str) {
    let (scoped, ambient) = global_writes(lang, path, BOUND_SHADOWED);
    assert_eq!(
        (scoped, ambient),
        (0, 0),
        "{path}: the bound closure parameter shadows the file-scope name"
    );
}

#[test]
fn js_bound_closure_param_shadows_the_global() {
    bound_param_writes_nothing(Lang::JavaScript, "app.js");
}

#[test]
fn ts_bound_closure_param_shadows_the_global() {
    bound_param_writes_nothing(Lang::TypeScript, "mod.ts");
}

fn top_param_writes_nothing(lang: Lang, path: &str) {
    let analysis = run_clean(lang, path, SHADOWED_TOP);
    let callback = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name.is_empty())
        .unwrap_or_else(|| panic!("{path} emits the callback with an empty name"));
    let params: Vec<&str> = callback.params.iter().map(|p| p.name).collect();
    assert_eq!(
        params,
        ["counter"],
        "{path}: the callback takes the shadowing parameter"
    );
    let (scoped, ambient) = global_writes(lang, path, SHADOWED_TOP);
    assert_eq!(
        (scoped, ambient),
        (0, 0),
        "{path}: the file-scope parameter shadows the file-scope name"
    );
}

#[test]
fn js_top_callback_param_shadows_the_global() {
    top_param_writes_nothing(Lang::JavaScript, "app.js");
}

#[test]
fn ts_top_callback_param_shadows_the_global() {
    top_param_writes_nothing(Lang::TypeScript, "mod.ts");
}
