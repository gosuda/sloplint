//! Valid C++ that the pinned tree-sitter-cpp 0.23.4 grammar rejected.
//!
//! Every repro here is valid input for `g++ -fsyntax-only -std=c++23` and
//! `clang++ -fsyntax-only`. The vendored grammar under
//! `vendor/tree-sitter-cpp` accepts it; the published crate reported a parse
//! error for each one. The control tests read facts from the parsed tree, so
//! a node-shape regression shows up as a changed fact and not only as a new
//! error.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Cpp,
        path: "gap.cc",
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("cpp language loads");
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
        "valid C++ must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

fn callees<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.calls.iter().map(|c| c.callee).collect()
}

#[test]
fn has_include_with_angle_brackets_parses() {
    let analysis = run_clean("#if __has_include(<endian.h>)\nint a;\n#endif\n");
    assert!(
        analysis.facts.functions.is_empty(),
        "has_include with angle brackets records no functions: {:?}",
        analysis.facts.functions
    );
    assert_eq!(callees(&analysis), ["__has_include"]);
}

#[test]
fn has_include_with_quotes_and_attribute_checks_parse() {
    let analysis = run_clean(concat!(
        "#if __has_include(\"missing.h\") || __has_cpp_attribute(nodiscard) >= 201603\n",
        "int c;\n",
        "#endif\n",
        "#if defined(__has_attribute) && __has_attribute(noreturn)\n",
        "int b;\n",
        "#endif\n",
    ));
    assert!(
        analysis.facts.functions.is_empty(),
        "has_include with quotes records no functions: {:?}",
        analysis.facts.functions
    );
    assert_eq!(
        callees(&analysis),
        ["__has_include", "__has_cpp_attribute", "__has_attribute"]
    );
}

#[test]
fn has_cpp_attribute_with_a_scoped_name_parses() {
    let analysis = run_clean("#if __has_cpp_attribute(clang::fallthrough)\nint d;\n#endif\n");
    assert_eq!(callees(&analysis), ["__has_cpp_attribute"]);
}

#[test]
fn has_embed_and_has_warning_with_strings_parse() {
    let analysis = run_clean(
        "#if __has_embed(\"data.bin\")\nint e;\n#endif\n#if __has_warning(\"-Wdeprecated\")\nint f;\n#endif\n",
    );
    assert!(
        analysis.facts.functions.is_empty(),
        "has_embed and has_warning record no functions: {:?}",
        analysis.facts.functions
    );
    assert_eq!(callees(&analysis), ["__has_embed", "__has_warning"]);
}

#[test]
fn an_unclosed_has_include_still_reports_a_parse_error() {
    assert!(
        run("#if __has_include(<a.h>\n#endif\n")
            .parse_error
            .is_some(),
        "an unclosed `__has_include(` must not parse"
    );
}

