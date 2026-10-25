//! Valid Swift that the pinned tree-sitter-swift 0.7.4 grammar rejects.
//!
//! Every repro here is valid input for `swiftc -parse` (Swift 6.4.1). The vendored
//! grammar under `vendor/tree-sitter-swift` accepts it; the published crate reported
//! a parse error for each one (proven by swapping the dependency to the registry
//! version). The control tests at the bottom pin inputs that stay errors and inputs
//! that already parsed, so the grammar cannot drift toward accepting invalid code
//! or regress existing behavior.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Swift,
        path,
        source,
        is_test_file: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn run_clean<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    // Raw-tree verdict first: recovery can insert MISSING nodes that are invisible to
    // cursor walks, so an `analyze`-only check would pass them as clean (false green).
    // An error-to-hidden-MISSING swap is not a fix.
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .expect("swift language loads");
    let tree = parser.parse(source, None).expect("source parses to a tree");
    assert!(
        !tree.root_node().has_error(),
        "{path} must parse cleanly on the raw tree"
    );
    assert!(
        !tree.root_node().to_sexp().contains("MISSING"),
        "{path} raw tree must not hide recovery nodes"
    );
    let analysis = run(path, source);
    assert!(
        analysis.parse_error.is_none(),
        "{path} must parse cleanly, first error at {:?}",
        analysis.parse_error
    );
    analysis
}

fn function_names<'s>(analysis: &Analysis<'s>) -> Vec<&'s str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

fn callees<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.calls.iter().map(|c| c.callee).collect()
}

// SW-VOID: `()` as a value expression.
#[test]
fn void_unit_values_parse() {
    run_clean("void_let.swift", "let x = ()\n");
    let a = run_clean("void_ret.swift", "func f() -> () {\n  return ()\n}\n");
    assert!(
        function_names(&a).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&a)
    );
    let b = run_clean(
        "void_call.swift",
        "func g(_ x: ()) {}\nfunc f() {\n  g(())\n}\n",
    );
    assert!(
        function_names(&b).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&b)
    );
    assert!(
        callees(&b).contains(&"g"),
        "void call records callee g: {:?}",
        callees(&b)
    );
    run_clean(
        "void_case.swift",
        "enum E {\n  case a\n}\nfunc f(_ e: E) {\n  switch e {\n  case .a:\n    ()\n  default:\n    break\n  }\n}\n",
    );
    run_clean("void_map.swift", "let y = [1].map { _ in () }\n");
}

// SW-LIFETIME: `@_lifetime` compiler attributes.
#[test]
fn lifetime_attributes_parse() {
    let a = run_clean(
        "life_borrow.swift",
        "struct S {\n  @_lifetime(borrow self)\n  func f() {}\n}\n",
    );
    assert!(
        function_names(&a).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&a)
    );
    run_clean(
        "life_copy.swift",
        "struct S {\n  @_lifetime(self: copy self)\n  func f() {}\n}\n",
    );
    run_clean(
        "life_dep.swift",
        "struct S {\n  @_lifetime(dependent: borrow source)\n  func f(_ source: Int) {}\n}\n",
    );
}

// SW-OWN: legacy `__consuming`/`__owned`/`__shared` ownership modifiers.
#[test]
fn legacy_ownership_modifiers_parse() {
    run_clean(
        "own_consuming.swift",
        "struct S {\n  __consuming func f() {}\n}\n",
    );
    run_clean(
        "own_owned.swift",
        "struct S {\n  func f(_ x: __owned some Collection<Int>) {}\n}\n",
    );
    run_clean(
        "own_shared.swift",
        "struct S {\n  init(_ r: __shared S) {}\n}\n",
    );
}

// SW-RUNTIME: `_runtime(...)` inside a compound `#if` condition.
#[test]
fn runtime_condition_parses() {
    run_clean(
        "runtime.swift",
        "#if (compiler(<6.1) && !os(WASI)) || (compiler(>=6.1) && _runtime(_multithreaded))\nlet x = 1\n#endif\n",
    );
}

// SW-UNSAFE: `unsafe` binary expressions in conditions.
#[test]
fn unsafe_binary_condition_parses() {
    let a = run_clean(
        "unsafe_if.swift",
        "func f(_ a: Int, _ b: Int) -> Bool {\n  if unsafe a == b {\n    return true\n  }\n  return false\n}\n",
    );
    assert!(
        function_names(&a).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&a)
    );
}

