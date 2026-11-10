use std::sync::LazyLock;

use sloplint_core::facts::ArgShape;
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

fn first_arg_shapes<'s>(analysis: &sloplint_syntax::Analysis<'s>) -> Vec<ArgShape<'s>> {
    analysis
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
        .collect()
}

#[test]
fn short_static_run_is_measured() {
    let analysis =
        facts_of("def f(name, other):\n    fail('oops ' + name)\n    run(name + other)\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes = first_arg_shapes(&analysis);
    assert_eq!(shapes.len(), 2, "two calls, got {shapes:?}");
    assert_eq!(
        shapes[0],
        ArgShape::Concat { longest_static: 5 },
        "short static run is measured"
    );
    assert_eq!(shapes[1], ArgShape::Other, "dynamic-only chain stays Other");
}

#[test]
fn interpolated_only_string_is_zero() {
    let analysis =
        facts_of("def f(x, err):\n    log(f\"{x}\")\n    log(f\"oops {err}\")\n    log('hello')\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes = first_arg_shapes(&analysis);
    assert_eq!(shapes.len(), 3, "three calls, got {shapes:?}");
    assert_eq!(
        shapes[0],
        ArgShape::Concat { longest_static: 0 },
        "interpolation without static text measures zero"
    );
    assert_eq!(
        shapes[1],
        ArgShape::Concat { longest_static: 5 },
        "interpolated static run is measured"
    );
    assert_eq!(
        shapes[2],
        ArgShape::Str { longest_static: 5 },
        "plain string stays a string"
    );
}

#[test]
fn keyed_literal_is_not_collection() {
    let analysis = facts_of("def f():\n    my_func({\"a\": 1, \"b\": 2})\n    my_func([1, 2])\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes = first_arg_shapes(&analysis);
    assert_eq!(shapes.len(), 2, "two calls, got {shapes:?}");
    assert_eq!(
        shapes[0],
        ArgShape::Other,
        "keyed literal is not a collection"
    );
    assert_eq!(shapes[1], ArgShape::Collection, "list stays a collection");
}

#[test]
fn collection_with_closure_is_code_body() {
    let analysis = facts_of("def f():\n    my_func([lambda x: x])\n    my_func([1, 2])\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes = first_arg_shapes(&analysis);
    assert_eq!(shapes.len(), 2, "two calls, got {shapes:?}");
    assert_eq!(
        shapes[0],
        ArgShape::CodeBody,
        "list holding a closure is a code body"
    );
    assert_eq!(
        shapes[1],
        ArgShape::Collection,
        "plain list stays a collection"
    );
}

#[test]
fn parenthesized_operands_count_toward_static_run() {
    let analysis = facts_of(
        "def f(name, suffix):\n    fail((\"Missing target name for package \" + name) + suffix)\n    fail(\"x: \" + (\"missing target name for package \" + name))\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes = first_arg_shapes(&analysis);
    assert_eq!(shapes.len(), 2, "two calls, got {shapes:?}");
    assert_eq!(
        shapes[0],
        ArgShape::Concat { longest_static: 32 },
        "string inside parentheses is not Other"
    );
    assert_eq!(
        shapes[1],
        ArgShape::Concat { longest_static: 32 },
        "inner static run is not the outer three chars"
    );
}
