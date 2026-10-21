//! Valid Starlark that the pinned tree-sitter-starlark 1.3.0 grammar rejected.
//!
//! In real Starlark `assert` is an ordinary name: the Go implementation does
//! not reserve it, and its tests use a predeclared `assert` module, as in
//! `assert.eq(a, b)`. The vendored grammar under
//! `vendor/tree-sitter-starlark` accepts these forms; the published crate
//! reported a parse error for each one. The Python-style statement
//! `assert cond, message` stays accepted where it cannot be confused with an
//! ordinary use. The control tests read facts from the parsed tree, so a
//! node-shape regression shows up as a changed fact and not only as a new
//! error.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Starlark,
        path: "gap.bzl",
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_starlark::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("starlark language loads");
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
        "valid Starlark must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn callees<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.calls.iter().map(|c| c.callee).collect()
}

fn bindings<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.bindings.iter().map(|b| b.name).collect()
}

#[test]
fn assert_module_calls_parse() {
    let analysis = run_clean("assert.true(True)\nassert.fail(\"x\")\nassert.lt(a, b)\n");
    assert_eq!(
        callees(&analysis),
        ["assert.true", "assert.fail", "assert.lt"]
    );
}

#[test]
fn assert_as_a_module_name_parses() {
    let source = "def m():\n    assert = module(\"assert\", eq = _eq)\n    assert.eq(1, 1)\n";
    let analysis = run_clean(source);
    assert!(
        bindings(&analysis).contains(&"assert"),
        "`assert` is bound, got {:?}",
        bindings(&analysis)
    );
    let eq_calls: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "assert.eq")
        .collect();
    assert_eq!(eq_calls.len(), 1, "one `assert.eq` call");
    assert_eq!(eq_calls[0].args.len(), 2, "two compared values");
}

#[test]
fn assert_helpers_are_ordinary_calls_and_names() {
    let source =
        "def f(a, b):\n    assert_eq(a, b)\n    assert_fails(a, \"msg\")\n    assert_ = 1\n";
    let analysis = run_clean(source);
    assert_eq!(callees(&analysis), ["assert_eq", "assert_fails"]);
    assert!(
        bindings(&analysis).contains(&"assert_"),
        "`assert_` is bound, got {:?}",
        bindings(&analysis)
    );
}

#[test]
fn ambiguous_forms_prefer_the_ordinary_reading() {
    let analysis = run_clean("x = assert(1)\ny = assert[0]\n");
    assert!(
        analysis.facts.calls.iter().any(|c| c.callee == "assert"),
        "`assert(1)` is an ordinary call, got {:?}",
        callees(&analysis)
    );
}

#[test]
fn an_unambiguous_assert_statement_still_parses() {
    let analysis = run_clean("assert x > 0\n");
    assert_eq!(
        analysis.facts.assertions.len(),
        1,
        "the kept statement form still reports one assertion"
    );
    assert!(
        !analysis.facts.assertions[0].has_message,
        "no message was given"
    );
}

#[test]
fn an_incomplete_assert_assignment_still_reports_a_parse_error() {
    let analysis = run("assert =\n");
    assert!(
        analysis.parse_error.is_some(),
        "an incomplete assignment must still be a parse error"
    );
}
