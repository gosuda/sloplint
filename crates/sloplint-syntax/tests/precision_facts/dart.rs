use std::sync::LazyLock;

use sloplint_core::facts::{PrimitiveKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Dart,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn visibility_of<'a>(analysis: &'a Analysis<'a>, name: &str) -> Visibility {
    analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("has function {name}"))
        .visibility
}

#[test]
fn underscore_functions_are_private_and_top_level_are_exported() {
    let analysis = run("case.dart", "void top() {}\nvoid _hidden() {}\n");
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        visibility_of(&analysis, "top"),
        Visibility::Exported,
        "a top-level name without an underscore is callable from another library"
    );
    assert_eq!(
        visibility_of(&analysis, "_hidden"),
        Visibility::Private,
        "a leading underscore keeps the function library-private"
    );
}

#[test]
fn override_marks_the_signature_forced() {
    let analysis = run(
        "case.dart",
        "class A {\n  void run() {}\n}\nclass B extends A {\n  @override\n  void run() {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mut runs = analysis.facts.functions.iter().filter(|f| f.name == "run");
    let plain = runs.next().expect("has the base method");
    let forced = runs.next().expect("has the override");
    assert!(
        !plain.signature_forced,
        "a plain method keeps an editable signature"
    );
    assert!(
        forced.signature_forced,
        "an @override method restates a forced signature"
    );
}

#[test]
fn main_with_a_test_call_inside_try_gets_a_test_region() {
    let analysis = run(
        "case.dart",
        "void main() {\n  try {\n    test('parses', () {});\n  } catch (e) {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let main = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("has main");
    assert!(main.in_test, "main hosting test calls counts as test code");
    assert!(
        analysis
            .facts
            .regions
            .iter()
            .any(|r| r.role == CodeRole::Test),
        "the body of main is one Test region even when the call sits in try"
    );
}

#[test]
fn main_without_test_calls_gets_no_test_region() {
    let analysis = run("case.dart", "void main() {\n  print('hi');\n}\n");
    assert!(analysis.parse_error.is_none(), "parses");
    let main = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("has main");
    assert!(!main.in_test, "a plain main is not test code");
    assert!(
        analysis
            .facts
            .regions
            .iter()
            .all(|r| r.role != CodeRole::Test),
        "no test call anywhere means no Test region"
    );
}

#[test]
fn primitive_param_kinds_follow_the_dart_type_table() {
    let analysis = run(
        "case.dart",
        "void f(String a, bool b, int c, double d, List<String> e) {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let kinds: Vec<PrimitiveKind> = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "f")
        .expect("has f")
        .params
        .iter()
        .map(|p| p.primitive_kind)
        .collect();
    assert_eq!(
        kinds,
        [
            PrimitiveKind::Str,
            PrimitiveKind::Bool,
            PrimitiveKind::Int,
            PrimitiveKind::Float,
            PrimitiveKind::Other
        ],
        "String bool int double map by name and a generic list stays Other"
    );
}

#[test]
fn dart_sdk_and_package_uris_report_external_roots() {
    let analysis = run(
        "case.dart",
        "import 'dart:async';\nimport 'package:http/http.dart';\nimport 'utils/helper.dart';\nvoid main() {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(analysis.facts.imports.len(), 3, "all imports are recorded");
    let mut roots: Vec<Option<&str>> = analysis
        .facts
        .imports
        .iter()
        .map(|import| import.external_root)
        .collect();
    roots.sort();
    assert_eq!(
        roots,
        vec![None, Some("dart:async"), Some("http")],
        "sdk and package uris name roots while a relative uri names none"
    );
    for import in &analysis.facts.imports {
        assert!(!import.is_reexport, "imports are not re-exports");
    }
}
