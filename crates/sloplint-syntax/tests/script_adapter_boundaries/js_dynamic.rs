//! JavaScript dynamic symbols: closed const tables and literal loops stay
//! silent while call results, reassigned lets, and open branches still flag.

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

const TERNARY_TABLE: &str = r#"export function pick(pick, chalk) {
  const typec = pick === 'a' ? 'red' : pick === 'b' ? 'yellow' : 'cyan'
  return chalk[typec](pick)
}
"#;

const OPEN_TERNARY: &str = r#"export function pick(pick, chalk, other) {
  const typec = pick === 'a' ? 'red' : other
  return chalk[typec](pick)
}
"#;

const TABLE_LOOP: &str = r#"const checks = [
  { cmd: 'checkPing' },
  { cmd: 'getLatestNpmVersion' },
]

export function run(target) {
  for (const cmd of checks) {
    target[cmd]()
  }
}
"#;

const KEYS_LOOP: &str = r#"const table = { alpha: 1, beta: 2 }

export function run(target) {
  for (const name of Object.keys(table)) {
    target[name]()
  }
}
"#;

const CALL_LOOP: &str = r#"export function run(target, names) {
  for (const a of names()) {
    target[a]()
  }
}
"#;

const REASSIGNED_LET: &str = r#"export function run(target) {
  let key = 'a'
  key = 'b'
  return target[key]()
}
"#;

const CONST_IMPORT: &str = r#"const mod = './ping.js'

export async function load() {
  return import(mod)
}
"#;

const OPEN_IMPORT: &str = r#"export async function load(name) {
  return import(name)
}
"#;

#[test]
fn const_ternary_chain_stays_silent() {
    assert_eq!(dynamics(TERNARY_TABLE), Vec::<String>::new());
}

#[test]
fn ternary_with_open_branch_still_flags() {
    assert_eq!(dynamics(OPEN_TERNARY).len(), 1);
}

#[test]
fn loop_over_const_table_stays_silent() {
    assert_eq!(dynamics(TABLE_LOOP), Vec::<String>::new());
}

#[test]
fn loop_over_const_object_keys_stays_silent() {
    assert_eq!(dynamics(KEYS_LOOP), Vec::<String>::new());
}

#[test]
fn loop_over_call_result_still_flags() {
    assert_eq!(dynamics(CALL_LOOP).len(), 1);
}

#[test]
fn reassigned_let_still_flags() {
    assert_eq!(dynamics(REASSIGNED_LET).len(), 1);
}

#[test]
fn const_module_import_stays_silent() {
    assert_eq!(dynamics(CONST_IMPORT), Vec::<String>::new());
}

#[test]
fn open_import_still_flags() {
    assert_eq!(dynamics(OPEN_IMPORT).len(), 1);
}

const DESTRUCTURED_TABLE_LOOP: &str = r#"const checks = [
  { title: 'one', cmd: 'a' },
  { title: 'two', cmd: 'b' },
]

export function run(target) {
  for (const { title, cmd } of checks) {
    target[cmd]()
  }
}
"#;

const TS_TABLE_LOOP: &str = r#"const checks = ['a', 'b']

export function run(target: Record<string, () => void>): void {
  for (const cmd of checks) {
    target[cmd]()
  }
}
"#;

const TS_OPEN_TERNARY: &str = r#"const chalk = { red: String }

export function paint(mode: number, other: string): string {
  const typec = mode === 1 ? 'red' : other
  return chalk[typec](mode)
}
"#;

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

#[test]
fn destructured_table_loop_stays_silent() {
    assert_eq!(dynamics(DESTRUCTURED_TABLE_LOOP), Vec::<String>::new());
}

#[test]
fn typescript_table_loop_stays_silent() {
    assert_eq!(
        typed(TS_TABLE_LOOP, Lang::TypeScript, "case.ts"),
        Vec::<String>::new()
    );
}

#[test]
fn typescript_open_ternary_still_flags() {
    assert_eq!(typed(TS_OPEN_TERNARY, Lang::TypeScript, "case.ts").len(), 1);
}
