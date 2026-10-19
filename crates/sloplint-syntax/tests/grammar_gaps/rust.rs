//! Rust grammar-gap regressions: valid constructs that used to report
//! `meta/parse-error` must parse cleanly through the public API.
//!
//! Each case derives from a minimal repro validated against rustc. Cases that
//! rustc rejects are asserted as still-errors, so the grammar cannot drift
//! toward accepting invalid code.

use std::sync::LazyLock;

use sloplint_core::facts::CallKind;
use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Rust,
        path,
        source,
        is_test_file: false,
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("rust language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let root = tree.root_node();
    assert!(
        !root.has_error() && !root.to_sexp().contains("MISSING"),
        "raw tree must be error-free, sexp: {}",
        root.to_sexp()
    );
}

fn run_clean<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    assert_raw_clean(source);
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

// Row 1: `pub type` inside `extern` blocks (wasm-bindgen shape).
#[test]
fn extern_block_pub_type_parses() {
    run_clean(
        "extern_pub_type.rs",
        "extern \"C\" {\n    pub type Foo;\n}\n",
    );
    run_clean(
        "extern_pub_type_attrs.rs",
        concat!(
            "extern \"C\" {\n",
            "    #[wasm_bindgen(extends = Object)]\n",
            "    pub type Foo;\n",
            "    #[wasm_bindgen(method, getter)]\n",
            "    pub fn x(this: &Foo) -> u32;\n",
            "}\n",
        ),
    );
    run_clean(
        "extern_pub_type_nested.rs",
        "mod m {\n    extern \"C\" {\n        pub type Foo;\n    }\n}\n",
    );
    // Existing behavior: a top-level type alias still yields a typedef fact.
    let a = run_clean("top_type_alias.rs", "pub type A = u8;\n");
    assert!(
        a.facts.types.iter().any(|t| t.name == "A" && t.public),
        "public type alias A recorded: {:?}",
        a.facts.types
    );
}

// Row 2: primitive-named macros and paths.
#[test]
fn primitive_named_macro_parses() {
    let a = run_clean("str_macro.rs", "fn f() {\n    let _ = str![\"\"];\n}\n");
    assert!(
        a.facts
            .calls
            .iter()
            .any(|c| c.callee == "str" && c.kind == CallKind::Macro),
        "str![..] must surface as a macro call"
    );
    run_clean("u16_macro.rs", "fn f() {\n    let _ = u16!(..);\n}\n");
    // Existing behavior: an ordinary macro still surfaces with its name.
    let b = run_clean("plain_macro.rs", "fn f() {\n    let _ = foo!(1);\n}\n");
    assert!(
        b.facts
            .calls
            .iter()
            .any(|c| c.callee == "foo" && c.kind == CallKind::Macro),
        "foo!(..) must surface as a macro call: {:?}",
        b.facts.calls
    );
}

// Row 3: the Rust 2015 `try!` macro.
#[test]
fn legacy_try_macro_parses() {
    let a = run_clean("try_macro.rs", "fn f() {\n    try!(foo());\n}\n");
    assert!(
        a.facts
            .calls
            .iter()
            .any(|c| c.callee == "try" && c.kind == CallKind::Macro),
        "try!(..) must surface as a macro call"
    );
}

// Row 4: `where` predicates on the unit type, a function type, and a macro.
#[test]
fn where_predicate_shapes_parse() {
    let a = run_clean("where_unit.rs", "fn f() where (): Tr<T, U> {}\n");
    assert!(
        function_names(&a).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&a)
    );
    run_clean("where_fn.rs", "fn f() where fn(u8): Send {}\n");
    run_clean("where_macro.rs", "fn f<T>() where mac!(T): Send {}\n");
    // Existing behavior: an ordinary where clause still parses.
    let b = run_clean("where_plain.rs", "fn f<T>() where T: Clone {}\n");
    assert!(
        function_names(&b).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&b)
    );
}

// Rows 5 and 6: `~` and lone `$` inside macro token trees.
#[test]
fn token_tree_punctuation_parses() {
    run_clean("tilde_macro.rs", "foo![~];\n");
    run_clean(
        "dollar_rules.rs",
        "macro_rules! m {\n    ([$]) => {};\n    () => { $ crate::Foo };\n}\n",
    );
    // Existing behavior: repetition patterns still parse.
    run_clean(
        "repetition_rules.rs",
        "macro_rules! m {\n    ($($t:tt)*) => { $($t)* };\n}\n",
    );
}

// Row 7: attributes on non-first tuple elements, struct-pattern fields,
// and closure parameters.
#[test]
fn attributes_on_later_items_parse() {
    run_clean(
        "tuple_attr.rs",
        "fn f() {\n    let _ = (1, #[allow(unused)] 2);\n}\n",
    );
    run_clean(
        "pattern_attr.rs",
        "fn f() {\n    let S { #[cfg(test)] a } = s;\n}\n",
    );
    run_clean(
        "closure_attr.rs",
        "fn f() {\n    let _ = |a, #[allow(unused)] b| a;\n}\n",
    );
    // Existing behavior: an attribute on the first element still parses.
    run_clean(
        "tuple_attr_first.rs",
        "fn f() {\n    let _ = (#[cfg(a)] 1, 2);\n}\n",
    );
}

