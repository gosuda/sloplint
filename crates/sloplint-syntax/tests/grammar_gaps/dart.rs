//! Regression tests for the tree-sitter-dart grammar vendored under
//! `vendor/tree-sitter-dart`.
//!
//! Fix repros are accepted by `dart format --output=none` (Dart 3.13.5, no
//! experiment flags) and must parse cleanly; the published 0.2.0 crate rejects
//! them. Controls are rejected by the same tool and must keep a parse error.
//! One non-regression guard passes on both grammars.
//!
//! Over-accepts and residue (version pins, experimental augmentation syntax,
//! `new` as a constructor name, and dart-accepted files that still error) are
//! documented in `vendor/tree-sitter-dart/SLOPLINT.md`. They are not asserted
//! here.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Dart,
        path: "gap.dart",
        source,
        is_test_file: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_dart::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("dart language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let root = tree.root_node();
    assert!(
        !root.has_error() && !root.to_sexp().contains("MISSING"),
        "raw tree must be error-free, sexp: {}",
        root.to_sexp()
    );
}

fn run_clean(source: &str) -> Analysis<'_> {
    assert_raw_clean(source);
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_none(),
        "valid Dart must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn type_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.types.iter().map(|t| t.name).collect()
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

#[test]
fn a_primary_constructor_class_header_parses() {
    let analysis = run_clean("class A(final int x) {}\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"A"),
        "type A recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_const_primary_header_with_a_const_constructor_call_parses() {
    let source = "class const B(final String name) {\n  static const B b = const B(\"b\");\n}\nvoid main() {}\n";
    let analysis = run_clean(source);
    assert!(
        type_names(&analysis).contains(&"B"),
        "type B recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_dotted_primary_constructor_after_type_parameters_parses() {
    let analysis = run_clean("class C<T>.named() {}\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"C"),
        "type C recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_primary_class_with_a_dotted_name_parses() {
    let analysis = run_clean("mixin class Class2.named();\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"Class2"),
        "type Class2 recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn bodyless_class_declarations_parse() {
    let analysis =
        run_clean("class CC;\nsealed class R;\nabstract interface class O;\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"CC"),
        "type CC recorded: {:?}",
        type_names(&analysis)
    );
    assert!(
        type_names(&analysis).contains(&"R"),
        "type R recorded: {:?}",
        type_names(&analysis)
    );
    assert!(
        type_names(&analysis).contains(&"O"),
        "type O recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_primary_parameter_list_may_end_in_a_trailing_comma() {
    let analysis = run_clean("class C(final int x,) {}\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"C"),
        "type C recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn an_extension_type_representation_may_end_in_a_trailing_comma() {
    let analysis = run_clean("extension type E(int it,) {}\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"E"),
        "type E recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn enum_primary_headers_parse() {
    let analysis = run_clean("enum E() {}\nenum const E2(int a) { x(0) }\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"E"),
        "type E recorded: {:?}",
        type_names(&analysis)
    );
    assert!(
        type_names(&analysis).contains(&"E2"),
        "type E2 recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn an_enum_with_an_empty_body_parses() {
    let analysis = run_clean("enum E {}\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"E"),
        "type E recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_new_declarator_with_an_initializer_list_parses() {
    let source = "class S {\n  final Object p;\n  final Object q;\n  new empty(this.p) : q = p;\n}\nvoid main() {}\n";
    run_clean(source);
}

#[test]
fn a_const_new_declarator_with_a_name_parses() {
    let source = "class S {\n  final int x;\n  const new named(this.x);\n}\nvoid main() {}\n";
    run_clean(source);
}

#[test]
fn a_const_new_declarator_with_a_body_parses() {
    let analysis = run_clean("class C7 {\n  const new() {}\n}\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"C7"),
        "type C7 recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn an_empty_name_factory_redirect_under_a_primary_header_parses() {
    let analysis = run_clean("class D(final int x) {\n  factory() = D;\n}\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"D"),
        "type D recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_this_primary_body_in_an_extension_type_parses() {
    let analysis =
        run_clean("extension type ET1(int a) {\n  this {\n    print(a);\n  }\n}\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"ET1"),
        "type ET1 recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_this_initializer_body_parses() {
    let analysis = run_clean("class C2 {\n  this : assert(true);\n}\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"C2"),
        "type C2 recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_typed_initializing_primary_parameter_parses() {
    let analysis = run_clean("class C(int this.z);\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"C"),
        "type C recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_super_primary_parameter_parses() {
    let analysis = run_clean("class C(super.x);\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"C"),
        "type C recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn declaring_and_initializing_primary_parameters_mix_parses() {
    let analysis = run_clean("class C(final int x, this.z);\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"C"),
        "type C recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_var_initializing_primary_parameter_still_reports_a_parse_error() {
    assert!(
        run("class C(var this.str);\n").parse_error.is_some(),
        "var initializing parameter must not parse"
    );
}

#[test]
fn a_final_initializing_primary_parameter_still_reports_a_parse_error() {
    assert!(
        run("class C(final this.z);\n").parse_error.is_some(),
        "final initializing parameter must not parse"
    );
}

#[test]
fn a_nul_byte_inside_a_string_literal_parses() {
    let analysis = run_clean("void main() {\n  print(\"\0\");\n}\n");
    assert!(
        function_names(&analysis).contains(&"main"),
        "function main recorded: {:?}",
        function_names(&analysis)
    );
}

#[test]
fn a_generator_getter_with_an_arrow_body_parses() {
    let analysis =
        run_clean("class G {\n  dynamic get g async* => throw \"x\";\n}\nvoid main() {}\n");
    assert!(
        type_names(&analysis).contains(&"G"),
        "type G recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn await_may_name_a_type() {
    let analysis = run_clean("class await {}\nvoid foo() {\n  await y, z;\n}\n");
    assert!(
        type_names(&analysis).contains(&"await"),
        "type await recorded: {:?}",
        type_names(&analysis)
    );
}

#[test]
fn a_function_typed_parameter_inside_a_function_type_parses() {
    run_clean("typedef F = int Function(int f(String x));\nvoid main() {}\n");
}

#[test]
fn a_tilde_between_operands_parses() {
    run_clean("int t() => 0;\nvoid main() {\n  t ~ 0;\n}\n");
}

#[test]
fn yield_may_label_a_statement() {
    run_clean(
        "void main() {\n  int c = 0;\n  yield: for (int a = 0; a < 10; ++a) {\n    ++c;\n  }\n}\n",
    );
}

#[test]
fn a_trailing_comma_lambda_parameter_list_inside_a_list_still_parses() {
    let source = "Future<List<G>> get allTargets async {\n  return <G>[\n    F('a', (_,) async {\n      return 1;\n    }),\n  ];\n}\n";
    run_clean(source);
}

#[test]
fn a_primary_class_with_a_parenthesis_error_still_reports_a_parse_error() {
    assert!(
        run("class C( {}\n").parse_error.is_some(),
        "open paren without parameters must not parse"
    );
}

#[test]
fn a_stray_closing_parenthesis_in_a_class_header_still_reports_a_parse_error() {
    assert!(
        run("class C) {}\n").parse_error.is_some(),
        "stray closing paren in class header must not parse"
    );
}

#[test]
fn a_const_header_without_parameters_still_reports_a_parse_error() {
    assert!(
        run("class const C {}\n").parse_error.is_some(),
        "const header without parameters must not parse"
    );
}

#[test]
fn a_dotted_name_without_parameters_still_reports_a_parse_error() {
    assert!(
        run("class C.named {}\n").parse_error.is_some(),
        "dotted name without parameters must not parse"
    );
}

#[test]
fn a_const_enum_header_without_parameters_still_reports_a_parse_error() {
    assert!(
        run("enum const E {}\n").parse_error.is_some(),
        "const enum header without parameters must not parse"
    );
}

#[test]
fn a_version_pinned_legacy_case_head_still_reports_a_parse_error() {
    let source = "void main() {\n  switch (1) {\n    case 1 + 1:\n      break;\n  }\n}\n";
    assert!(
        run(source).parse_error.is_some(),
        "legacy case head must not parse: {source:?}"
    );
}

#[test]
fn an_unterminated_block_still_reports_a_parse_error() {
    assert!(
        run("void foo() {\n").parse_error.is_some(),
        "unterminated block must not parse"
    );
}

#[test]
fn a_doubled_augment_keyword_still_reports_a_parse_error() {
    assert!(
        run("class C {\n  augment augment void method() {}\n}\n")
            .parse_error
            .is_some(),
        "doubled augment keyword must not parse"
    );
}

#[test]
fn a_truncated_null_aware_cascade_still_reports_a_parse_error() {
    assert!(
        run("main() => 1?.{;\n").parse_error.is_some(),
        "truncated null-aware cascade must not parse"
    );
}
