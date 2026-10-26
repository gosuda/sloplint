//! Valid Kotlin that the pinned tree-sitter-kotlin-ng 1.1.0 grammar rejected.
//!
//! Clean assertions check the raw tree (`has_error` on the root), not only
//! `analyze`: a MISSING node inserted during error recovery is hidden from the
//! cursor, so `analyze` can report no error for a tree that has one. Invalid
//! controls are inputs kotlinc 2.4.21 rejects with a syntax error; they must
//! keep producing an error on the raw tree.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Kotlin,
        path: "gap.kt",
        source,
        is_test_file: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn raw_has_error(source: &str) -> bool {
    let language: tree_sitter::Language = tree_sitter_kotlin_ng::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("kotlin language loads");
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let root = tree.root_node();
    root.has_error() || root.to_sexp().contains("MISSING")
}

fn run_clean(source: &str) -> Analysis<'_> {
    assert!(
        !raw_has_error(source),
        "valid Kotlin must parse cleanly on the raw tree:\n{source}"
    );
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_none(),
        "valid Kotlin must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn run_invalid(source: &str) {
    assert!(
        raw_has_error(source),
        "invalid Kotlin must leave an error on the raw tree:\n{source}"
    );
}

#[test]
fn context_parameters_on_a_declaration_parse() {
    run_clean("context(x: String)\nfun f(): Int = 1\n");
}

#[test]
fn context_parameters_in_a_function_type_parse() {
    run_clean("fun g(block: context(String, Int) () -> Unit) {}\n");
}

#[test]
fn context_receiver_in_a_parenthesized_function_type_parses() {
    run_clean("class S\nfun <R> g(b: (context(S) () -> R)): R = null as R\n");
}

#[test]
fn suspend_lambda_bound_to_a_value_parses() {
    run_clean("fun f() {\n    val l = suspend { 1 }\n}\n");
}

#[test]
fn nullable_callable_reference_parses() {
    run_clean(
        "fun <A, B, R> m(a: A, b: B, t: (A, B) -> R): R = null as R\n\
         fun g(a: String?, b: Int): String = m(a, b, String?::plus)\n",
    );
}

#[test]
fn local_function_then_callable_reference_statement_parses() {
    run_clean("fun h() {}\nfun f() {\n    fun g() {}\n    ::h.toString()\n}\n");
}

#[test]
fn five_quote_closer_after_content_parses() {
    run_clean("fun f(): String {\n    val s = \"\"\"ab\"\"\"\"\"\n    return s\n}\n");
}

#[test]
fn escaped_quote_before_a_five_quote_closer_parses() {
    run_clean("fun f(): String {\n    val s = \"\"\"a\"\\\"\"\"\"\"\n    return s\n}\n");
}

#[test]
fn double_dollar_string_parses() {
    run_clean("fun f(): String = $$\"hello\"\n");
}

#[test]
fn double_dollar_multiline_string_parses() {
    run_clean("fun f(): String = $$\"\"\"\ncontent\n\"\"\"\n");
}

#[test]
fn enum_entry_named_enum_parses() {
    run_clean("enum class E {\n    exclusiveMinimum,\n    enum,\n    id\n}\n");
}

#[test]
fn when_entry_with_an_is_type_guard_parses() {
    run_clean(
        "fun f(e: Exception): Int = when (e) {\n    is RuntimeException if e.message != null -> 1\n    else -> 2\n}\n",
    );
}

#[test]
fn get_call_with_a_trailing_lambda_after_a_value_parses() {
    run_clean(
        "fun get(s: String, h: () -> Unit) {}\nfun f() {\n    val m = 1\n    get(\"/x\") {\n    }\n}\n",
    );
}

#[test]
fn set_call_with_nested_call_arguments_after_a_value_parses() {
    run_clean(
        "fun set(a: String, b: Int) {}\nfun g(s: String): Int = 1\nfun f() {\n    val s = \"d\"\n    set(s, g(s))\n}\n",
    );
}

#[test]
fn enum_entries_with_bodies_closed_on_the_same_line_parse() {
    run_clean(
        "enum class E {\n    A { override fun toString(): String = \"a\" },\n    B { override fun toString(): String = \"b\" };\n\n    abstract fun h(): Int\n}\n",
    );
}

#[test]
fn class_member_closed_by_a_brace_on_the_same_line_parses() {
    run_clean("class A { fun f(): Int = 1 }\n");
}

#[test]
fn valid_single_entry_enum_body_still_parses() {
    run_clean("enum class E {\n    A { override fun toString(): String = \"a\" }\n}\n");
}

#[test]
fn valid_one_line_enum_entry_body_still_parses() {
    run_clean("enum class E { A { override fun toString(): String = \"a\" } }\n");
}

#[test]
fn valid_local_class_with_a_class_literal_still_parses() {
    run_clean("fun f() {\n    class C { fun g(): Int = 1 }\n    val m = C::class.java\n}\n");
}

#[test]
fn valid_one_line_local_class_separated_by_a_semicolon_parses() {
    run_clean("fun f() { class C { fun g(): Int = 1 }; val m = C::class.java }\n");
}

#[test]
fn invalid_getter_without_a_body_still_reports_an_error() {
    run_invalid("var x: Int = 1\n    get()\nfun f(): Int = x\n");
}

#[test]
fn invalid_local_class_without_a_separator_still_reports_an_error() {
    run_invalid("fun f() { class C { fun g(): Int = 1 } val m = C::class.java }\n");
}

#[test]
fn invalid_enum_entry_body_without_a_separator_still_reports_an_error() {
    run_invalid("enum class E { A { fun f() = 1 } B }\n");
}
