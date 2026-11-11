//! JavaScript dynamic symbols: only an inline array const loop stays silent.

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn dynamics(source: &str) -> Vec<String> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::JavaScript,
        path: "case.js",
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

fn typed(source: &str, lang: Lang, path: &str) -> Vec<String> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
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

const INLINE_LOOP: &str = r#"export function run(obj) {
  for (const key of ['a', 'b']) {
    obj[key]()
  }
}
"#;

const TS_INLINE_LOOP: &str = r#"export function run(obj: Record<string, () => void>): void {
  for (const key of ['a', 'b']) {
    obj[key]()
  }
}
"#;

const AFTER_LOOP: &str = r#"export function run(obj) {
  for (const key of ['a', 'b']) {
    obj[key]()
  }
  return obj[key]()
}
"#;

const LET_HEAD: &str = r#"export function run(obj) {
  for (let key of ['a', 'b']) {
    obj[key]()
  }
}
"#;

const VAR_HEAD: &str = r#"export function run(obj) {
  for (var key of ['a', 'b']) {
    obj[key]()
  }
}
"#;

const NAMED_TABLE: &str = r#"const keys = ['a', 'b']
export function run(obj) {
  for (const key of keys) {
    obj[key]()
  }
}
"#;

const CALL_RESULT: &str = r#"export function run(obj, names) {
  for (const key of names()) {
    obj[key]()
  }
}
"#;

const COMPOSED_KEYS: &str = r#"export function run(obj, key) {
  obj[key + 'Handler']()
  obj[`${key}`]()
}
"#;

const NON_STRING_ELEMENT: &str = r#"export function run(obj) {
  for (const key of ['a', 1]) {
    obj[key]()
  }
}
"#;

const SHADOWED_DECL: &str = r#"export function run(obj, getName) {
  for (const name of ['fixed']) {
    {
      const name = getName()
      obj[name]()
    }
  }
}
"#;

const TS_SHADOWED_DECL: &str = r#"export function run(obj: Record<string, () => void>, getName: () => string): void {
  for (const name of ['fixed']) {
    {
      const name = getName()
      obj[name]()
    }
  }
}
"#;

const SHADOWED_FUNC_DECL: &str = r#"export function run(obj) {
  for (const name of ['a', 'b']) {
    function name() {}
    obj[name]()
  }
}
"#;

const UNRELATED_DECL: &str = r#"export function run(obj) {
  for (const name of ['a', 'b']) {
    const other = 1
    obj[name]()
  }
}
"#;

const ESCAPED_DECL: &str = r#"export function run(obj, getName) {
  for (const name of ['a']) {
    {
      const n\u0061me = getName()
      obj[name]()
    }
  }
}
"#;

#[test]
fn inline_array_const_loop_stays_silent() {
    assert_eq!(dynamics(INLINE_LOOP), Vec::<String>::new());
}

#[test]
fn typescript_inline_array_const_loop_stays_silent() {
    assert_eq!(
        typed(TS_INLINE_LOOP, Lang::TypeScript, "case.ts"),
        Vec::<String>::new()
    );
}

#[test]
fn lookup_after_loop_still_flags() {
    assert_eq!(dynamics(AFTER_LOOP).len(), 1);
}

#[test]
fn let_head_still_flags() {
    assert_eq!(dynamics(LET_HEAD).len(), 1);
}

#[test]
fn var_head_still_flags() {
    assert_eq!(dynamics(VAR_HEAD).len(), 1);
}

#[test]
fn named_const_table_still_flags() {
    assert_eq!(dynamics(NAMED_TABLE).len(), 1);
}

#[test]
fn loop_over_call_result_still_flags() {
    assert_eq!(dynamics(CALL_RESULT).len(), 1);
}

#[test]
fn composed_keys_still_flag() {
    assert_eq!(dynamics(COMPOSED_KEYS).len(), 2);
}

#[test]
fn non_string_element_still_flags() {
    assert_eq!(dynamics(NON_STRING_ELEMENT).len(), 1);
}

#[test]
fn many_lookups_complete() {
    let mut source =
        String::from("export function run(obj) {\n  for (const key of ['a', 'b']) {\n");
    for _ in 0..300 {
        source.push_str("    obj[key]()\n");
    }
    source.push_str("  }\n}\n");
    assert_eq!(dynamics(&source), Vec::<String>::new());
}

#[test]
fn shadowed_local_declaration_still_flags() {
    assert_eq!(dynamics(SHADOWED_DECL).len(), 1);
    assert_eq!(
        typed(TS_SHADOWED_DECL, Lang::TypeScript, "case.ts").len(),
        1
    );
}

#[test]
fn shadowed_function_declaration_still_flags() {
    assert_eq!(dynamics(SHADOWED_FUNC_DECL).len(), 1);
}

#[test]
fn unrelated_local_declaration_stays_silent() {
    assert_eq!(dynamics(UNRELATED_DECL), Vec::<String>::new());
}

#[test]
fn escaped_local_declaration_still_flags() {
    assert_eq!(dynamics(ESCAPED_DECL).len(), 1);
}

