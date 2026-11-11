//! Starlark call-argument shapes: lambda and collection arguments, computed getattr, deep plus chains, operator spellings, and repo labels.

use std::sync::LazyLock;

use sloplint_core::facts::{ArgShape, AssertShape};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> sloplint_syntax::Analysis<'_> {
    facts_at("pkg/test.bzl", source)
}

fn facts_at<'s>(path: &'s str, source: &'s str) -> sloplint_syntax::Analysis<'s> {
    let input = FileInput {
        lang: Lang::Starlark,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn lambda_argument_is_code_body() {
    let analysis = facts_of("def f():\n    helper(lambda x: x + 1)\n    helper([1, 2])\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes: Vec<ArgShape> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "helper")
        .map(|c| {
            c.args
                .first()
                .expect("helper takes one argument")
                .shape
                .clone()
        })
        .collect();
    assert_eq!(shapes.len(), 2, "two helper calls, got {shapes:?}");
    assert_eq!(
        shapes[0],
        ArgShape::CodeBody,
        "lambda argument is a code body"
    );
    assert_eq!(
        shapes[1],
        ArgShape::Collection,
        "list argument stays a collection"
    );
}

#[test]
fn computed_getattr_reports_dynamic_symbol() {
    let analysis = facts_of(
        "def f(x, name):\n    native.glob(a, b, c)\n    getattr(x, name)\n    getattr(x, \"name\")\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.dynamic_symbols.len(),
        1,
        "one computed name, got {:?}",
        analysis.facts.dynamic_symbols.len()
    );
    assert_eq!(
        analysis.facts.dynamic_symbols[0].what, "getattr() with computed name",
        "computed getattr is reported"
    );
}

fn computed_names(source: &str) -> usize {
    let analysis = facts_of(source);
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    analysis.facts.dynamic_symbols.len()
}

#[test]
fn a_literal_loop_name_is_static_until_something_rebinds_it() {
    let plain = "def f(obj):\n    for k in [\"a\", \"b\"]:\n        getattr(obj, k)\n";
    let subscript =
        "def f(obj, d):\n    for k in [\"a\", \"b\"]:\n        d[k] = 1\n        getattr(obj, k)\n";
    let assigned = "def f(obj, dyn):\n    for k in [\"a\", \"b\"]:\n        k = dyn\n        getattr(obj, k)\n";
    let inner_loop = "def f(obj, pairs):\n    for k in [\"a\", \"b\"]:\n        for k, v in pairs:\n            getattr(obj, k)\n";
    let comprehension = "def f(obj, names):\n    for k in [\"a\", \"b\"]:\n        out = [getattr(obj, k) for k in names]\n";
    assert_eq!(computed_names(plain), 0, "the loop name is a literal");
    assert_eq!(computed_names(subscript), 0, "a keyed write binds nothing");
    assert_eq!(computed_names(assigned), 1, "an assignment rebinds it");
    assert_eq!(computed_names(inner_loop), 1, "a tuple target rebinds it");
    assert_eq!(computed_names(comprehension), 1, "a clause rebinds it");
}

#[test]
fn deep_plus_chain_marks_concat_without_recursion() {
    const OPERANDS: usize = 20_000;
    let tail = (0..OPERANDS - 1).map(|_| "x + ").collect::<String>();
    let source = format!("def f(x):\n    g('s' + {tail}x)\n    h({tail}x)\n");
    let analysis = facts_of(&source);
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes: Vec<ArgShape> = analysis
        .facts
        .calls
        .iter()
        .map(|c| {
            c.args
                .first()
                .expect("call takes one argument")
                .shape
                .clone()
        })
        .collect();
    assert_eq!(shapes.len(), 2, "two calls, got {shapes:?}");
    assert_eq!(
        shapes[0],
        ArgShape::Concat { longest_static: 1 },
        "string at the bottom of a deep chain is found"
    );
    assert_eq!(
        shapes[1],
        ArgShape::Other,
        "deep identifier-only chain stays Other"
    );
}

#[test]
fn operator_spelling_inside_operands_is_not_top_eq_ne() {
    let analysis =
        facts_of("assert \"!=\" in flags\nassert names.count(\"==\") < 2\nassert a != b\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.assertions.len(),
        3,
        "three assertions, got {:?}",
        analysis.facts.assertions.len()
    );
    assert_eq!(
        analysis.facts.assertions[0].shape,
        AssertShape::Other,
        "in with operator spelling in the operand is Other"
    );
    assert_eq!(
        analysis.facts.assertions[1].shape,
        AssertShape::Other,
        "less-than with operator spelling in the operand is Other"
    );
    assert_eq!(
        analysis.facts.assertions[2].shape,
        AssertShape::TopEqNe,
        "not-equal operator stays TopEqNe"
    );
}

#[test]
fn canonical_repo_labels_resolve_to_single_root() {
    let analysis = facts_of(
        "load('@@//pkg:defs.bzl', 'x')\nload('@@rules_go//go:def.bzl', 'y')\nload('@rules_go//go:def.bzl', 'z')\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let roots: Vec<Option<&str>> = analysis
        .facts
        .imports
        .iter()
        .map(|i| i.external_root)
        .collect();
    assert_eq!(
        roots,
        vec![None, Some("rules_go"), Some("rules_go")],
        "main load is first-party and both spellings count one root"
    );
}
