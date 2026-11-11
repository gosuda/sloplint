//! Java call-site facts: switch defaults, line-broken receivers, entry points, concat runs, closure initializers, and ambient writes.

use std::sync::LazyLock;

use sloplint_core::facts::ArgShape;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Java,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn nested_switch_default_is_found() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run(int x, int y) {\n    switch (x) {\n      case 1 -> runOne();\n      default -> other();\n    }\n    switch (y) {\n      case 2 -> runTwo();\n      default -> stop();\n    }\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.wildcard_arms.len(),
        2,
        "both switch defaults are arms"
    );
}

#[test]
fn line_broken_receiver_keeps_qualifier() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run() {\n    logger\n      .error(\"boom\");\n    helper.render(1);\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let broken = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee.contains("error") && c.callee.contains("logger"))
        .expect("has line-broken logger call");
    assert!(broken.is_simple_path, "line break keeps the simple path");
    assert_eq!(
        broken.callee_root,
        Some("logger"),
        "root stays the receiver"
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "helper.render")
        .expect("has plain qualified call");
    assert_eq!(
        plain.callee, "helper.render",
        "single-line qualifier is kept"
    );
}

#[test]
fn string_like_array_params_are_not_entry_points() {
    let analysis = run(
        "Case.java",
        "public class App {\n  public static void main(String[] args) {}\n  public static void main(java.util.List<String[]> rows) {}\n  public static void main(StringBuilder[] sb) {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let entries = analysis
        .facts
        .functions
        .iter()
        .filter(|f| f.name == "main" && f.entry_point)
        .count();
    assert_eq!(entries, 1, "only the String array main is an entry point");
}

#[test]
fn concat_carries_longest_static_run() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run(String name) {\n    log(\"hello world\" + name);\n    log(\"hi \" + name);\n    log(\"\" + name);\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mut runs: Vec<u32> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "log")
        .filter_map(|c| {
            c.args.first().and_then(|arg| match arg.shape {
                ArgShape::Concat { longest_static } => Some(longest_static),
                _ => None,
            })
        })
        .collect();
    runs.sort_unstable();
    assert_eq!(runs, vec![0, 3, 11], "empty, short and long static runs");
}
#[test]
fn array_initializer_with_closure_is_code_body() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run() {\n    compute(new Object[] { () -> 1, () -> 2 });\n    compute(new int[] { 1, 2, 3 });\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let body = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.callee == "compute" && c.args.iter().any(|a| matches!(a.shape, ArgShape::CodeBody))
        })
        .expect("has closure initializer call");
    assert!(
        body.args
            .iter()
            .any(|a| matches!(a.shape, ArgShape::CodeBody)),
        "closure in collection is code"
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.callee == "compute"
                && c.args
                    .iter()
                    .any(|a| matches!(a.shape, ArgShape::Collection))
        })
        .expect("has plain initializer call");
    assert!(
        plain
            .args
            .iter()
            .any(|a| matches!(a.shape, ArgShape::Collection)),
        "plain initializer stays collection"
    );
}

#[test]
fn concat_parenthesized_operand_counts() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run(String name) {\n    log.info((\"Processing \" + name) + \" now\");\n    log.info(\"ab\" + name);\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let parenthesized = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.args
                .first()
                .is_some_and(|arg| matches!(arg.shape, ArgShape::Concat { longest_static: 11 }))
        })
        .expect("has parenthesized long static run");
    assert!(
        parenthesized
            .args
            .first()
            .is_some_and(|arg| matches!(arg.shape, ArgShape::Concat { longest_static: 11 })),
        "parens do not hide the static run"
    );
    let short = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.args
                .first()
                .is_some_and(|arg| matches!(arg.shape, ArgShape::Concat { longest_static: 2 }))
        })
        .expect("has short static run");
    assert!(
        short
            .args
            .first()
            .is_some_and(|arg| matches!(arg.shape, ArgShape::Concat { longest_static: 2 })),
        "short run keeps its length"
    );
}

#[test]
fn only_override_forces_signature() {
    let analysis = run(
        "Case.java",
        "interface Config {\n  void apply(boolean verbose, boolean quiet);\n}\nclass Impl implements Config {\n  @Override\n  public void apply(boolean verbose, boolean quiet) {}\n}\nclass Holder {\n  public final void close(boolean dirty, boolean force) {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let base = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "apply" && f.is_trait_item)
        .expect("has interface declaration");
    assert!(
        !base.signature_forced,
        "interface declaration owns its signature"
    );
    let overrider = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "apply" && !f.is_trait_item)
        .expect("has override implementation");
    assert!(
        overrider.signature_forced,
        "override marker forces the signature"
    );
    let final_method = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "close")
        .expect("has final method");
    assert!(
        !final_method.signature_forced,
        "final without override stays unforced"
    );
}

#[test]
fn inner_class_write_reaches_outer_static() {
    let analysis = run(
        "Case.java",
        "class Outer {\n  static int count;\n  class Inner {\n    void bump() {\n      count++;\n    }\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let writes: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == sloplint_core::facts::AmbientKind::GlobalWrite && a.name == "count")
        .collect();
    assert_eq!(writes.len(), 1, "inner write joins the outer static");
    let plain = run(
        "Plain.java",
        "class Outer {\n  int count;\n  class Inner {\n    void bump() {\n      count++;\n    }\n  }\n}\n",
    );
    assert!(plain.parse_error.is_none(), "parses");
    let silent: Vec<_> = plain
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == sloplint_core::facts::AmbientKind::GlobalWrite && a.name == "count")
        .collect();
    assert!(silent.is_empty(), "instance field write stays silent");
}