const ESCAPED_LOOKUP: &str = "function load(obj) {\n  for (const n\\u0061me of ['a', 'b']) {\n    { const name = pick(); obj[n\\u0061me](); }\n  }\n}\n";

#[test]
fn escaped_lookup_name_still_flags() {
    assert_eq!(dynamics(ESCAPED_LOOKUP).len(), 1);
}

const WITH_IN_BODY: &str = "function load(obj, scope) {\n  for (const name of ['a', 'b']) {\n    with (scope) { obj[name](); }\n  }\n}\n";

#[test]
fn with_statement_in_literal_loop_still_flags() {
    assert_eq!(dynamics(WITH_IN_BODY).len(), 1);
}

const PAREN_COMPUTED_CALLEE: &str = r#"export function run(handlers, payload, name) {
  (handlers[name])(payload);
}
"#;

#[test]
fn parenthesized_computed_callee_still_flags() {
    assert_eq!(dynamics(PAREN_COMPUTED_CALLEE).len(), 1);
}

const PAREN_LITERAL_KEY: &str = r#"export function run(handlers, payload) {
  (handlers["save"])(payload);
}
"#;

#[test]
fn parenthesized_literal_key_stays_silent() {
    assert_eq!(dynamics(PAREN_LITERAL_KEY), Vec::<String>::new());
}

const PAREN_LITERAL_INDEX: &str = r#"export function run(handlers, payload) {
  handlers[("save")](payload);
}
"#;

#[test]
fn parenthesized_literal_index_stays_silent() {
    assert_eq!(dynamics(PAREN_LITERAL_INDEX), Vec::<String>::new());
}

const PAREN_PLAIN_CALLEE: &str = r#"export function run(fn, payload) {
  (fn)(payload);
}
"#;

#[test]
fn parenthesized_plain_callee_stays_silent() {
    assert_eq!(dynamics(PAREN_PLAIN_CALLEE), Vec::<String>::new());
}

const GLOBAL_FUNCTION_CTOR: &str = r#"export function build(source) {
  return new globalThis.Function(source);
}
"#;

#[test]
fn global_this_function_ctor_still_flags() {
    assert_eq!(dynamics(GLOBAL_FUNCTION_CTOR).len(), 1);
}

const WINDOW_FUNCTION_CTOR: &str = r#"export function build(source) {
  return new window.Function(source);
}
"#;

#[test]
fn window_function_ctor_still_flags() {
    assert_eq!(dynamics(WINDOW_FUNCTION_CTOR).len(), 1);
}

const PAREN_HOST_FUNCTION_CTOR: &str = r#"export function build(source) {
  return new (globalThis).Function(source);
}
"#;

#[test]
fn parenthesized_host_function_ctor_still_flags() {
    assert_eq!(dynamics(PAREN_HOST_FUNCTION_CTOR).len(), 1);
}

const TS_CAST_HOST_FUNCTION_CTOR: &str = r#"export function build(source: string): unknown {
  return new (globalThis as any).Function(source);
}
"#;

#[test]
fn typescript_cast_host_function_ctor_still_flags() {
    assert_eq!(
        typed(TS_CAST_HOST_FUNCTION_CTOR, Lang::TypeScript, "case.ts").len(),
        1
    );
}

const OWN_FUNCTION_CTOR: &str = r#"export function build(source, my) {
  return new my.Function(source);
}
"#;

#[test]
fn own_namespace_function_ctor_stays_silent() {
    assert_eq!(dynamics(OWN_FUNCTION_CTOR), Vec::<String>::new());
}

const TS_NON_NULL_CALLEE: &str = r#"export function run(handlers: Record<string, (n: number) => void>, name: string): void {
  handlers[name]!(1);
}
"#;

#[test]
fn typescript_non_null_callee_still_flags() {
    assert_eq!(
        typed(TS_NON_NULL_CALLEE, Lang::TypeScript, "case.ts").len(),
        1
    );
}

const TS_AS_CALLEE: &str = r#"export function run(handlers: Record<string, (n: number) => void>, name: string): void {
  (handlers[name] as (n: number) => void)(1);
}
"#;

#[test]
fn typescript_as_callee_still_flags() {
    assert_eq!(typed(TS_AS_CALLEE, Lang::TypeScript, "case.ts").len(), 1);
}

const TS_SATISFIES_CALLEE: &str = r#"export function run(handlers: Record<string, (n: number) => void>, name: string): void {
  (handlers[name] satisfies (n: number) => void)(1);
}
"#;

#[test]
fn typescript_satisfies_callee_still_flags() {
    assert_eq!(
        typed(TS_SATISFIES_CALLEE, Lang::TypeScript, "case.ts").len(),
        1
    );
}

const TS_PAREN_LITERAL_KEY: &str = r#"export function run(handlers: Record<string, (n: number) => void>): void {
  (handlers["save"])(1);
}
"#;

#[test]
fn typescript_parenthesized_literal_key_stays_silent() {
    assert_eq!(
        typed(TS_PAREN_LITERAL_KEY, Lang::TypeScript, "case.ts"),
        Vec::<String>::new()
    );
}

