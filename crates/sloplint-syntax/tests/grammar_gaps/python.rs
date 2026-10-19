//! Valid Python that the pinned tree-sitter-python 0.25.0 grammar rejected.
//!
//! Every repro here is accepted by CPython 3.15.0 (`ast.parse`). The vendored
//! grammar under `vendor/tree-sitter-python` accepts it; the published crate
//! reported a parse error for each repro and parsed each control as it does
//! now. The control tests read facts from the parsed tree, so a node-shape
//! regression shows up as a changed fact and not only as a new error.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Python,
        path: "gap.py",
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn run_clean(source: &str) -> Analysis<'_> {
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_none(),
        "valid Python must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn run_error(source: &str) {
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_some(),
        "invalid Python must still report a parse error:\n{source}"
    );
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

fn call_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.calls.iter().map(|c| c.callee).collect()
}

fn import_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.imports.iter().map(|i| i.name).collect()
}

// --- PEP 696: type parameter defaults -------------------------------------

#[test]
fn type_alias_with_a_defaulted_type_parameter_parses() {
    let source = "type X[T = int] = list[T]\n";
    let analysis = run_clean(source);
    // The whole alias is one expression statement; the tree shape must not
    // swallow it.
    assert!(analysis.facts.calls.is_empty());
}

#[test]
fn generic_function_with_a_defaulted_type_parameter_parses() {
    let source = "from typing import reveal_type\n\n\ndef f[T = int](x: T) -> T:\n    return x\n\n\nreveal_type(f)\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
    assert_eq!(call_names(&analysis), ["reveal_type"]);
}

#[test]
fn generic_class_with_a_defaulted_type_parameter_parses() {
    let source = "class C[T = int]:\n    x: T\n";
    run_clean(source);
}

#[test]
fn every_type_parameter_kind_takes_a_default() {
    run_clean(concat!(
        "def f[T = int, *Ts = *tuple[int, ...], **P = [int, str]](): ...\n",
        "class C[*Ts = *tuple[int]]: ...\n",
        "type X[**P = [int, str]] = int\n",
    ));
}

#[test]
fn constrained_type_parameters_take_defaults_and_still_parse_without() {
    run_clean("def f[T: int = str, U: (int, str) = int](): ...\n");
    // A bound with no default still parses as before.
    run_clean("def f[T: int, U: (int, str)](): ...\n");
}

// --- Starred annotations ----------------------------------------------------

#[test]
fn parenthesized_starred_annotation_parses() {
    let source = "def foo(*args: *(int)): ...\n";
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["foo"]);
}

#[test]
fn plain_starred_annotation_still_parses() {
    run_clean("def foo(*args: *int): ...\ndef bar(*args: *a.b): ...\ndef baz(*args: *a[b]): ...\n");
}

// --- Starred expressions in comma lists -------------------------------------

#[test]
fn starred_list_on_an_assignment_right_hand_side_parses() {
    let analysis = run_clean("_ = *[x],\n_ = a, *[y]\n_ = b, *[g(z)]\n");
    assert_eq!(
        call_names(&analysis),
        ["g"],
        "the call inside the starred list is still a call"
    );
}

#[test]
fn starred_await_in_a_comma_list_parses() {
    let analysis = run_clean("async def g():\n    *await y,\n");
    assert_eq!(function_names(&analysis), ["g"]);
}

#[test]
fn starred_list_in_a_for_target_parses() {
    run_clean("for a in *[x], y:\n    pass\nfor a in *(x), y:\n    pass\n");
}

#[test]
fn starred_bracket_and_literal_operands_parse_in_comma_lists() {
    run_clean(concat!(
        "_ = *{1}, 2\n",
        "_ = *{1: 2},\n",
        "_ = *\"ab\",\n",
        "_ = *1,\n",
        "_ = *None,\n",
        "_ = *...,\n",
        "_ = *-x,\n",
        "_ = *[a for a in b],\n",
        "_ = *(a for a in b),\n",
        "_ = *(a, b), c\n",
        "*(a + 1),\n",
    ));
}

