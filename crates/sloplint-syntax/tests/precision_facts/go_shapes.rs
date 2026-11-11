use std::sync::LazyLock;

use sloplint_core::facts::ArgShape;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> sloplint_syntax::Analysis<'_> {
    let input = FileInput {
        lang: Lang::Go,
        path: "pkg/sample.go",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn concat_carries_longest_static_run() {
    let analysis = facts_of(
        "package sample\nfunc f(name string) { g(\"hello world \" + name) }\nfunc h(name string) { g(\"hi \" + name) }\nfunc z(name string) { g(\"\" + name) }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "g")
        .map(|c| c.args[0].shape.clone())
        .collect();
    assert_eq!(
        shapes,
        [
            ArgShape::Concat { longest_static: 12 },
            ArgShape::Concat { longest_static: 3 },
            ArgShape::Concat { longest_static: 0 },
        ],
        "long, short, and empty static runs are measured"
    );
}

#[test]
fn keyed_literal_is_not_a_collection() {
    let analysis = facts_of(
        "package sample\nfunc f(w int) { render(Config{Width: w}) }\nfunc g() { render([]int{1, 2}) }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "render")
        .map(|c| c.args[0].shape.clone())
        .collect();
    assert_eq!(
        shapes,
        [ArgShape::Other, ArgShape::Collection],
        "a keyed literal is Other while a slice stays Collection"
    );
}

#[test]
fn closure_in_collection_is_code_body() {
    let analysis = facts_of(
        "package sample\nfunc f() { run([]func(){ func() { println(\"x\") } }) }\nfunc g() { run([]int{1, 2}) }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes: Vec<_> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "run")
        .map(|c| c.args[0].shape.clone())
        .collect();
    assert_eq!(
        shapes,
        [ArgShape::CodeBody, ArgShape::Collection],
        "a collection holding a closure is CodeBody"
    );
}