// SW-ASSOCWHERE: `where` on typealiases, multi-bound associatedtypes, multiline
// dotted equality constraints.
#[test]
fn associatedtype_requirements_parse() {
    run_clean(
        "assoc_where.swift",
        "public typealias RangeType<Bound> = Range<Bound> where Bound: Strideable, Bound.Stride: SignedInteger\n",
    );
    run_clean(
        "assoc_multi.swift",
        "protocol P {\n  associatedtype I: Equatable, Comparable, Hashable\n}\n",
    );
    run_clean(
        "where_multi.swift",
        "protocol P {\n  associatedtype A: Sendable\n  associatedtype B: Sendable\n  where\n    A.C == B,\n    A.D == B\n}\n",
    );
}

// SW-ATTRCLASS: attributes wrapped around `#if` blocks.
#[test]
fn attributes_around_directives_parse() {
    run_clean(
        "attr_if.swift",
        "#if !DEBUG\n@exclusivity(unchecked)\n#endif\n@usableFromInline var _capacity: Int = 0\n",
    );
    let a = run_clean(
        "attr_wrap.swift",
        "@available(macOS 10.15, *)\n#if compiler(>=6.2)\n@available(*, deprecated)\n#endif\n@inlinable\nfunc f() {}\n",
    );
    assert!(
        function_names(&a).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&a)
    );
}

// SW-ATTRCLASS: `nonisolated(unsafe)` in function bodies.
#[test]
fn local_nonisolated_parses() {
    let a = run_clean(
        "noniso_local.swift",
        "func testF() {\n  nonisolated(unsafe) var x = 1\n  print(x)\n}\n",
    );
    assert!(
        function_names(&a).contains(&"testF"),
        "function testF recorded: {:?}",
        function_names(&a)
    );
}

// Case ranges over parenthesized and implicit-member bounds.
#[test]
fn case_range_patterns_parse() {
    run_clean(
        "range_paren.swift",
        "func f(_ n: Int) {\n  switch n {\n  case (1 + 2)...10:\n    break\n  default:\n    break\n  }\n}\n",
    );
    run_clean(
        "range_dotmin.swift",
        "func f(_ n: Int) {\n  switch n {\n  case .min..<0:\n    break\n  default:\n    break\n  }\n}\n",
    );
}

// `sending` on function declarations and function types.
#[test]
fn sending_results_parse() {
    let a = run_clean(
        "send_ret.swift",
        "func f() async throws -> sending Int {\n  return 1\n}\n",
    );
    assert!(
        function_names(&a).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&a)
    );
    run_clean(
        "send_type.swift",
        "func f(_ body: () async throws -> sending Int) {}\n",
    );
}

// Space before an explicit semicolon.
#[test]
fn spaced_semicolons_parse() {
    run_clean(
        "semi_space.swift",
        "func f() {\n  let x = 4 ; let y = 5\n  print(x, y)\n}\n",
    );
}

// Legacy addressor accessors.
#[test]
fn addressor_accessors_parse() {
    run_clean(
        "unsafe_addr.swift",
        "struct S {\n  var p: Int = 0\n  var x: Int {\n    unsafeAddress {\n      return p\n    }\n    nonmutating unsafeMutableAddress {\n      return p\n    }\n  }\n}\n",
    );
}

// Invalid input that stays an error: trailing comma in `where`, regex literals
// the compiler also rejects.
#[test]
fn invalid_input_still_errors() {
    let trailing = run(
        "where_trailing.swift",
        "protocol P {\n  associatedtype B where A == B,\n}\n",
    );
    assert!(
        trailing.parse_error.is_some(),
        "trailing comma in where must still error"
    );
    let regex = run(
        "regex.swift",
        "struct P {\n  private let regex = /^]: (\\d+)$/\n}\n",
    );
    assert!(
        regex.parse_error.is_some(),
        "invalid regex literal must still error"
    );
}

// Existing behavior: inputs the published grammar already accepted.
#[test]
fn existing_behavior_keeps_parsing() {
    run_clean(
        "tilde.swift",
        "func f(_ x: Int) -> Int {\n  return (~x).leadingZeroBitCount\n}\n",
    );
    run_clean("package.swift", "package func f() {}\n");
}
