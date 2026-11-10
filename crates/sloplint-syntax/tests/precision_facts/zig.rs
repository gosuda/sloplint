use std::sync::LazyLock;

use sloplint_core::facts::{AssertShape, ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Zig,
        path: "src/main.zig",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn visibility_of(analysis: &Analysis<'_>, want: &str) -> Visibility {
    analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == want)
        .unwrap_or_else(|| panic!("has function {want}"))
        .visibility
}

#[test]
fn pub_function_is_exported() {
    let analysis = facts_of("pub fn exported() void {}\nfn hidden() void {}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        visibility_of(&analysis, "exported"),
        Visibility::Exported,
        "pub fn is exported"
    );
    assert_eq!(
        visibility_of(&analysis, "hidden"),
        Visibility::Private,
        "plain fn is private"
    );
}

#[test]
fn params_default_to_other_and_plain() {
    let analysis = facts_of("fn f(a: i32, b: bool) void {}\nfn g(...) void {}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let plain = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "f")
        .expect("has function f");
    assert_eq!(
        plain.params.len(),
        2,
        "two params, got {:?}",
        plain.params.len()
    );
    assert!(!plain.params[0].has_default, "plain param has no default");
    assert!(!plain.params[0].is_variadic, "plain param is not variadic");
    let variadic = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "g")
        .expect("has function g");
    assert!(
        variadic.params.iter().any(|p| p.is_variadic),
        "splat marks is_variadic"
    );
}

#[test]
fn const_call_initializes_binding() {
    let analysis = facts_of("const x = foo();\npub fn main() void {\n    foo();\n}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let inits: Vec<bool> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "foo")
        .map(|c| c.initializes_binding)
        .collect();
    assert_eq!(inits.len(), 2, "two foo calls, got {inits:?}");
    assert!(inits[0], "const initializer call initializes the binding");
    assert!(!inits[1], "bare statement call initializes nothing");
}

#[test]
fn imports_stay_module_scope_without_external_root() {
    let analysis = facts_of("const std = @import(\"std\");\nconst foo = @import(\"mylib\");\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let import = |name: &str| {
        analysis
            .facts
            .imports
            .iter()
            .find(|i| i.name == name)
            .unwrap_or_else(|| panic!("has import {name}"))
    };
    assert_eq!(
        import("std").external_root,
        None,
        "std has no external root"
    );
    assert_eq!(
        import("foo").external_root,
        None,
        "zig leaves third-party roots at None"
    );
    assert!(
        import("std").scope == import("foo").scope,
        "both imports share module scope"
    );
}

#[test]
fn assert_shapes_split_bare_and_comparison() {
    let analysis = facts_of(
        "pub fn check(ready: bool, a: u32, b: u32) void {\n    std.debug.assert(ready);\n    std.testing.expect(a == b);\n}\n",
    );
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
fn switch_else_reports_scrutinee_and_variants() {
    let analysis = facts_of(
        "const Color = enum { Red, Green };\npub fn name(c: Color) u8 {\n    return switch (c) {\n        Color.Red => 1,\n        Color.Green => 2,\n        else => 0,\n    };\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.wildcard_arms.len(),
        1,
        "one else arm, got {:?}",
        analysis.facts.wildcard_arms.len()
    );
    let arm = &analysis.facts.wildcard_arms[0];
    assert_eq!(arm.scrutinee, ScrutineeKind::Ident, "identifier scrutinee");
    assert_eq!(
        arm.variant_count, 2,
        "two enum sibling variants, got {:?}",
        arm.variant_count
    );
    assert!(!arm.has_nullable_ctor, "no nullable constructor sibling");
}
