//! Valid Kotlin that the pinned tree-sitter-kotlin-ng 1.1.0 grammar rejected.
//!
//! Every clean case here is valid input for kotlinc 2.4.21. The invalid cases
//! are inputs kotlinc rejects; they must keep reporting a parse error.

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
        has_interface: false,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn run_clean(source: &str) -> Analysis<'_> {
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_none(),
        "valid Kotlin must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn run_invalid(source: &str) {
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_some(),
        "invalid Kotlin must report a parse error:\n{source}"
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
fn invalid_empty_array_literal_still_reports_an_error() {
    run_invalid("fun f(): Int {\n    val x = []\n    return 1\n}\n");
}

#[test]
fn invalid_getter_without_a_body_still_reports_an_error() {
    run_invalid("var x: Int = 1\n    get()\nfun f(): Int = x\n");
}

#[test]
fn valid_single_entry_enum_body_still_parses() {
    run_clean("enum class E {\n    A { override fun toString(): String = \"a\" }\n}\n");
}

#[test]
fn valid_local_class_with_a_class_literal_still_parses() {
    run_clean("fun f() {\n    class C { fun g(): Int = 1 }\n    val m = C::class.java\n}\n");
}
