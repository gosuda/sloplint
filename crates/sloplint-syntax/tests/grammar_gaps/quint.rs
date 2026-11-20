//! Representative Quint shapes the adapter reads, pinned through `analyze`
//! plus a raw-tree check.
//!
//! The vendored grammar (`vendor/tree-sitter-quint` at `f3b26e6`, npm
//! `0.33.0`) is verbatim upstream: there is no published-grammar failure
//! fixed here, so every case below must parse cleanly. The controls pin the
//! facts the Quint adapter reads, so a node-shape regression shows up as a
//! changed fact and not only as a new error. `first_error_pos` cannot see
//! hidden MISSING recovery nodes, so each case also asserts a healthy raw
//! tree (`has_error() == false`, no `MISSING`/`ERROR` in the s-expression).

use std::sync::LazyLock;

use sloplint_core::facts::ImportKind;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Quint,
        path: "gap.qnt",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_quint::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("quint language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let root = tree.root_node();
    let sexp = root.to_sexp();
    assert!(
        !root.has_error() && !sexp.contains("MISSING") && !sexp.contains("ERROR"),
        "raw tree must be error-free, sexp: {sexp}"
    );
}

fn run_clean(source: &str) -> Analysis<'_> {
    assert_raw_clean(source);
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_none(),
        "valid Quint must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

#[test]
fn operator_qualifiers_and_typed_params() {
    let source = "module M {\n  pure def add(a: int, b: int): int = a + b\n  action step = all { x' = 1 }\n  temporal prop = true\n  run r = all { step }\n}\n";
    let analysis = run_clean(source);
    let names: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    assert_eq!(names, ["add", "step", "prop", "r"]);
    let add = &analysis.facts.functions[0];
    let params: Vec<&str> = add.params.iter().map(|p| p.name).collect();
    assert_eq!(params, ["a", "b"]);
    assert_eq!(add.return_type, Some("int"));
}

#[test]
fn val_and_lambda_params() {
    let source = "module M {\n  val inc = x => x + 1\n  val apply = (f, v) => f(v)\n}\n";
    let analysis = run_clean(source);
    let mut names: Vec<&str> = analysis.facts.functions.iter().map(|f| f.name).collect();
    names.retain(|name| !name.is_empty());
    assert_eq!(names, ["inc", "apply"]);
    let closures: Vec<Vec<&str>> = analysis
        .facts
        .functions
        .iter()
        .filter(|f| f.name.is_empty())
        .map(|f| f.params.iter().map(|p| p.name).collect())
        .collect();
    assert_eq!(closures, [vec!["x"], vec!["f", "v"]]);
    let callees: Vec<&str> = analysis.facts.calls.iter().map(|c| c.callee).collect();
    assert!(
        callees.contains(&"f"),
        "lambda body call recorded: {callees:?}"
    );
}
#[test]
fn match_arms_report_wildcards() {
    let source = "module M {\n  type T = A(int) | B | C\n  pure def f(v: T): int = match v { A(x) => x | B => 0 | _ => 1 }\n}\n";
    let analysis = run_clean(source);
    assert_eq!(analysis.facts.wildcard_arms.len(), 1);
    let arm = &analysis.facts.wildcard_arms[0];
    assert!(arm.enum_like, "two constructor siblings make it enum-like");
    assert_eq!(arm.variant_count, 2);
}

#[test]
fn match_without_wildcard_stays_silent() {
    let source = "module M {\n  type T = A(int) | B\n  pure def f(v: T): int = match v { A(x) => x | B => 0 }\n}\n";
    let analysis = run_clean(source);
    assert!(analysis.facts.wildcard_arms.is_empty());
}

#[test]
fn if_and_branching_blocks_nest() {
    let source = "module M {\n  var x: int\n  action step = all { any { x' = 1, x' = 2 }, if (x > 0) x' = x + 1 else x' = 0 }\n}\n";
    let analysis = run_clean(source);
    assert!(analysis.facts.functions.iter().any(|f| f.max_depth >= 2));
}

#[test]
fn imports_exports_and_instances() {
    let source = "module Lib {\n  pure val value = 1\n  pure val other = 2\n}\nmodule Use {\n  import Lib.value\n  import Lib.*\n  export Lib.other\n}\n";
    let analysis = run_clean(source);
    let named = analysis
        .facts
        .imports
        .iter()
        .find(|import| import.kind == ImportKind::Named && !import.is_reexport)
        .expect("named import recorded");
    assert_eq!((named.module.as_ref(), named.name), ("Lib", "value"));
    assert!(
        analysis
            .facts
            .imports
            .iter()
            .any(|import| import.kind == ImportKind::Wildcard && !import.is_reexport),
        "wildcard import recorded"
    );
    assert!(
        analysis
            .facts
            .imports
            .iter()
            .any(|import| import.is_reexport && import.name == "other"),
        "re-export recorded"
    );
}

#[test]
fn record_copy_tracks_single_source() {
    let source = "module M {\n  pure def f(r: { a: int, b: int, c: int }): int = r.a + { a: r.a, b: r.b, c: r.c }.a\n}\n";
    let analysis = run_clean(source);
    assert_eq!(analysis.facts.field_copies.len(), 1);
    let copy = &analysis.facts.field_copies[0];
    assert_eq!((copy.fields, copy.source), (3, Some("r")));
}

#[test]
fn state_var_is_global_mutable_state() {
    let source =
        "module M {\n  var balance: int\n  action deposit = all { balance' = balance + 1 }\n}\n";
    let analysis = run_clean(source);
    assert!(
        analysis
            .facts
            .ambient
            .iter()
            .any(|site| site.name == "balance"),
        "module var is global mutable state"
    );
}

#[test]
fn doc_comments_attach() {
    let source =
        "module M {\n  /// Adds two numbers.\n  pure def add(a: int, b: int): int = a + b\n}\n";
    let analysis = run_clean(source);
    let add = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "add")
        .expect("operator recorded");
    assert!(!add.doc.is_empty(), "doc comment attached");
}

#[test]
fn type_aliases_are_typedefs() {
    let source = "module M {\n  type Pair = (int, str)\n  type Hidden\n}\n";
    let analysis = run_clean(source);
    let names: Vec<&str> = analysis.facts.types.iter().map(|t| t.name).collect();
    assert_eq!(names, ["Pair", "Hidden"]);
}
