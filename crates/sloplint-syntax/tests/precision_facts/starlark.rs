use std::sync::LazyLock;

use sloplint_core::facts::{AssertShape, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> sloplint_syntax::Analysis<'_> {
    let input = FileInput {
        lang: Lang::Starlark,
        path: "pkg/test.bzl",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn underscore_function_is_private() {
    let analysis = facts_of("def top():\n    return 1\n\ndef _hidden():\n    return 2\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let names: Vec<_> = analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name.to_owned(), f.visibility))
        .collect();
    let visibility = |want: &str| {
        names
            .iter()
            .find(|row| row.0 == want)
            .unwrap_or_else(|| panic!("has function {want}"))
            .1
    };
    assert_eq!(
        visibility("top"),
        Visibility::Exported,
        "public def is exported"
    );
    assert_eq!(
        visibility("_hidden"),
        Visibility::Private,
        "underscore def is private"
    );
}

#[test]
fn default_param_marks_has_default() {
    let analysis = facts_of("def f(a, b = True):\n    return a\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let fun = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "f")
        .expect("has function f");
    assert_eq!(
        fun.params.len(),
        2,
        "two params, got {:?}",
        fun.params.len()
    );
    assert!(!fun.params[0].has_default, "plain param has no default");
    assert!(fun.params[1].has_default, "default param marks has_default");
    assert!(
        fun.params[1].bool_default,
        "True default marks bool_default"
    );
}

#[test]
fn splat_param_marks_variadic() {
    let analysis = facts_of("def g(first, *rest):\n    return first\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let fun = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "g")
        .expect("has function g");
    assert_eq!(
        fun.params.len(),
        2,
        "two params, got {:?}",
        fun.params.len()
    );
    assert!(!fun.params[0].is_variadic, "plain param is not variadic");
    assert!(fun.params[1].is_variadic, "splat param marks is_variadic");
}

#[test]
fn simple_and_chained_callees() {
    let analysis = facts_of("def f():\n    render(1)\n    a.b.c(2)\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shape = |callee: &str| {
        analysis
            .facts
            .calls
            .iter()
            .find(|c| c.callee == callee)
            .unwrap_or_else(|| panic!("has call {callee}"))
            .is_simple_path
    };
    assert!(shape("render"), "bare call is a simple path");
    assert!(!shape("a.b.c"), "chained select is not a simple path");
}

#[test]
fn assignment_call_initializes_binding() {
    let analysis = facts_of("def f():\n    result = compute(1)\n    compute(2)\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let inits: Vec<bool> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "compute")
        .map(|c| c.initializes_binding)
        .collect();
    assert_eq!(inits.len(), 2, "two compute calls, got {inits:?}");
    assert!(inits[0], "assignment call initializes the binding");
    assert!(!inits[1], "bare statement call initializes nothing");
}

#[test]
fn assert_shapes_split_bare_and_comparison() {
    let analysis = facts_of("assert x\nassert x == y\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.assertions.len(),
        2,
        "two assertions, got {:?}",
        analysis.facts.assertions.len()
    );
    assert_eq!(
        analysis.facts.assertions[0].shape,
        AssertShape::BareName,
        "bare name shape"
    );
    assert!(
        !analysis.facts.assertions[0].comparison,
        "bare name is not a comparison"
    );
    assert_eq!(
        analysis.facts.assertions[1].shape,
        AssertShape::TopEqNe,
        "equality shape"
    );
    assert!(
        analysis.facts.assertions[1].comparison,
        "equality marks comparison"
    );
}