#[test]
fn a_plain_right_hand_side_star_keeps_parsing() {
    // These parsed before the patch and must keep parsing.
    let analysis = run_clean("_ = *x,\n_ = a, *b\n_ = a, *c.d, e[0]\n_ = g(*x), h()\n");
    assert_eq!(call_names(&analysis), ["g", "h"]);
}

// --- Starred subscripts -------------------------------------------------------

#[test]
fn starred_walrus_in_a_subscript_parses() {
    run_clean("data[*(x := y)]\n");
}

#[test]
fn starred_subscript_operands_parse() {
    run_clean("data[*[a]]\ndata[*(a)]\nx[a, *(b)]\n");
}

#[test]
fn plain_starred_subscript_still_parses() {
    run_clean("x[*a] = 1\ndel x[*a]\nx[*a] += 1\nx[*a, b]\n");
}

// --- PEP 810: lazy imports ----------------------------------------------------

#[test]
fn lazy_imports_parse() {
    run_clean("lazy import foo\nlazy import a.b.c as d\n");
}

#[test]
fn lazy_from_imports_parse() {
    let analysis = run_clean("lazy from foo import bar\nlazy from . import x\n");
    assert_eq!(import_names(&analysis), ["bar", "x"]);
}

#[test]
fn lazy_still_works_as_a_name() {
    let analysis = run_clean("lazy = 1\nlazy(x)\nlazy.x = 1\nimport lazy\n");
    assert_eq!(call_names(&analysis), ["lazy"]);
    assert_eq!(import_names(&analysis), ["lazy"]);
}

// --- PEP 798: unpacking in comprehensions -------------------------------------

#[test]
fn dict_unpacking_in_a_dict_comprehension_parses() {
    run_clean("out = {**x for x in y}\n");
}

#[test]
fn starred_comprehension_bodies_parse() {
    run_clean("[*x for x in y]\n{*x for x in y}\n(*x for x in y)\n{**{} for x in y}\n");
}

// --- 3.15: unary plus in case patterns -----------------------------------------

#[test]
fn unary_plus_in_a_case_pattern_parses() {
    run_clean("match foo:\n    case +1: ...\n");
}

#[test]
fn signed_complex_case_patterns_parse() {
    run_clean(concat!(
        "match foo:\n",
        "    case -1: ...\n",
        "    case 1 + 2j: ...\n",
        "    case -1 - 2j: ...\n",
        "    case +1 + 2j: ...\n",
        "    case +1 | -1: ...\n",
        "    case {+1: a}: ...\n",
        "    case [+1, -2]: ...\n",
    ));
}

// --- Backslash continuation at the start of a block -----------------------------

#[test]
fn backslash_continuation_before_an_indented_block_parses() {
    run_clean("if True:\n    \\\n        1\n    \\\n2\nelse:\n    3\n");
}

#[test]
fn backslash_continuations_with_comments_and_blank_lines_parse() {
    run_clean("if x:\n    \\\n\n    3\nif x:\n    \\\n   # c\n    3\n");
}

// --- Controls: statements that must keep reporting parse errors ------------------

#[test]
fn invalid_python_still_reports_parse_errors() {
    run_error("f((a)=1\n");
    run_error("def f(:\n");
    run_error("class\n");
    run_error("x =\n");
}

#[test]
fn bare_starred_parenthesized_statement_stays_an_error() {
    // The published grammar reports this, CPython's parser accepts it and its
    // compiler rejects it ("can't use starred expression here"); the vendored
    // grammar keeps reporting it.
    run_error("*(a + 1)\n");
}

#[test]
fn a_single_starred_list_element_is_a_tuple_display() {
    // `*x,` is a valid tuple display and stays clean.
    run_clean("*x,\n");
}
