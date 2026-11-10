use std::sync::LazyLock;

use sloplint_core::facts::{ArgShape, AssertShape, Visibility};
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
    assert!(
        shape("a.b.c"),
        "member-joined identifiers are a simple path"
    );
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

#[test]
fn receiver_call_callee_has_no_argument_text() {
    let analysis =
        facts_of("def f(ctx, x, name):\n    get_repo(ctx).name(x)\n    helpers.render(name)\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let outer = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "name")
        .expect("outer call keeps only the member name");
    assert!(
        !outer.is_simple_path,
        "receiver rooted in a call is not a simple path"
    );
    assert!(
        outer.callee_root.is_none(),
        "receiver rooted in a call has no root"
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "helpers.render")
        .expect("plain member call keeps its path");
    assert!(
        plain.is_simple_path,
        "identifier-joined callee is a simple path"
    );
    assert_eq!(
        plain.callee_root,
        Some("helpers"),
        "plain member call roots at the leftmost identifier"
    );
    assert!(
        analysis.facts.calls.iter().all(|c| !c.callee.contains('(')),
        "no callee keeps argument text"
    );
}

#[test]
fn deep_member_chain_is_simple_path() {
    let analysis = facts_of("def f(name, x):\n    pkg.mod.render(name)\n    items[0].strip(x)\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let deep = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "pkg.mod.render")
        .expect("two-level member call is recorded");
    assert!(deep.is_simple_path, "two member selects stay a simple path");
    assert_eq!(
        deep.callee_root,
        Some("pkg"),
        "deep chain roots at the leftmost identifier"
    );
    let indexed = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "strip")
        .expect("index-rooted call keeps only the member name");
    assert!(
        !indexed.is_simple_path,
        "index in the chain is not a simple path"
    );
    assert!(
        indexed.callee_root.is_none(),
        "index-rooted chain has no root"
    );
}

#[test]
fn left_nested_concat_marks_concat() {
    let analysis = facts_of(
        "def f(name, other, suffix):\n    fail('Missing target ' + name + suffix)\n    run(name + other)\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let fail_arg = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "fail")
        .expect("fail call is recorded")
        .args
        .first()
        .expect("fail takes the concatenation")
        .shape
        .clone();
    assert_eq!(
        fail_arg,
        ArgShape::Concat { longest_static: 15 },
        "left-nested string chain is a concatenation"
    );
    let plain_arg = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "run")
        .expect("run call is recorded")
        .args
        .first()
        .expect("run takes the addition")
        .shape
        .clone();
    assert_eq!(
        plain_arg,
        ArgShape::Other,
        "identifier-only addition stays Other"
    );
}

#[test]
fn non_equality_comparison_is_other() {
    let analysis = facts_of("assert len(items) < 3\nassert a != b\n");
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
        AssertShape::Other,
        "less-than is not TopEqNe"
    );
    assert!(
        analysis.facts.assertions[0].comparison,
        "less-than still marks comparison"
    );
    assert_eq!(
        analysis.facts.assertions[1].shape,
        AssertShape::TopEqNe,
        "not-equal stays TopEqNe"
    );
}

#[test]
fn star_separator_marks_keyword_only() {
    let analysis = facts_of(
        "def f(a, *, verbose = False):\n    return a\ndef g(first, *rest):\n    return first\n",
    );
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
        "f takes two params, got {:?}",
        fun.params.len()
    );
    assert!(
        !fun.params[0].keyword_only,
        "parameter before the separator stays positional"
    );
    assert!(
        fun.params[1].keyword_only,
        "parameter after bare star is keyword-only"
    );
    let other = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "g")
        .expect("has function g");
    assert_eq!(
        other.params.len(),
        2,
        "g takes two params, got {:?}",
        other.params.len()
    );
    assert!(
        other.params[1].is_variadic,
        "splat parameter stays variadic"
    );
    assert!(
        !other.params[1].keyword_only,
        "the splat itself is not keyword-only"
    );
}

#[test]
fn repo_load_marks_external_root() {
    let analysis =
        facts_of("load('@rules_go//go:def.bzl', 'go_library')\nload('//pkg:file.bzl', 'sym')\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let external = analysis
        .facts
        .imports
        .iter()
        .find(|i| i.module == "@rules_go//go:def.bzl")
        .expect("repo load is recorded");
    assert_eq!(
        external.external_root,
        Some("rules_go"),
        "at-sign label names its repository root"
    );
    let local = analysis
        .facts
        .imports
        .iter()
        .find(|i| i.module == "//pkg:file.bzl")
        .expect("workspace load is recorded");
    assert!(
        local.external_root.is_none(),
        "workspace-absolute load is first-party"
    );
}

#[test]
fn helpers_file_in_tests_dir_is_test() {
    let source = "def make_flags(a = False, b = False):\n    return a\n";
    let in_tests = facts_at("tests/helpers.bzl", source);
    assert!(
        in_tests.parse_error.is_none(),
        "parses, got {:?}",
        in_tests.parse_error
    );
    let flagged = in_tests
        .facts
        .functions
        .iter()
        .find(|f| f.name == "make_flags")
        .expect("has function make_flags");
    assert!(flagged.in_test, "function under tests/ is in test");
    let library = facts_at("pkg/helpers.bzl", source);
    assert!(
        library.parse_error.is_none(),
        "parses, got {:?}",
        library.parse_error
    );
    let plain = library
        .facts
        .functions
        .iter()
        .find(|f| f.name == "make_flags")
        .expect("has function make_flags");
    assert!(!plain.in_test, "same file outside tests/ is library code");
}
