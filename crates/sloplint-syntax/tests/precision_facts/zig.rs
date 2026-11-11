use std::sync::LazyLock;

use sloplint_core::facts::{AmbientKind, ArgShape, AssertShape, ScrutineeKind, Visibility};
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

#[test]
fn destructured_write_reports_every_global_target() {
    let analysis = facts_of(
        "var total: i32 = 0;\nfn bump() void {\nvar local: i32 = 1;\nlocal, total = .{ 2, 3 };\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "destructuring source parses, got {:?}",
        analysis.parse_error
    );
    assert!(
        analysis.facts.ambient.iter().any(|a| {
            a.kind == AmbientKind::GlobalMutable && a.name == "total" && a.in_function.is_none()
        }),
        "file-scope var is a global mutable"
    );
    let writes: Vec<&str> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .map(|a| a.name)
        .collect();
    assert!(
        writes.contains(&"total"),
        "later destructuring target joins the file-scope var, got {writes:?}"
    );
    assert!(
        !writes.contains(&"local"),
        "function-local targets stay quiet, got {writes:?}"
    );
}

#[test]
fn receiver_call_callee_carries_no_arguments() {
    let analysis = facts_of("pub fn run() void {\n    builder().with(1);\n    pkg.f(2);\n}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let with = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "with")
        .expect("has receiver call with bare callee");
    assert!(
        !with.is_simple_path,
        "receiver rooted in a call is not a simple path"
    );
    assert_eq!(
        with.callee_root, None,
        "receiver rooted in a call has no root"
    );
    let qualified = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "pkg.f")
        .expect("has qualified call with full path");
    assert!(
        qualified.is_simple_path,
        "dotted identifiers stay a simple path"
    );
    assert_eq!(
        qualified.callee_root,
        Some("pkg"),
        "qualified call keeps its root, got {:?}",
        qualified.callee_root
    );
}

#[test]
fn const_local_shadows_file_global_write() {
    let analysis = facts_of(
        "var total: u32 = 0;\nfn f() void {\n    const total: u32 = 1;\n    total = 2;\n}\nfn g() void {\n    total = 3;\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert!(
        analysis.facts.ambient.iter().any(|a| {
            a.kind == AmbientKind::GlobalMutable && a.name == "total" && a.in_function.is_none()
        }),
        "file-scope var is a global mutable"
    );
    assert_eq!(
        analysis
            .facts
            .bindings
            .iter()
            .filter(|b| b.name == "total")
            .count(),
        0,
        "file-scope declarations and const locals create no bindings"
    );
    let writes: Vec<u32> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite && a.name == "total")
        .map(|a| a.pos.line)
        .collect();
    assert_eq!(
        writes,
        vec![7],
        "only the unshadowed write reports, got {writes:?}"
    );
}