const TS_CONST_LIST_LOOP: &str = r#"export function run(handlers: Record<string, () => void>): void {
  for (const key of ["open", "close"] as const) {
    handlers[key]();
  }
}
"#;

#[test]
fn typescript_const_asserted_list_loop_stays_silent() {
    assert_eq!(
        typed(TS_CONST_LIST_LOOP, Lang::TypeScript, "case.ts"),
        Vec::<String>::new()
    );
}

const INDIRECT_EVAL: &str = r#"export function run(source) {
  return (0, eval)(source);
}
"#;

#[test]
fn indirect_eval_still_flags() {
    assert_eq!(dynamics(INDIRECT_EVAL), ["eval"]);
}

const HOST_EVAL: &str = r#"export function run(source) {
  return globalThis.eval(source);
}
"#;

#[test]
fn host_qualified_eval_still_flags() {
    assert_eq!(dynamics(HOST_EVAL), ["eval"]);
}

const OWN_EVAL: &str = r#"export function run(source, parser) {
  return (0, parser.eval)(source);
}
"#;

#[test]
fn own_namespace_eval_stays_silent() {
    assert_eq!(dynamics(OWN_EVAL), Vec::<String>::new());
}

const PAREN_STATIC_IMPORT: &str = r#"export async function load() {
  return import(("./a.js"));
}
"#;

#[test]
fn parenthesized_static_import_stays_silent() {
    assert_eq!(dynamics(PAREN_STATIC_IMPORT), Vec::<String>::new());
}

const PAREN_IMPORT_LOOP: &str = r#"export async function load() {
  for (const n of ['./a.js', './b.js']) {
    await import((n));
  }
}
"#;

#[test]
fn parenthesized_import_in_literal_loop_stays_silent() {
    assert_eq!(dynamics(PAREN_IMPORT_LOOP), Vec::<String>::new());
}

const PAREN_FREE_IMPORT: &str = r#"export async function load(name) {
  return import((name));
}
"#;

#[test]
fn parenthesized_free_import_still_flags() {
    assert_eq!(dynamics(PAREN_FREE_IMPORT), ["dynamic import"]);
}

const PLAIN_TEMPLATE_KEY: &str = r#"export function run(handlers, x) {
  handlers[`save`](x);
  return import(`./a.js`);
}
"#;

#[test]
fn plain_template_key_and_import_stay_silent() {
    assert_eq!(dynamics(PLAIN_TEMPLATE_KEY), Vec::<String>::new());
}

const SUBSTITUTED_TEMPLATE_KEY: &str = r#"export function run(handlers, x, name) {
  handlers[`on${name}`](x);
  return import(`./${name}.js`);
}
"#;

#[test]
fn substituted_template_key_and_import_still_flag() {
    assert_eq!(
        dynamics(SUBSTITUTED_TEMPLATE_KEY),
        ["computed method call", "dynamic import"]
    );
}

const TS_ANGLE_CAST_CALLEE: &str = r#"type Handler = (n: number) => void;
export function run(handlers: Record<string, Handler>, name: string): void {
  (<Handler>handlers[name])(1);
}
"#;

#[test]
fn typescript_angle_cast_callee_still_flags() {
    assert_eq!(
        typed(TS_ANGLE_CAST_CALLEE, Lang::TypeScript, "case.ts"),
        ["computed method call"]
    );
}

const TS_ANGLE_CAST_LITERAL_KEY: &str = r#"type Handler = (n: number) => void;
export function run(handlers: Record<string, Handler>): void {
  (<Handler>handlers["save"])(1);
}
"#;

#[test]
fn typescript_angle_cast_literal_key_stays_silent() {
    assert_eq!(
        typed(TS_ANGLE_CAST_LITERAL_KEY, Lang::TypeScript, "case.ts"),
        Vec::<String>::new()
    );
}

const PAREN_FUNCTION_CTOR: &str = r#"export function build(source) {
  return new (window.Function)(source);
}
"#;

#[test]
fn parenthesized_function_ctor_still_flags() {
    assert_eq!(dynamics(PAREN_FUNCTION_CTOR), ["Function constructor"]);
}

const PAREN_OWN_FUNCTION_CTOR: &str = r#"export function build(source, my) {
  return new (my.Function)(source);
}
"#;

#[test]
fn parenthesized_own_function_ctor_stays_silent() {
    assert_eq!(dynamics(PAREN_OWN_FUNCTION_CTOR), Vec::<String>::new());
}

fn deep_parens(inner: &str) -> String {
    format!("{}{inner}{}", "(".repeat(20), ")".repeat(20))
}

#[test]
fn deeply_wrapped_ctor_flags() {
    let source = format!(
        "export function build(source) {{\n  return new {}(source);\n}}\n",
        deep_parens("make")
    );
    assert_eq!(dynamics(&source), ["Function constructor"]);
}

#[test]
fn deeply_wrapped_ctor_host_flags() {
    let source = format!(
        "export function build(source) {{\n  return new {}.Function(source);\n}}\n",
        deep_parens("make")
    );
    assert_eq!(dynamics(&source), ["Function constructor"]);
}
