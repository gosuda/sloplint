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

const CONST_TERNARY: &str = r#"export function run(obj, type) {
  const typec = type === 'major' ? 'red' : type === 'minor' ? 'yellow' : 'cyan';
  return obj[typec](type);
}
"#;

const CONST_PLAIN: &str = r#"export function run(obj) {
  const kind = 'render';
  return obj[kind]();
}
"#;

const TS_CONST_TERNARY: &str = r#"export function run(obj: Record<string, (kind: string) => void>, type: string): void {
  const typec = type === 'major' ? 'red' : 'cyan';
  obj[typec](type);
}
"#;

const MIXED_BRANCH: &str = r#"export function run(obj, pick, other) {
  const kind = pick ? 'render' : other;
  return obj[kind]();
}
"#;

const LET_TERNARY: &str = r#"export function run(obj, type) {
  let typec = type === 'major' ? 'red' : 'cyan';
  return obj[typec](type);
}
"#;

const PARAM_SHADOW: &str = r#"const kind = 'render';
export function run(obj, kind) {
  return obj[kind]();
}
"#;

const SIBLING_BLOCK: &str = r#"export function run(obj) {
  {
    const kind = 'render';
  }
  return obj[kind]();
}
"#;

const INNER_CONST_SHADOW: &str = r#"export function run(obj, get) {
  for (const key of ['a', 'b']) {
    const key = get();
    obj[key]();
  }
}
"#;

#[test]
fn const_ternary_stays_silent() {
    assert_eq!(dynamics(CONST_TERNARY), Vec::<String>::new());
}

#[test]
fn const_plain_string_stays_silent() {
    assert_eq!(dynamics(CONST_PLAIN), Vec::<String>::new());
}

#[test]
fn typescript_const_ternary_stays_silent() {
    assert_eq!(
        typed(TS_CONST_TERNARY, Lang::TypeScript, "case.ts"),
        Vec::<String>::new()
    );
}

#[test]
fn mixed_branch_still_flags() {
    assert_eq!(dynamics(MIXED_BRANCH).len(), 1);
}

#[test]
fn let_ternary_still_flags() {
    assert_eq!(dynamics(LET_TERNARY).len(), 1);
}

#[test]
fn shadowing_parameter_still_flags() {
    assert_eq!(dynamics(PARAM_SHADOW).len(), 1);
}

#[test]
fn sibling_block_const_still_flags() {
    assert_eq!(dynamics(SIBLING_BLOCK).len(), 1);
}

#[test]
fn inner_const_shadow_still_flags() {
    assert_eq!(dynamics(INNER_CONST_SHADOW).len(), 1);
}