// Row 8: where clause on a field-less struct.
#[test]
fn fieldless_struct_where_parses() {
    let a = run_clean("unit_where.rs", "struct S<T> where T: Send;\n");
    assert!(
        a.facts.types.iter().any(|t| t.name == "S"),
        "unit struct S recorded: {:?}",
        a.facts.types
    );
    // Existing behavior: a tuple struct with a where clause still parses.
    let b = run_clean("tuple_where.rs", "struct S<T>(T) where T: Send;\n");
    assert!(
        b.facts.types.iter().any(|t| t.name == "S"),
        "tuple struct S recorded: {:?}",
        b.facts.types
    );
}

// Row 9: several attributes on one function parameter.
#[test]
fn repeated_param_attributes_parse() {
    let a = run_clean(
        "multi_attr_param.rs",
        "fn f(#[allow(unused)] #[allow(unused)] x: u8) {}\n",
    );
    let f = a
        .facts
        .functions
        .iter()
        .find(|f| f.name == "f")
        .expect("function f must be reported");
    assert_eq!(f.params.iter().map(|p| p.name).collect::<Vec<_>>(), ["x"]);
    // Existing behavior: a single attribute still parses.
    let b = run_clean("single_attr_param.rs", "fn f(#[allow(unused)] x: u8) {}\n");
    assert!(
        function_names(&b).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&b)
    );
}

// Row 10: negative literals as const generic arguments.
#[test]
fn negative_const_generic_parses() {
    run_clean("neg_generic.rs", "fn f() {\n    let _: R<-5, 5>;\n}\n");
    // Existing behavior: a plain literal argument still parses.
    run_clean("pos_generic.rs", "fn f() {\n    let _: R<5>;\n}\n");
}

// Row 11: underscore separators in unicode escapes.
#[test]
fn unicode_escape_separators_parse() {
    run_clean("escape_sep.rs", "fn f() {\n    let _ = '\\u{0__}';\n}\n");
    // Existing behavior: a plain unicode escape still parses.
    run_clean("escape_plain.rs", "fn f() {\n    let _ = '\\u{0}';\n}\n");
}

// Row 13: reserved words as const and static names.
#[test]
fn reserved_const_names_parse() {
    run_clean("const_default.rs", "pub const default: u8 = 0;\n");
    run_clean(
        "assoc_const_raw.rs",
        "impl S {\n    const raw: u8 = 1;\n}\n",
    );
    // Existing behavior: an ordinary const name still parses.
    run_clean("const_plain.rs", "pub const K: u8 = 0;\n");
}

// Rows 14 and 15: `union`/`raw` turbofish calls, `raw` as a type,
// and reserved words in patterns.
#[test]
fn reserved_word_paths_parse() {
    let a = run_clean(
        "union_turbofish.rs",
        "fn union<T>() -> u8 {\n    1\n}\nfn f() -> u8 {\n    union::<u8>()\n}\n",
    );
    assert!(
        a.facts.calls.iter().any(|c| c.callee == "union::<u8>"),
        "union::<u8>() must surface as a call"
    );
    // The bamTiScript shape: a reserved-word call with arguments.
    run_clean(
        "raw_turbofish_args.rs",
        "fn f() {\n    foo(raw::<H>(), union::<H>);\n}\n",
    );
    let b = run_clean("raw_type.rs", "struct raw;\nfn f(x: raw) {}\n");
    let f = b
        .facts
        .functions
        .iter()
        .find(|f| f.name == "f")
        .expect("function f must be reported");
    assert_eq!(
        f.params.iter().map(|p| (p.name, p.ty)).collect::<Vec<_>>(),
        [("x", Some("raw"))]
    );
    run_clean(
        "captured_reserved.rs",
        "fn f() {\n    match x {\n        raw @ _ => {}\n    }\n}\n",
    );
    run_clean(
        "generic_reserved_pattern.rs",
        "fn f() {\n    let raw::<u8> = 1;\n}\n",
    );
    // Existing behavior: an ordinary turbofish call still surfaces.
    let c = run_clean("plain_turbofish.rs", "fn f() {\n    foo::<u8>();\n}\n");
    assert!(
        c.facts.calls.iter().any(|c| c.callee == "foo::<u8>"),
        "foo::<u8>() must surface as a call: {:?}",
        c.facts.calls
    );
}

