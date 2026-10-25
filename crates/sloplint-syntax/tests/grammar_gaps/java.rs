//! Valid Java that the pinned tree-sitter-java 0.23.5 grammar rejected.
//!
//! Every repro here is valid input for `javac 25.0.4.1` (checked 2026-10-20;
//! the scratch inputs live outside the repo). The vendored grammar under
//! `vendor/tree-sitter-java` accepts each one; the published crate reported
//! a parse error for each one. The control tests read facts from the parsed
//! tree, so a node-shape regression shows up as a changed fact and not only
//! as a new error.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Java,
        path: "gap.java",
        source,
        is_test_file: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("java language loads");
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
        "valid Java must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

fn params_of<'a>(analysis: &'a Analysis<'a>, function: &str) -> Vec<(&'a str, Option<&'a str>)> {
    analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == function)
        .map(|f| f.params.iter().map(|p| (p.name, p.ty)).collect())
        .unwrap_or_default()
}

#[test]
fn annotation_before_ellipsis_parses() {
    let analysis = run_clean("void m(String @N ... xs) {\n}\n");
    assert_eq!(function_names(&analysis), ["m"]);
    assert_eq!(params_of(&analysis, "m"), [("xs", Some("String @N ..."))]);
}

#[test]
fn modifier_and_annotation_before_ellipsis_parse() {
    let analysis = run_clean("void m(@N Object @N ... a) {\n}\n");
    assert_eq!(function_names(&analysis), ["m"]);
    assert_eq!(params_of(&analysis, "m"), [("a", Some("Object @N ..."))]);
}

#[test]
fn scoped_and_argument_annotations_before_ellipsis_parse() {
    let analysis = run_clean("void m(String @Valid ... v) {\n}\n");
    assert_eq!(
        params_of(&analysis, "m"),
        [("v", Some("String @Valid ..."))]
    );
    let analysis = run_clean("void m(String @lib.Valid ... w) {\n}\n");
    assert_eq!(
        params_of(&analysis, "m"),
        [("w", Some("String @lib.Valid ..."))]
    );
    let analysis = run_clean("void m(String @V2(\"x\") ... v) {\n}\n");
    assert_eq!(
        params_of(&analysis, "m"),
        [("v", Some("String @V2(\"x\") ..."))]
    );
}

#[test]
fn primitive_and_repeated_annotations_before_ellipsis_parse() {
    let analysis = run_clean("void m(int @N ... ns) {\n}\n");
    assert_eq!(params_of(&analysis, "m"), [("ns", Some("int @N ..."))]);
    let analysis = run_clean("void m(String @A @B ... xs) {\n}\n");
    assert_eq!(
        params_of(&analysis, "m"),
        [("xs", Some("String @A @B ..."))]
    );
}

#[test]
fn constructor_and_record_varargs_annotations_parse() {
    let analysis = run_clean("class C {\n  C(String @N ... xs) {\n  }\n}\n");
    assert_eq!(function_names(&analysis), ["C"]);
    assert_eq!(params_of(&analysis, "C"), [("xs", Some("String @N ..."))]);
    let analysis = run_clean("record R(String @N ... xs) {\n}\n");
    assert!(
        analysis.facts.types.iter().any(|t| t.name == "R"),
        "the `R` record is still recorded: {:?}",
        analysis.facts.types
    );
}

#[test]
fn varargs_controls_without_pre_ellipsis_annotations_parse() {
    // A plain variadic parameter keeps its shape: no annotations involved.
    let analysis = run_clean("void plain(String ... xs) {\n}\n");
    assert_eq!(params_of(&analysis, "plain"), [("xs", Some("String ..."))]);
    // An annotated array dimension still parses as a formal parameter, not a
    // spread parameter.
    let analysis = run_clean("void dims(String @N [] f) {\n}\n");
    assert_eq!(params_of(&analysis, "dims"), [("f", Some("String @N []"))]);
    // An annotated type bound still parses.
    let analysis = run_clean("<T extends @N Object> T id(T x) {\n  return x;\n}\n");
    assert_eq!(function_names(&analysis), ["id"]);
}

#[test]
fn invalid_varargs_still_report_parse_errors() {
    // A missing declarator is invalid: javac rejects `String ...` with no name.
    let analysis = run("void m(String ... ) {\n}\n");
    assert!(
        analysis.parse_error.is_some(),
        "a variadic parameter with no name must still report a parse error"
    );
    // A doubled ellipsis is invalid: javac rejects `... ...`.
    let analysis = run("void m(String ... ... xs) {\n}\n");
    assert!(
        analysis.parse_error.is_some(),
        "a doubled ellipsis must still report a parse error"
    );
}
