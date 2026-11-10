use std::sync::LazyLock;

use sloplint_core::facts::{AmbientKind, ArgShape, PrimitiveKind, ScrutineeKind, Visibility};
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
        vec![None, None, Some("http")],
        "the sdk uri names no root while the package uri names its own"
    );
    for import in &analysis.facts.imports {
        assert!(!import.is_reexport, "imports are not re-exports");
    }
}

#[test]
fn in_function_write_to_top_level_var_is_one_global_write() {
    let analysis = run(
        "case.dart",
        "var count = 0;\nvoid bump() {\ncount = 1;\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mutables: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalMutable)
        .collect();
    assert_eq!(mutables.len(), 1, "one top-level var is a global mutable");
    assert_eq!(mutables[0].name, "count", "mutable keeps the declared name");
    let writes: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .collect();
    assert_eq!(writes.len(), 1, "in-function write joins the top-level var");
    assert_eq!(writes[0].name, "count", "write keeps the assigned name");
    assert!(
        writes[0].in_function.is_some(),
        "write is attributed inside the function"
    );
}

#[test]
fn library_exports_are_reexports() {
    let analysis = run(
        "case.dart",
        "import 'package:b/b.dart';\nexport 'package:a/a.dart';\nvoid main() {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.imports.len(),
        2,
        "both directives are recorded as imports"
    );
    let exported = analysis
        .facts
        .imports
        .iter()
        .find(|import| import.name == "a")
        .expect("has the export");
    assert!(
        exported.is_reexport,
        "a library export re-exports its module"
    );
    assert_eq!(
        exported.external_root,
        Some("a"),
        "the export names its package root"
    );
    let plain = analysis
        .facts
        .imports
        .iter()
        .find(|import| import.name == "b")
        .expect("has the import");
    assert!(!plain.is_reexport, "a plain import is not a re-export");
}

#[test]
fn defaulted_params_report_has_default() {
    let analysis = run(
        "case.dart",
        "void f(String a, [String b = 'x', bool c = false]) {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let params = &analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "f")
        .expect("has f")
        .params;
    assert_eq!(params.len(), 3, "all three params are recorded");
    assert!(!params[0].has_default, "a required param has no default");
    assert!(
        params[1].has_default,
        "an optional param with a value is defaulted"
    );
    assert!(
        !params[1].bool_default,
        "a string default is not a bool default"
    );
    assert!(params[2].has_default, "a false default is still a default");
    assert!(params[2].bool_default, "a false default is a bool default");
    assert!(!params[2].keyword_only, "bracket params stay positional");
}

#[test]
fn braced_params_are_keyword_only() {
    let analysis = run(
        "case.dart",
        "void send({bool stream = false, bool verify = true}) {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let params = &analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "send")
        .expect("has send")
        .params;
    assert_eq!(params.len(), 2, "both named params are recorded");
    for param in params {
        assert!(param.has_default, "a braced default counts as defaulted");
        assert!(param.keyword_only, "a braced param passes by name only");
    }
}

#[test]
fn interpolated_dart_strings_are_concat() {
    let analysis = run(
        "case.dart",
        "void f(String id) {\nlog.info('Processing request $id');\nlog.info('id: $id');\nlog.info('$id');\nlog.info('all done here now');\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let calls: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log.info")
        .collect();
    assert_eq!(calls.len(), 4, "all four log calls are recorded");
    for (index, want) in [(0usize, 19u32), (1, 4), (2, 0)] {
        let ArgShape::Concat { longest_static } = calls[index].args[0].shape else {
            panic!("call {index} is not a concatenation");
        };
        assert_eq!(longest_static, want, "concat static runs are hand-checked");
    }
    assert!(
        matches!(calls[3].args[0].shape, ArgShape::Str { longest_static } if longest_static == 17),
        "a plain literal keeps its measured run"
    );
}

#[test]
fn named_dart_arguments_span_the_label() {
    let analysis = run(
        "case.dart",
        "void caller() {\nfoo(\nitems:\n[1, 2, 3],\n);\nbar(\n[4, 5, 6],\n);\n}\nvoid foo({required List<int> items}) {}\nvoid bar(List<int> xs) {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let foo = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "foo")
        .expect("has foo");
    assert_eq!(foo.args.len(), 1, "foo takes one argument");
    assert_eq!(
        foo.args[0].start_line, 3,
        "a named span starts at the label"
    );
    assert_eq!(foo.args[0].end_line, 4, "a named span ends at the value");
    let bar = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "bar")
        .expect("has bar");
    assert_eq!(bar.args.len(), 1, "bar takes one argument");
    assert_eq!(
        bar.args[0].start_line, 7,
        "a positional span starts at the value"
    );
    assert_eq!(
        bar.args[0].end_line, 7,
        "a positional span ends at the value"
    );
}

#[test]
fn comments_are_excluded_from_dart_arguments() {
    let analysis = run(
        "case.dart",
        "void caller() {\nlog.info(\n// reason\n'msg here now');\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "log.info")
        .expect("has the log call");
    assert_eq!(call.args.len(), 1, "the comment is not an argument");
    assert!(
        matches!(call.args[0].shape, ArgShape::Str { .. }),
        "the literal survives the comment"
    );
}

#[test]
fn interpolated_symbol_names_are_dynamic() {
    let analysis = run(
        "case.dart",
        "dynamic a(String n) {\nreturn Symbol('get_$n');\n}\ndynamic b() {\nreturn Symbol('name');\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.dynamic_symbols.len(),
        1,
        "only the interpolated name is dynamic"
    );
}

#[test]
fn wildcard_arms_keep_values_after_reorder() {
    let analysis = run(
        "case.dart",
        "enum Color { red, green, blue }\nString describe(Color c) {\nreturn switch (c) {\nColor.red => 'red',\nColor.green => 'green',\n_ => 'other',\n};\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(analysis.facts.wildcard_arms.len(), 1, "one wildcard arm");
    let arm = &analysis.facts.wildcard_arms[0];
    assert!(arm.enum_like, "two named siblings are enum-like");
    assert_eq!(arm.variant_count, 2, "two sibling variants");
    assert_eq!(
        arm.scrutinee,
        ScrutineeKind::Ident,
        "switch on a variable reports Ident"
    );
}

#[test]
fn single_variant_switches_are_not_enum_like() {
    let analysis = run(
        "case.dart",
        "enum Color { red, green }\nString describe(Color c) {\nreturn switch (c) {\nColor.red => 'red',\n_ => 'other',\n};\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(analysis.facts.wildcard_arms.len(), 1, "one wildcard arm");
    let arm = &analysis.facts.wildcard_arms[0];
    assert!(!arm.enum_like, "one named sibling is not enum-like");
    assert_eq!(arm.variant_count, 1, "one sibling variant");
}

#[test]
fn private_extensions_hide_their_methods() {
    let analysis = run(
        "case.dart",
        "extension _Fmt on int {\nvoid show(bool a, bool b) {}\n}\nextension Fmt on int {\nvoid tell(bool a, bool b) {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        visibility_of(&analysis, "show"),
        Visibility::Private,
        "methods of an underscore extension are library-private"
    );
    assert_eq!(
        visibility_of(&analysis, "tell"),
        Visibility::Exported,
        "methods of a named extension stay exported"
    );
}

#[test]
fn comments_do_not_break_struct_literals() {
    let analysis = run(
        "case.dart",
        "class Point {\nfinal int x;\nfinal int y;\nPoint({required this.x, required this.y});\n}\nint copyX(Point p) {\nreturn Point(\nx: p.x,\n// axis\n y: p.y,\n).x;\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.field_copies.len(),
        1,
        "named member copies still form a literal"
    );
    let copy = &analysis.facts.field_copies[0];
    assert_eq!(copy.fields, 2, "both fields count");
    assert_eq!(copy.source, Some("p"), "the shared source is kept");
}

#[test]
fn top_level_const_initializers_count() {
    let analysis = run(
        "case.dart",
        "const table = build([1, 2]);\nvoid main() {\nbuild([3]);\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let bound: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "build")
        .collect();
    assert_eq!(bound.len(), 2, "both calls are recorded");
    assert!(
        bound[0].initializes_binding,
        "a const initializer counts as a binding"
    );
    assert!(
        !bound[1].initializes_binding,
        "a plain call does not initialize a binding"
    );
}

#[test]
fn comments_do_not_hide_dart_defaults() {
    let analysis = run(
        "case.dart",
        "void f(String a, [String b /* why */ = 'x']) {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let params = &analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "f")
        .expect("has f")
        .params;
    assert_eq!(params.len(), 2, "both params are recorded");
    assert!(!params[0].has_default, "a required param has no default");
    assert!(
        params[1].has_default,
        "a comment before the equals sign hides no default"
    );
}

#[test]
fn mixed_type_siblings_are_not_enum_like() {
    let analysis = run(
        "case.dart",
        "void f(Object x) {\nswitch (x) {\ncase A.a:\nbreak;\ncase B.b:\nbreak;\ndefault:\nbreak;\n}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(analysis.facts.wildcard_arms.len(), 1, "one wildcard arm");
    let arm = &analysis.facts.wildcard_arms[0];
    assert_eq!(arm.variant_count, 2, "two sibling variants");
    assert!(!arm.enum_like, "variants of two types are not enum-like");
}

#[test]
fn keyed_dart_literals_are_not_collections() {
    let analysis = run(
        "case.dart",
        "void go() {\nemit({'a': 1, 'b': 2});\nstore({1, 2, 3});\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let emit = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "emit")
        .expect("has emit");
    assert!(
        matches!(emit.args[0].shape, ArgShape::Other),
        "a keyed literal is not a collection"
    );
    let store = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "store")
        .expect("has store");
    assert!(
        matches!(store.args[0].shape, ArgShape::Collection),
        "a set literal stays a collection"
    );
}

#[test]
fn closures_inside_dart_collections_are_bodies() {
    let analysis = run(
        "case.dart",
        "void go() {\nrun([() { work(); }, 1]);\nrun([1, 2]);\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let calls: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "run")
        .collect();
    assert_eq!(calls.len(), 2, "both calls are recorded");
    assert!(
        matches!(calls[0].args[0].shape, ArgShape::CodeBody),
        "a list holding a closure is a body"
    );
    assert!(
        matches!(calls[1].args[0].shape, ArgShape::Collection),
        "a plain list stays a collection"
    );
}
