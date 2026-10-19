//! Valid C that the pinned tree-sitter-c 0.24.2 grammar rejected.
//!
//! Every repro here is valid input for `gcc -fsyntax-only -std=gnu2x`
//! (checked 2026-10-20; the `#embed` case needs the named file to exist,
//! and the failure without it is a missing file, not a syntax error).
//! The vendored grammar under `vendor/tree-sitter-c` accepts each one;
//! the published crate reported a parse error for each one. The control
//! tests read facts from the parsed tree, so a node-shape regression
//! shows up as a changed fact and not only as a new error.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::C,
        path: "gap.c",
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_c::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("c language loads");
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
        "valid C must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

fn type_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.types.iter().map(|t| t.name).collect()
}

#[test]
fn complex_spellings_parse() {
    let source = concat!(
        "double complex z;\n",
        "float complex w;\n",
        "long double complex q;\n",
        "double _Complex dz;\n",
        "_Complex double zd;\n",
        "complex double cd;\n",
        "double complex f(double complex x) { return x; }\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn complex_typedef_and_sizeof_parse() {
    let source = concat!(
        "typedef double complex c;\n",
        "int n = sizeof(double complex);\n",
        "int main(void) { c v = 0; return (int)v; }\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["main"]);
    assert!(
        type_names(&analysis).contains(&"c"),
        "the `c` typedef is still recorded: {:?}",
        analysis.facts.types
    );
}

#[test]
fn thread_local_parses() {
    let source = concat!(
        "_Thread_local int x;\n",
        "static _Thread_local int y;\n",
        "extern _Thread_local int z;\n",
        "int main(void) { return x + y + z; }\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["main"]);
}

#[test]
fn atomic_functional_form_parses() {
    let source = concat!(
        "typedef _Atomic(int) t;\n",
        "_Atomic(int) a;\n",
        "struct S { _Atomic(unsigned) u; };\n",
        "void f(_Atomic(int) *p) { *p = 1; }\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    let names = type_names(&analysis);
    assert!(names.contains(&"t"), "typedef `t`: {names:?}");
    assert!(names.contains(&"S"), "struct `S`: {names:?}");
}

#[test]
fn atomic_qualifier_form_still_parses() {
    let analysis =
        run_clean("_Atomic int b;\nstatic _Atomic int c;\nint main(void) { return b + c; }\n");
    assert_eq!(function_names(&analysis), ["main"]);
}

#[test]
fn embed_as_initializer_element_parses() {
    let source = "char d[]={\n#embed \"x\" limit(5)\n};\nint main(void) { return (int)d[0]; }\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["main"]);
}

#[test]
fn embed_between_elements_parses() {
    let analysis = run_clean("char d[]={1,\n#embed <x>\n, 2};\n");
    assert!(analysis.facts.functions.is_empty());
}

#[test]
fn plain_initializer_lists_still_parse() {
    let analysis = run_clean("int a[] = {1, 2, 3};\nstruct P p = {.x = 1, .y = 2};\n");
    assert!(analysis.facts.functions.is_empty());
}

#[test]
fn has_include_in_if_parses() {
    let source = concat!(
        "#if __has_include(<endian.h>)\n",
        "#endif\n",
        "#if __has_include(\"x.h\")\n",
        "#endif\n",
        "#if defined(__has_include) && __has_include(<x.h>)\n",
        "#endif\n",
    );
    let analysis = run_clean(source);
    assert!(analysis.facts.functions.is_empty());
}

#[test]
fn has_include_next_and_embed_checks_parse() {
    let source = concat!(
        "#if __has_include_next(<x.h>)\n",
        "#endif\n",
        "#if __has_embed(\"x\") == 1\n",
        "#endif\n",
    );
    run_clean(source);
}

#[test]
fn enum_with_underlying_type_parses() {
    let source = "enum : unsigned short {A=2};\nenum E : unsigned short {B=2};\n";
    let analysis = run_clean(source);
    assert!(
        type_names(&analysis).contains(&"E"),
        "named enum `E` is still a type: {:?}",
        analysis.facts.types
    );
}

#[test]
fn plain_enums_still_parse() {
    let analysis = run_clean("enum E : int {C=1};\nenum F {D};\nenum {G};\n");
    let names = type_names(&analysis);
    assert!(names.contains(&"E"), "{names:?}");
    assert!(names.contains(&"F"), "{names:?}");
}

#[test]
fn unnamed_bitfields_parse() {
    let source = concat!(
        "struct S{unsigned:0; int x;};\n",
        "struct T{int a:3, :4, b:2;};\n",
        "int main(void) { struct S s = {0}; return s.x; }\n",
    );
    let analysis = run_clean(source);
    let names = type_names(&analysis);
    assert!(names.contains(&"S"), "{names:?}");
    assert!(names.contains(&"T"), "{names:?}");
    assert_eq!(function_names(&analysis), ["main"]);
}

#[test]
fn named_bitfields_still_parse() {
    let analysis = run_clean("struct S{unsigned a:0; int x;};\n");
    assert!(type_names(&analysis).contains(&"S"));
}

#[test]
fn register_asm_initializer_parses() {
    let source = concat!(
        "long f(long n) {\n",
        "  register long a7 __asm__(\"a7\") = n;\n",
        "  return a7;\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn signed_spellings_parse() {
    let analysis = run_clean("typedef __signed char i8;\n__signed int x;\n__signed__ int y;\n");
    assert!(type_names(&analysis).contains(&"i8"));
}

#[test]
fn a_missing_function_body_brace_still_reports_a_parse_error() {
    let analysis = run("int main(void) {\n  return 0;\n");
    assert!(
        analysis.parse_error.is_some(),
        "a missing closing brace must not parse"
    );
}

#[test]
fn a_broken_declaration_still_reports_a_parse_error() {
    let analysis = run("int x = ;\n");
    assert!(analysis.parse_error.is_some(), "`int x = ;` must not parse");
}

#[test]
fn an_unterminated_struct_still_reports_a_parse_error() {
    let analysis = run("struct S{unsigned:0;\n");
    assert!(
        analysis.parse_error.is_some(),
        "a missing struct closing must not parse"
    );
}
