//! Export surfaces, callee chains, and destructuring writes for JavaScript sources.

use std::sync::LazyLock;

use sloplint_core::facts::{ArgShape, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::JavaScript,
        path: "src/app.js",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn receiver_call_callee_drops_to_member() {
    let analysis = facts_of("ctx.log().error('x');\na.b.c('y');\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let collapsed = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "error")
        .expect("has the collapsed call");
    assert!(
        !collapsed.is_simple_path,
        "a call receiver is not a simple path"
    );
    assert!(
        collapsed.callee_root.is_none(),
        "a collapsed chain has no root, got {:?}",
        collapsed.callee_root
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "a.b.c")
        .expect("has the plain call");
    assert!(
        plain.is_simple_path,
        "a plain member chain stays a simple path"
    );
    assert_eq!(plain.callee_root, Some("a"), "a plain chain keeps its root");
}

#[test]
fn mixed_and_parenthesized_chains_measure_every_static_leaf() {
    let analysis = facts_of(
        "logger.info(total - skipped + ' files were skipped');\nconsole.error('err: ' + (msg + ' while connecting to the database'));\nlogger.info(done - 1);\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.calls.len(),
        3,
        "three calls, got {:?}",
        analysis.facts.calls.len()
    );
    assert_eq!(
        analysis.facts.calls[0].args[0].shape,
        ArgShape::Concat { longest_static: 19 },
        "non-plus subtree is a leaf, static tail still measured"
    );
    assert_eq!(
        analysis.facts.calls[1].args[0].shape,
        ArgShape::Concat { longest_static: 33 },
        "parenthesized right chain contributes its run"
    );
    assert_eq!(
        analysis.facts.calls[2].args[0].shape,
        ArgShape::Other,
        "static-free chain stays Other"
    );
}
#[test]
fn paren_and_index_links_are_not_simple_paths() {
    let analysis = facts_of("(render)(n);\n(addr => addr)();\na.b();\na[0]();\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.calls.len(),
        4,
        "four calls, got {:?}",
        analysis.facts.calls.len()
    );
    assert!(
        !analysis.facts.calls[0].is_simple_path,
        "parenthesized callee is not a simple path"
    );
    assert!(
        !analysis.facts.calls[1].is_simple_path,
        "call receiver is not a simple path"
    );
    assert_eq!(
        analysis.facts.calls[2].callee_root,
        Some("a"),
        "pure member chain keeps its root"
    );
    assert!(
        analysis.facts.calls[2].is_simple_path,
        "pure member chain stays a simple path"
    );
    assert_eq!(
        analysis.facts.calls[3].callee_root, None,
        "index link clears the root"
    );
    assert!(
        !analysis.facts.calls[3].is_simple_path,
        "index link is not a simple path"
    );
}

#[test]
fn destructured_assignment_counts_as_a_write() {
    let analysis = facts_of(
        "function f(n) {\n  let a = 0;\n  a = 1;\n  [a] = next(n);\n  let b = 0;\n  b = 2;\n  return a + b;\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let binding = |want: &str| {
        analysis
            .facts
            .bindings
            .iter()
            .find(|binding| binding.name == want)
            .unwrap_or_else(|| panic!("has binding {want}"))
    };
    assert_eq!(binding("a").write_lines, 2, "destructuring write counts");
    assert_eq!(
        binding("a").last_write_line,
        4,
        "destructuring write moves the last write line"
    );
    assert_eq!(
        binding("b").write_lines,
        1,
        "plain write count stays unchanged"
    );
}

#[test]
fn anonymous_exported_closures_keep_exported_names() {
    let analysis = facts_of(
        "export default function (a = false, b = false, c = false) { return a; }\nmodule.exports = function parse(src, a = false, b = false, c = false) { return src; }\nexports.run = (a = false, b = false, c = false) => a;\nconst hidden = (a = false) => a;\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let visibility = |want: &str| {
        analysis
            .facts
            .functions
            .iter()
            .find(|function| function.name == want)
            .unwrap_or_else(|| panic!("has function {want}"))
            .visibility
    };
    assert_eq!(
        visibility("default"),
        Visibility::Exported,
        "anonymous default export stays exported with a groupable name"
    );
    assert_eq!(
        visibility("parse"),
        Visibility::Exported,
        "module.exports keeps the function name exported"
    );
    assert_eq!(
        visibility("run"),
        Visibility::Exported,
        "exports key names the arrow exported"
    );
    assert_eq!(
        visibility("hidden"),
        Visibility::Private,
        "non-exported anonymous closure stays private"
    );
    assert!(
        analysis
            .facts
            .functions
            .iter()
            .all(|function| !function.name.is_empty()),
        "no exported head keeps an empty name"
    );
}

#[test]
fn default_exported_class_method_stays_exported() {
    let analysis = facts_of(
        "export default class { probe(a = false, b = false, c = false) { return a; } }\nclass Plain { other() {} }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let visibility = |want: &str| {
        analysis
            .facts
            .functions
            .iter()
            .find(|function| function.name == want)
            .unwrap_or_else(|| panic!("has function {want}"))
            .visibility
    };
    assert_eq!(
        visibility("probe"),
        Visibility::Exported,
        "default-exported class exposes its method"
    );
    assert_eq!(
        visibility("other"),
        Visibility::Private,
        "plain class method stays private"
    );
}

#[test]
fn object_destructuring_assignment_counts_every_name() {
    let analysis = facts_of(
        "function f(obj) {\n  let a = 0;\n  let b = 0;\n  a = 1;\n  ({a, b} = obj);\n  let c = 0;\n  c = 1;\n  return a + b + c;\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let binding = |want: &str| {
        analysis
            .facts
            .bindings
            .iter()
            .find(|binding| binding.name == want)
            .unwrap_or_else(|| panic!("has binding {want}"))
    };
    assert_eq!(binding("a").write_lines, 2, "object pattern write counts");
    assert_eq!(
        binding("a").last_write_line,
        5,
        "object pattern write moves the last write line"
    );
    assert_eq!(
        binding("b").write_lines,
        1,
        "the destructuring is the only write to b"
    );
    assert_eq!(
        binding("c").write_lines,
        1,
        "plain write count stays unchanged"
    );
}
