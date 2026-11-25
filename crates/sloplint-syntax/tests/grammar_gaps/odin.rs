//! Odin source that the published tree-sitter-odin 1.3.0 grammar rejected.
//!
//! The vendored grammar under `vendor/tree-sitter-odin` accepts each repro
//! below. The published crate reports a parse error for each repro. The
//! controls and the invalid literals are the same against both grammars, so
//! a change in them shows up as a regression.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Odin,
        path: "gap.odin",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_odin::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("odin language loads");
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
        "valid Odin must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn run_error(source: &str) {
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_some(),
        "invalid Odin must still report a parse error:\n{source}"
    );
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

// --- `do` consequence followed by `;` before else (newport, odinfmt) ----------

#[test]
fn if_do_semicolon_else_parses() {
    let analysis = run_clean(concat!(
        "f :: proc(a: bool) {\n",
        "    if a do g(); else do h()\n",
        "}\n",
    ));
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn if_do_else_if_do_chain_parses() {
    run_clean(concat!(
        "f :: proc(a, b: bool) {\n",
        "    if a do g()\n",
        "    else if b do h(); else do i()\n",
        "}\n",
    ));
}

#[test]
fn when_do_else_when_do_chain_parses() {
    run_clean(concat!(
        "current_platform_string :: proc() -> string {\n",
        "    when ODIN_OS == .Darwin do return \"darwin\"\n",
        "    else when ODIN_OS == .Linux do return \"linux\"\n",
        "    else do return \"unknown\"\n",
        "}\n",
    ));
}

#[test]
fn if_do_without_semicolon_still_parses() {
    run_clean("f :: proc(a: bool) {\n    if a do g()\n    else do h()\n}\n");
}

// --- for loop with initializer and `in` iterator (karl2d) ---------------------

#[test]
fn for_init_in_iterator_parses() {
    let analysis = run_clean(concat!(
        "walk :: proc() {\n",
        "    for it := iter_make(&c); vo, _ in iterate(&it) {\n",
        "        use(vo)\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(function_names(&analysis), ["walk"]);
}

#[test]
fn for_condition_in_expression_still_parses() {
    // `a in b` as a C-style condition must not be eaten by the iterator form.
    run_clean("f :: proc(a, b: int) {\n    for a in b {\n    }\n}\n");
}

// --- Anonymous container literal in call args (gjallarhorn) -------------------

#[test]
fn anonymous_struct_literal_in_call_parses() {
    run_clean(concat!(
        "f :: proc() {\n",
        "    push(struct { a: int }{1})\n",
        "    push(union { a: int, b: f32 }{a = 1})\n",
        "}\n",
    ));
}

// --- `#soa` collection types (speedrun, odin-wasm, sds) ------------------------

#[test]
fn soa_types_parse() {
    run_clean(concat!(
        "A :: #soa[]Particle\n",
        "B :: #soa[64]Particle\n",
        "C :: #soa^Particle\n",
    ));
}

// --- Struct member defaults (struct `x: T = v`) -------------------------------

#[test]
fn struct_member_default_parses() {
    run_clean("Cfg :: struct {\n    retries: int = 3,\n    name: string,\n}\n");
}

// --- `@(a, b = v,)` attribute list trailing comma (darwodin) -------------------

#[test]
fn attribute_list_trailing_comma_parses() {
    run_clean(concat!(
        "@(private, objc_implement, objc_class = \"X\", objc_superclass = NS.Object,)\n",
        "MacOSAppDelegate :: struct {\n    using _: NS.Object,\n}\n",
    ));
}

// --- `#partial switch` after a label (hybrid-type-aware-linting) --------------

#[test]
fn label_partial_switch_parses() {
    run_clean(concat!(
        "visit :: proc(node: ^Node) {\n",
        "    visitors: #partial switch node.kind {\n",
        "    case .Binary:\n",
        "        handle(node)\n",
        "    }\n",
        "}\n",
    ));
}

// --- `auto_cast`/`no_alias` on a defaulted parameter (newport) -----------------

#[test]
fn auto_cast_default_parameter_parses() {
    let analysis = run_clean(concat!(
        "draw :: proc(using ctx: ^Context, auto_cast vertex_count: int, auto_cast first_vertex := 0) {\n",
        "    vk.CmdDraw(command_buffer, u32(vertex_count), 1, u32(first_vertex), 0)\n",
        "}\n",
    ));
    assert_eq!(function_names(&analysis), ["draw"]);
}

// --- `#force_inline` directive leading a call operand (odin-wasm) --------------
//
// The directive-led call parses at statement level and with a plain callee.
// Known gap: `#tag` followed by a member call inside call ARGUMENTS
// (`f(#force_inline gl.g())`) and a `tag` continuing into a binary operator in
// parens/args still error — see SLOPLINT.md in the vendored grammar.

#[test]
fn force_inline_call_operand_parses() {
    run_clean(concat!(
        "attribute_location_int :: proc(program: u32, name: string) -> i32 {\n",
        "    loc := #force_inline gl.GetAttribLocation(program, name)\n",
        "    fast := i32(#force_inline get_loc(program, name))\n",
        "    return loc + fast\n",
        "}\n",
    ));
}

// --- Directive tag as an expression operand (ols `spall.trace`) -----------------

#[test]
fn directive_tag_in_binary_parses() {
    run_clean(concat!(
        "record :: proc(name: string) {\n",
        "    x := #procedure + \" expr\"\n",
        "    log.info(x)\n",
        "}\n",
    ));
}

// --- Matrix literal with keyed elements (odinfmt) ------------------------------

#[test]
fn matrix_keyed_literal_parses() {
    run_clean(concat!(
        "main :: proc() {\n",
        "    foo := #row_major matrix[4, 4]int{4 ..< 8 = 1, 1 ..< 2 = 2}\n",
        "}\n",
    ));
}

// --- `union` tag operand (`union #align(16) {..}`) ----------------------------

#[test]
fn union_declaration_tag_operand_parses() {
    run_clean("U :: union #align(16) {\n    u64,\n    f64,\n}\n");
}

// --- Bare-identifier result types (upstream d2ca8ef regresses them) ----------

#[test]
fn bare_identifier_result_types_still_parse() {
    run_clean(concat!(
        "f :: proc() -> (int, bool) {\n",
        "    return 1, true\n",
        "}\n",
        "S :: struct {\n",
        "    cb: proc(x: int) -> int,\n",
        "}\n",
    ));
}

// --- Controls: still invalid ---------------------------------------------------

#[test]
fn unclosed_block_still_errors() {
    run_error("f :: proc() {\n");
}

#[test]
fn unterminated_paren_still_errors() {
    run_error("f :: proc( {\n}\n");
}
