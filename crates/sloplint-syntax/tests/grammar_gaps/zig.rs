//! Zig source that the published tree-sitter-zig 1.1.2 grammar rejected.
//!
//! The vendored grammar under `vendor/tree-sitter-zig` accepts each repro
//! below. The published crate reports a parse error for each repro. The
//! controls and the invalid literals are the same against both grammars, so
//! a change in them shows up as a regression. Shapes that the 0.14 compiler
//! rejects but the master stdlib uses are accepted on purpose; SLOPLINT.md
//! documents that over-acceptance.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Zig,
        path: "gap.zig",
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_zig::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("zig language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let root = tree.root_node();
    assert!(
        !root.has_error() && !root.to_sexp().contains("MISSING"),
        "raw tree must be error-free, sexp: {}",
        root.to_sexp()
    );
}

fn run_clean(source: &str) -> Analysis<'_> {
    assert_raw_clean(source);
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_none(),
        "valid Zig must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn run_error(source: &str) {
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_some(),
        "invalid Zig must still report a parse error:\n{source}"
    );
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

// --- Bare error set as a container field (Zig 0.14 accepts) -------------------

#[test]
fn bare_error_set_container_field_parses() {
    let analysis = run_clean("const S = struct { error{foo} };\n");
    assert!(
        analysis.facts.functions.is_empty(),
        "bare error set field records no functions: {:?}",
        analysis.facts.functions
    );
}

#[test]
fn bare_error_set_field_in_a_comptime_call_parses() {
    run_clean(concat!(
        "test \"type checks\" {\n",
        "    try std.testing.expect(!comptime canParseType(struct { error{foo} }));\n",
        "}\n",
    ));
}

#[test]
fn bare_error_set_field_before_a_named_field_parses() {
    run_clean("const T = struct { error{foo, bar}, x: u8 };\n");
}

// --- Struct-literal asm clobbers (master Zig accepts) -------------------------

#[test]
fn asm_struct_literal_clobbers_parse() {
    let source = concat!(
        "pub fn syscall0(number: u64) u64 {\n",
        "    return asm volatile (\"syscall\"\n",
        "        : [ret] \"={rax}\" (-> u64),\n",
        "        : [number] \"{rax}\" (number),\n",
        "        : .{ .rcx = true, .r11 = true, .memory = true });\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["syscall0"]);
}

#[test]
fn asm_triple_colon_struct_clobber_parses() {
    let analysis = run_clean(
        "pub fn flush() void {\n    asm volatile (\"flushw\" ::: .{ .memory = true });\n}\n",
    );
    assert_eq!(function_names(&analysis), ["flush"]);
}

// --- async and await as identifiers (master Zig accepts) ----------------------

#[test]
fn async_as_enum_tag_parses() {
    run_clean(
        "const UwTable = enum(u32) {\n    none,\n    sync,\n    async,\n    pub const default = UwTable.async;\n};\n",
    );
}

#[test]
fn async_and_await_as_container_field_names_parse() {
    run_clean(concat!(
        "const Io = struct {\n",
        "    async: *const fn (userdata: ?*anyopaque) void,\n",
        "    await: *const fn (userdata: ?*anyopaque) void,\n",
        "};\n",
    ));
}

#[test]
fn async_and_await_as_field_values_parse() {
    run_clean(concat!(
        "const vtable = .{\n",
        "    .async = async,\n",
        "    .await = await,\n",
        "};\n",
    ));
}

// --- Controls that already parsed and must keep parsing -----------------------

#[test]
fn asm_return_type_output_without_clobbers_still_parses() {
    let analysis = run_clean(concat!(
        "pub fn syscall0(number: u64) u64 {\n",
        "    return asm volatile (\"ecall\"\n",
        "        : [ret] \"={x10}\" (-> u64),\n",
        "        : [number] \"{x17}\" (number),\n",
        "    );\n",
        "}\n",
    ));
    assert_eq!(function_names(&analysis), ["syscall0"]);
}

#[test]
fn asm_underscore_output_name_still_parses() {
    run_clean(
        "pub fn read() usize {\n    return asm volatile (\"nop\"\n        : [_] \"={r3}\" (-> usize),\n    );\n}\n",
    );
}

#[test]
fn asm_multiline_template_with_string_clobber_still_parses() {
    run_clean(concat!(
        "pub fn xor() void {\n",
        "    asm volatile (\n",
        "        \\\\ xor %%ecx, %%ecx\n",
        "        \\\\ ret\n",
        "        ::: \"memory\");\n",
        "}\n",
    ));
}

// --- Invalid input that must keep failing --------------------------------------

#[test]
fn float_literal_with_a_suffix_still_errors() {
    run_error("const x = 1e1f;\n");
}

#[test]
fn other_invalid_literals_still_error() {
    run_error("const x = 1e1x;\n");
    run_error("const x = 0b102;\n");
    run_error("const x = 0o8;\n");
    run_error("const x = 1__0;\n");
    run_error("const x = 1.0e;\n");
}

#[test]
fn bare_braced_asm_clobber_with_no_field_still_errors() {
    run_error("pub fn f() void {\n    asm volatile (\"nop\"\n        :: .{ foo });\n}\n");
}
