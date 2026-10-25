//! A top-level function or arrow-function binding is public when the file exports its name
//! later, with `export { name }`, `export { name as other }`, or `export default name`, the
//! same as when the declaration itself carries `export`. Members of a class exported by
//! name are public too. Re-exports from another module, and same-named functions or classes
//! in nested scopes, do not make anything local public.

use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

type Row = (String, bool);

fn visibility(lang: Lang, path: &str, source: &str) -> Vec<Row> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "{path} parses, first syntax error at {:?}",
        analysis.parse_error
    );
    let mut rows: Vec<Row> = analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name.to_string(), f.public))
        .collect();
    rows.sort();
    rows
}

fn row(name: &str, public: bool) -> Row {
    (name.to_string(), public)
}

fn expected() -> Vec<Row> {
    vec![
        row("hidden", false),
        row("late", true),
        row("main", true),
        row("other", false),
        row("renamed", true),
        row("run", false),
        row("run", true),
        row("wire", true),
    ]
}

const TYPESCRIPT: &str = "export { late };
function run(job: string, dryRun: boolean): void {}
const wire = (a: string): string => a;
function hidden(): void {
  function run(): void {}
}
function renamed(): void {}
function main(): void {}
function other(): void {}
function late(): void {}
export { run, wire, renamed as published };
export { other } from \"./elsewhere\";
export default main;
";

const JAVASCRIPT: &str = "export { late };
function run(job, dryRun) {}
const wire = (a) => a;
function hidden() {
  function run() {}
}
function renamed() {}
function main() {}
function other() {}
function late() {}
export { run, wire, renamed as published };
export { other } from \"./elsewhere\";
export default main;
";

#[test]
fn typescript_exports_made_by_name_are_public() {
    assert_eq!(
        visibility(Lang::TypeScript, "lib.ts", TYPESCRIPT),
        expected()
    );
}

#[test]
fn javascript_exports_made_by_name_are_public() {
    assert_eq!(
        visibility(Lang::JavaScript, "lib.js", JAVASCRIPT),
        expected()
    );
}

const TYPESCRIPT_CLASSES: &str = "class Service {
  run(): void {}
  private hidden(): void {}
}
function make(): void {
  class Service {
    inner(): void {}
  }
}
class Remote {
  call(): void {}
}
export { Service };
export { Remote } from \"./remote\";
";

const JAVASCRIPT_CLASSES: &str = "class Service {
  run() {}
}
function make() {
  class Service {
    inner() {}
  }
}
class Remote {
  call() {}
}
export { Service };
export { Remote } from \"./remote\";
";

fn class_members() -> Vec<Row> {
    vec![
        row("call", false),
        row("inner", false),
        row("make", false),
        row("run", true),
    ]
}

#[test]
fn typescript_members_of_a_locally_exported_class_are_public() {
    let mut expected = class_members();
    expected.insert(1, row("hidden", false));
    assert_eq!(
        visibility(Lang::TypeScript, "service.ts", TYPESCRIPT_CLASSES),
        expected,
        "`private` members, nested same-named classes, and `export .. from` stay non-public"
    );
}

#[test]
fn javascript_members_of_a_locally_exported_class_are_public() {
    assert_eq!(
        visibility(Lang::JavaScript, "service.js", JAVASCRIPT_CLASSES),
        class_members()
    );
}

#[test]
fn typescript_export_equals_makes_the_function_public() {
    let source = "function legacy(): void {}\nfunction local(): void {}\nexport = legacy;\n";
    assert_eq!(
        visibility(Lang::TypeScript, "legacy.ts", source),
        vec![row("legacy", true), row("local", false)]
    );
}