// Row 16: comparison after an `as` cast on a scoped type.
#[test]
fn cast_then_compare_parses() {
    let a = run_clean("cast_le.rs", "fn f() {\n    x as libc::c_int <= 3;\n}\n");
    assert!(
        function_names(&a).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&a)
    );
    // Existing behavior: the same comparison on a primitive type parses.
    let b = run_clean(
        "cast_le_prim.rs",
        "fn f() {\n    let _ = x as i32 <= 3;\n}\n",
    );
    assert!(
        function_names(&b).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&b)
    );
}

// Row 17: `where` on a function type and a leading lifetime under `dyn`.
#[test]
fn where_fn_and_dyn_lifetime_parse() {
    run_clean("where_fn_ty.rs", "fn f() where fn(u8): Send {}\n");
    run_clean("dyn_lifetime.rs", "fn f(x: Box<dyn 'a + Send>) {}\n");
}

// Row 18: prefix inclusive ranges (previously misparsed as assignment,
// and rejected outright with a non-literal endpoint).
#[test]
fn prefix_inclusive_range_parses() {
    run_clean("prefix_range_lit.rs", "fn f() {\n    let _ = ..=5;\n}\n");
    run_clean(
        "prefix_range_ident.rs",
        "fn f() {\n    let _ = self.end.clone().shrink().map(|end| ..=end);\n}\n",
    );
    // Existing behavior: other range shapes still parse.
    run_clean(
        "other_ranges.rs",
        "fn f() {\n    let _ = ..5;\n    let _ = ..;\n    let _ = 1..=5;\n    for i in ..=3 {}\n}\n",
    );
}

// Row 19: empty bounds in a where predicate.
#[test]
fn empty_where_bounds_parse() {
    let a = run_clean("empty_bounds.rs", "fn f<C>() where C: {}\n");
    assert!(
        function_names(&a).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&a)
    );
    run_clean(
        "empty_bounds_struct.rs",
        "struct S<C> where C: {\n    x: C,\n}\n",
    );
    // Empty bounds before a comma, mid-list and alone.
    let b = run_clean(
        "empty_bounds_comma.rs",
        "fn f<C, D>() where C: , D: Send {}\n",
    );
    assert!(
        function_names(&b).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&b)
    );
    run_clean("empty_bounds_self.rs", "trait T where Self:,\n{\n}\n");
}

// Row 20: `safe` items in an `unsafe extern` block.
#[test]
fn unsafe_extern_safe_items_parse() {
    run_clean(
        "safe_extern.rs",
        "unsafe extern \"C\" {\n    safe fn foo();\n}\n",
    );
    run_clean(
        "safe_statics.rs",
        "unsafe extern \"C\" {\n    safe static X: u8;\n    unsafe static Y: u8;\n}\n",
    );
    // Existing behavior: a plain unsafe foreign function still parses.
    run_clean(
        "unsafe_extern_plain.rs",
        "unsafe extern \"C\" {\n    pub unsafe fn bar();\n}\n",
    );
    // `safe` keeps working as an ordinary identifier.
    let a = run_clean(
        "safe_ident.rs",
        "fn f(safe: u8) {\n    let _ = safe + 1;\n}\n",
    );
    assert!(
        function_names(&a).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&a)
    );
}

// Row 21: turbofish in struct patterns.
#[test]
fn turbofish_struct_pattern_parses() {
    let a = run_clean(
        "turbofish_pattern.rs",
        "fn f() {\n    let R::<u8> { s, e } = r;\n}\n",
    );
    assert!(
        function_names(&a).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&a)
    );
    // Existing behavior: a plain struct pattern still parses.
    let b = run_clean("plain_pattern.rs", "fn f() {\n    let R { s } = r;\n}\n");
    assert!(
        function_names(&b).contains(&"f"),
        "function f recorded: {:?}",
        function_names(&b)
    );
}

// Constructs rustc rejects must stay parse errors here too.
#[test]
fn invalid_code_still_reports_parse_error() {
    // Attributes are not allowed in tuple patterns.
    let a = run(
        "tuple_pattern_attr.rs",
        "fn f(t: (u8, u8)) {\n    let (x, #[cfg(y)] y) = t;\n}\n",
    );
    assert!(
        a.parse_error.is_some(),
        "attribute in tuple pattern must stay an error"
    );
    // A bare `<` after `as` on a scoped type is a generic-args error.
    let b = run("cast_lt.rs", "fn f() {\n    x as libc::c_int < 3;\n}\n");
    assert!(
        b.parse_error.is_some(),
        "bare `<` after `as` on a scoped type must stay an error"
    );
    // `<<` after `as` is a generic-args error, not a shift.
    let c = run("cast_shl.rs", "fn f() {\n    let _ = x as T << 3;\n}\n");
    assert!(
        c.parse_error.is_some(),
        "`<<` after `as` must stay an error"
    );
    // A missing expression is invalid in every grammar.
    let d = run("empty_expr.rs", "fn broken() {\n    let x = ;\n}\n");
    assert!(
        d.parse_error.is_some(),
        "missing expression must stay an error"
    );
}