#[test]
fn an_unnamed_enum_with_a_fixed_base_parses() {
    let analysis = run_clean("enum : unsigned short {A=2};\nvoid f() {}\n");
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn a_named_enum_with_a_base_still_parses() {
    let analysis = run_clean("enum class E : int {A};\nvoid f() {}\n");
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn unnamed_bitfields_parse() {
    let analysis = run_clean("struct S{unsigned:0; int x;};\nint size(S *p) { return p->x; }\n");
    assert_eq!(function_names(&analysis), ["size"]);
}

#[test]
fn a_named_bitfield_still_parses() {
    let analysis = run_clean("struct T{int a:3; int b:2;};\nvoid f() {}\n");
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn a_bitfield_with_a_braced_default_member_init_parses() {
    let analysis = run_clean("struct S{int m:3{0};};\nvoid f() {}\n");
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn a_member_access_initializer_in_a_declaration_parses() {
    let source = concat!(
        "struct T { int get() const; int x; };\n",
        "void f(T v) {\n",
        "  const int& r(v.get());\n",
        "  int* p(v.get());\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(callees(&analysis), ["v.get", "v.get"]);
}

#[test]
fn a_plain_initializer_with_member_access_still_parses() {
    let analysis = run_clean("struct T { int get() const; };\nvoid f(T v) { int x(v.get()); }\n");
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(callees(&analysis), ["v.get"]);
}

#[test]
fn explicit_class_template_instantiation_parses() {
    let source = concat!(
        "template <class T> struct C { T t; };\n",
        "template class C<int>;\n",
        "extern template class C<long>;\n",
        "template struct C<char>;\n",
    );
    let analysis = run_clean(source);
    assert!(
        analysis.facts.functions.is_empty(),
        "instantiations are not definitions: {:?}",
        function_names(&analysis)
    );
}

#[test]
fn explicit_function_instantiation_still_parses() {
    let analysis = run_clean("template <class T> void g(T) {}\ntemplate void g<int>(int);\n");
    assert_eq!(function_names(&analysis), ["g"]);
}

#[test]
fn operator_calls_through_member_access_parse() {
    let source = concat!(
        "struct X { bool equals(const X&) const; };\n",
        "void f(X x, X *px) {\n",
        "  x.operator==(x);\n",
        "  px->operator()(1);\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(callees(&analysis), ["x.operator==", "px->operator()"]);
}

#[test]
fn pointer_to_member_access_parses_without_a_call() {
    let source = concat!(
        "struct F { int field; };\n",
        "void f(F *config, int F::*fieldp) { config->*fieldp; }\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert!(
        analysis.facts.calls.is_empty(),
        "a bare `->*` access is not a call"
    );
}

#[test]
fn a_plain_member_call_still_parses() {
    let analysis = run_clean("void f(X x) { x.foo(1); }\n");
    assert_eq!(callees(&analysis), ["x.foo"]);
}

#[test]
fn a_braced_default_argument_parses() {
    let analysis = run_clean("void f(int x={}) {}\nvoid h(int y = {3}) {}\n");
    assert_eq!(function_names(&analysis), ["f", "h"]);
    let f = &analysis.facts.functions[0];
    assert_eq!(f.params.len(), 1, "one parameter with a default");
}

#[test]
fn array_new_of_pointers_parses() {
    let analysis = run_clean("void f(int n) {\n  int **p = new int*[n];\n  delete[] p;\n}\n");
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(
        callees(&analysis),
        ["int"],
        "the allocation still reads as a `new` of `int`"
    );
}

#[test]
fn plain_array_new_still_parses() {
    let analysis = run_clean("void f(int n) {\n  int *p = new int[n];\n  delete[] p;\n}\n");
    assert_eq!(callees(&analysis), ["int"]);
}

#[test]
fn a_deleted_free_operator_parses_like_a_deleted_member() {
    // A body-less definition yields no function fact, exactly like an in-class
    // `= delete;` member. What matters is that `delete` is not misread as a
    // delete-expression call and the file parses.
    let analysis = run_clean("struct A{}; struct B{};\nbool operator==(A a,B b)=delete;\n");
    assert!(
        analysis.facts.functions.is_empty(),
        "deleted free operator records no functions: {:?}",
        analysis.facts.functions
    );
    assert!(
        analysis.facts.calls.is_empty(),
        "`= delete` is not a call: {:?}",
        callees(&analysis)
    );
}

#[test]
fn a_defaulted_friend_operator_still_parses() {
    let analysis = run_clean("struct S { friend bool operator==(S,S) = default; };\n");
    assert!(
        analysis.facts.functions.is_empty(),
        "defaulted friend operator records no functions: {:?}",
        analysis.facts.functions
    );
}

#[test]
fn a_functional_cast_to_a_dependent_type_parses_as_an_argument() {
    let source = concat!(
        "void g(...);\n",
        "template <class S> void f() { g(typename S::First()); }\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(callees(&analysis), ["g", "typename S::First"]);
}

#[test]
fn a_braced_dependent_type_parses_as_an_initializer() {
    let analysis = run_clean("template <class S> void h() { auto x = typename S::First{}; }\n");
    assert_eq!(function_names(&analysis), ["h"]);
}

#[test]
fn a_bitfield_without_a_width_still_reports_a_parse_error() {
    assert!(
        run("struct S { int : ; };\n").parse_error.is_some(),
        "a bit-field needs a width"
    );
}

#[test]
fn an_enum_base_without_a_type_still_reports_a_parse_error() {
    assert!(
        run("enum : {A};\n").parse_error.is_some(),
        "an unnamed enum needs a base type"
    );
}

#[test]
fn a_missing_semicolon_after_delete_still_reports_a_parse_error() {
    assert!(
        run("bool operator==(A a, B b) = delete\n")
            .parse_error
            .is_some(),
        "a deleted operator needs its semicolon"
    );
}