#[test]
fn mixed_header_destructure_reports_plain_target() {
    let analysis = facts_of(
        "var total: u32 = 0;\nfn f() void {\n    total, var x = .{ 1, 2 };\n    _ = x;\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let writes: Vec<&str> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .map(|a| a.name)
        .collect();
    assert!(
        writes.contains(&"total"),
        "plain target before the header reports, got {writes:?}"
    );
    assert!(
        !writes.contains(&"x"),
        "header-bound name stays quiet, got {writes:?}"
    );
}

#[test]
fn concat_reports_longest_static_run() {
    let analysis = facts_of(
        "pub fn run(name: []const u8) void {\n    f(\"abcdefghijklmnopqrstuvwxyz0123456789\" ++ name);\n    f(\"hi!\" ++ name);\n    f(\"\" ++ name);\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let runs: Vec<u32> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "f")
        .map(|c| match c.args[0].shape {
            ArgShape::Concat { longest_static } => longest_static,
            _ => u32::MAX,
        })
        .collect();
    assert_eq!(
        runs,
        vec![36, 3, 0],
        "36-char, 3-char and empty static runs, got {runs:?}"
    );
}

#[test]
fn keyed_literal_is_other_while_array_stays_collection() {
    let analysis = facts_of(
        "pub fn run() void {\n    f(Point{ .x = 1, .y = 2 });\n    f([_]u32{ 1, 2 });\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shapes: Vec<ArgShape> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "f")
        .map(|c| c.args[0].shape.clone())
        .collect();
    assert_eq!(
        shapes,
        vec![ArgShape::Other, ArgShape::Collection],
        "keyed record is Other, positional array is Collection, got {shapes:?}"
    );
}

#[test]
fn collection_holding_block_is_code_body() {
    let analysis = facts_of("pub fn run() void {\n    f([_]u32{ { 1; }, 2 });\n}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let shape = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "f")
        .map(|c| c.args[0].shape.clone())
        .expect("has call f");
    assert_eq!(
        shape,
        ArgShape::CodeBody,
        "array holding a block is CodeBody, got {shape:?}"
    );
}

#[test]
fn multi_header_registers_every_name() {
    let analysis = facts_of(
        "var total: u32 = 0;\nfn f() void {\n    var a, var total = .{ 1, 2 };\n    _ = a;\n    total = 3;\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let writes: Vec<&str> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .map(|a| a.name)
        .collect();
    assert!(
        !writes.contains(&"total"),
        "second header name shadows the file var, got {writes:?}"
    );
}

#[test]
fn post_header_plain_target_reports() {
    let analysis = facts_of(
        "var total: u32 = 0;\nfn f() void {\n    var x, total = .{ 1, 2 };\n    _ = x;\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let writes: Vec<&str> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .map(|a| a.name)
        .collect();
    assert!(
        writes.contains(&"total"),
        "plain target after a header reports, got {writes:?}"
    );
    assert!(
        !writes.contains(&"x"),
        "header-bound name stays quiet, got {writes:?}"
    );
}

#[test]
fn plain_reassignment_does_not_initialize_binding() {
    let analysis =
        facts_of("var buf: u32 = 0;\nfn f() void {\n    buf = make();\n    const x = make();\n}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let inits: Vec<bool> = analysis
        .facts
        .calls
        .iter()
        .filter(|c| c.callee == "make")
        .map(|c| c.initializes_binding)
        .collect();
    assert_eq!(
        inits,
        vec![false, true],
        "reassignment initializes nothing, const declaration does, got {inits:?}"
    );
}

#[test]
fn reassignment_writes_reach_the_binding() {
    let analysis = facts_of(
        "pub fn bump(seed: u32) u32 {\n    var scratch: u32 = seed;\n    scratch = scratch + 1;\n    scratch = scratch + 1;\n    return scratch;\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let binding = analysis
        .facts
        .bindings
        .iter()
        .find(|b| b.name == "scratch")
        .expect("has binding scratch");
    assert_eq!(
        binding.write_lines, 2,
        "two reassignments recorded, got {:?}",
        binding.write_lines
    );
    assert_eq!(
        binding.last_write_line - binding.first_write_line,
        1,
        "writes span lines 3-4, got {:?}",
        binding.last_write_line - binding.first_write_line
    );
}

#[test]
fn param_name_shadows_file_global() {
    let analysis = facts_of(
        "var total: u32 = 0;\nfn f(total: u32) u32 {\n    total = 1;\n    return total;\n}\nfn g() void {\n    total = 2;\n}\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let writes: Vec<u32> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite && a.name == "total")
        .map(|a| a.pos.line)
        .collect();
    assert_eq!(
        writes,
        vec![7],
        "param write stays local, only the free write reports, got {writes:?}"
    );
}

#[test]
fn keyed_struct_literal_reports_field_copy_source() {
    let analysis = facts_of(include_str!(
        "../../../sloplint/tests/fixtures/slop/field-copy__zig/src/convert.zig"
    ));
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let copy = analysis
        .facts
        .field_copies
        .iter()
        .find(|c| c.source == Some("p"))
        .expect("has field copy sourced from p");
    assert_eq!(copy.fields, 4, "four copied fields, got {:?}", copy.fields);
}
