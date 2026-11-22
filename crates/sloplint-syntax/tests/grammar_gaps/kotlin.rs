//! Valid Kotlin that the pinned tree-sitter-kotlin-ng 1.1.0 grammar rejected.
//!
//! Clean assertions check the raw tree (`has_error` on the root), not only
//! `analyze`: a MISSING node inserted during error recovery is hidden from the
//! cursor, so `analyze` can report no error for a tree that has one. Invalid
//! controls are inputs kotlinc 2.4.21 rejects with a syntax error; they must
//! keep producing an error on the raw tree.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Kotlin,
        path: "gap.kt",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn raw_tree(source: &str) -> tree_sitter::Tree {
    let language: tree_sitter::Language = tree_sitter_kotlin_ng::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("kotlin language loads");
    parser.parse(source, None).expect("parser returns a tree")
}

fn raw_has_error(source: &str) -> bool {
    let tree = raw_tree(source);
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

#[test]
fn explicit_backing_field_with_a_type_parses() {
    run_clean(
        "class C {\n    val ready: kotlinx.coroutines.flow.MutableStateFlow<Boolean>\n        field: MutableStateFlow<Boolean> = MutableStateFlow(false)\n}\n",
    );
}

#[test]
fn explicit_backing_field_without_a_type_parses() {
    run_clean("class C {\n    var flag: Boolean\n        field = false\n}\n");
}

#[test]
fn field_as_a_plain_expression_still_splits_statements() {
    run_clean("fun f(field: Int): Int {\n    val x = 1\n    field + x\n}\n");
}

#[test]
fn modified_and_comment_backing_fields_parse() {
    run_clean(
        "class C {\n    val a: Boolean\n        private field = false\n    val b: Boolean\n        field /* c */ = false\n}\n",
    );
}

#[test]
fn field_assignment_in_a_body_stays_a_statement() {
    run_clean("fun f(field: Boolean) {\n    val x = false\n    field = true\n}\n");
}

#[test]
fn semicolon_then_field_assignment_stays_a_local_statement() {
    // Local variables cannot carry a backing field (Kotlin gives those to
    // member and top-level properties only), so `var x = 1; field = 5` in a
    // body is an initializer then an assignment, never `backing_field`.
    let sexp = raw_tree("fun f() {\n    var x = 1; field = 5\n}\n")
        .root_node()
        .to_sexp();
    assert!(
        sexp.contains("(assignment"),
        "field = 5 must be an assignment, got:\n{sexp}"
    );
    assert!(
        !sexp.contains("(backing_field"),
        "locals cannot have a backing field, got:\n{sexp}"
    );
}

#[test]
fn semicolon_then_getter_call_stays_a_local_statement() {
    let sexp = raw_tree("fun f() {\n    var x = 1; get() {}\n}\n")
        .root_node()
        .to_sexp();
    assert!(
        sexp.contains("(call_expression"),
        "get() {{}} must be a call with a trailing lambda, got:\n{sexp}"
    );
    assert!(
        !sexp.contains("(getter"),
        "locals cannot have accessors, got:\n{sexp}"
    );
}

#[test]
fn explicit_backing_field_then_accessors_parse() {
    run_clean(
        "class C {\n    val y = MutableStateFlow(0)\n        field = it\n        get() = field.value\n}\n",
    );
    let sexp = raw_tree(
        "class C {\n    val y = MutableStateFlow(0)\n        field = it\n        get() = field.value\n}\n",
    )
    .root_node()
    .to_sexp();
    assert!(
        sexp.contains("(backing_field") && sexp.contains("(getter"),
        "accessors must be allowed after a backing field, got:\n{sexp}"
    );
}

#[test]
fn open_and_override_as_identifiers_parse() {
    run_clean(
        "fun a() { val open = 1; open(2) }\nclass A : SeekableByteChannel {\n    private var open = true\n    override fun close() { open = false }\n}\n",
    );
}

#[test]
fn annotation_on_a_parenthesized_function_type_parses() {
    run_clean("fun f(content: @Composable (() -> Unit)) {}\n");
}

#[test]
fn empty_array_literal_in_an_annotation_argument_parses() {
    run_clean(
        "@Entity(tableName = \"config\", indices = [])\ndata class ConfigEntity(val id: Int)\n",
    );
}

#[test]
fn primary_constructor_parameters_on_the_next_line_parse() {
    run_clean("internal class D\n/** doc */\n    (x: Int) {\n}\n");
    let sexp = raw_tree("internal class D\n/** doc */\n    (x: Int) {\n}\n")
        .root_node()
        .to_sexp();
    assert!(
        sexp.contains("(primary_constructor"),
        "the next-line parens must stay the primary_constructor, got:\n{sexp}"
    );
}

#[test]
fn parenthesized_line_after_a_statement_keeps_its_separator() {
    for source in [
        "fun f() {\n    fun g() {}\n    (a as B).c()\n}\n",
        "fun f() {\n    do {\n        p()\n    } while (c)\n    (a as B).c()\n}\n",
        "fun f() {\n    val x: Int\n    (a as B).c()\n}\n",
        "fun f() {\n    class D {}\n    (a as B).c()\n}\n",
    ] {
        run_clean(source);
    }
}

#[test]
fn suspend_receiver_function_type_binds_suspend_to_the_type() {
    let sexp = raw_tree("val f: suspend (Int).() -> Unit = TODO()\n")
        .root_node()
        .to_sexp();
    assert!(
        sexp.contains("(function_type (type_modifiers"),
        "suspend must modify the function_type, got:\n{sexp}"
    );
}

#[test]
fn annotated_open_class_stays_a_declaration() {
    run_clean("@Deprecated(\"old\") open class C {}\n");
    let sexp = raw_tree("@Deprecated(\"old\") open class C {}\n")
        .root_node()
        .to_sexp();
    assert!(
        sexp.contains("(class_declaration"),
        "annotated class must land in class_declaration, got:\n{sexp}"
    );
}

#[test]
fn annotated_local_class_stays_a_declaration() {
    run_clean("fun f() { @Deprecated(\"old\") open class C {} }\n");
    let sexp = raw_tree("fun f() { @Deprecated(\"old\") open class C {} }\n")
        .root_node()
        .to_sexp();
    assert!(
        sexp.contains("(block (class_declaration"),
        "annotated local class must land in class_declaration, got:\n{sexp}"
    );
}

#[test]
fn top_level_statement_query_matches_statements() {
    // `(source_file (statement))` must compile — sloplint rules query the
    // `statement` supertype at file level and a compile failure aborts the
    // whole lint.
    let language: tree_sitter::Language = tree_sitter_kotlin_ng::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let source = "var x = 1\ny = 2\n";
    let tree = parser.parse(source, None).unwrap();
    let query = tree_sitter::Query::new(&language, "(source_file (statement) @s)")
        .expect("source_file must expose the statement supertype");
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matched = 0;
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    tree_sitter::StreamingIterator::for_each(&mut matches, |_| matched += 1);
    assert_eq!(
        matched, 1,
        "the top-level assignment must match (statement)"
    );
}

#[test]
fn value_receiver_class_literal_keeps_parenthesized_expression() {
    run_clean("fun f(v: Any) { (v)::class.java }\n");
    let sexp = raw_tree("fun f(v: Any) { (v)::class.java }\n")
        .root_node()
        .to_sexp();
    assert!(
        sexp.contains("(callable_reference (parenthesized_expression"),
        "the value receiver must stay a parenthesized_expression, got:\n{sexp}"
    );
}

#[test]
fn superclass_call_arguments_survive_a_newline() {
    // `class D : Base\n() {}` — when a declaration separator is followed
    // by `(`, the scanner emits `_delegation_semi` instead of `_semi`,
    // which only the delegation's constructor invocation accepts.
    for source in [
        "class D : Base\n() {}\n",
        "class E : Iface1, Base\n(arg) {}\n",
        "class F : Base\n(x = 1) {}\n",
    ] {
        run_clean(source);
    }
    let sexp = raw_tree("class D : Base\n(x) {}\n").root_node().to_sexp();
    assert!(
        sexp.contains("(constructor_invocation"),
        "split superclass call must stay a constructor_invocation, got:\n{sexp}"
    );
}

#[test]
fn bodyless_class_with_generic_delegation_parses() {
    // `class T : M<Int>` — the separator after the delegation type must end
    // the declaration, not shift into the constructor invocation's tolerated
    // separator (which then demands `(`). `_delegation_semi` is emitted only
    // when `(` follows, so an EOF- or statement-followed `_semi` reduces the
    // specifier instead.
    for source in [
        "class T : M<Int>\n",
        "class Tagged : Marker<Int,>\n",
        "class T : M<Int>\nval y = 1\n",
        "class T private constructor() : Base\n(1) {}\n",
        "class T : A, B\n(1) {}\n",
    ] {
        run_clean(source);
    }
}

#[test]
fn wrapped_suspend_function_type_parses() {
    // `suspend ((A) -> B)` — ktor's JWT verifier returns this shape.
    for source in [
        "val v: suspend ((A) -> B)\n",
        "typealias T = suspend ((A) -> B)\n",
        "fun f(): suspend ((A) -> B) = TODO()\n",
    ] {
        run_clean(source);
    }
}

#[test]
fn trailing_commas_in_type_lists_parse() {
    run_clean("fun <T,> takesTrailing() {}\nval r = box<Int, String,>()\n");
}

#[test]
fn unnamed_context_parameter_parses() {
    run_clean("context(_: CoroutineContext)\nfun withCtx() {}\n");
}

#[test]
fn untyped_anonymous_function_parameters_parse() {
    run_clean("val inc = fun(m) = m\nval g = fun(x, y) { return x + y }\n");
}

#[test]
fn secondary_constructor_head_split_across_lines_parses() {
    run_clean(
        "class C {\n    @Inject\n    internal constructor(a: A)\n    @Anno(name = \"x\")\n    actual constructor(b: B)\n}\n",
    );
}

#[test]
fn semicolon_before_accessors_parses() {
    run_clean(
        "var top = 1; private set\n\
         class C {\n\
         \x20   var m = 1; private set\n\
         \x20   var g = 0; get() = 2\n\
         \x20   var s = 0; set(v) { field = v }\n\
         \x20   val f = 2\n\
         }\n",
    );
}

#[test]
fn plain_member_after_semicolon_accessor_parses() {
    run_clean("class C {\n    var m = 1; private set\n    fun f() {}\n}\n");
}

#[test]
fn bare_default_accessors_after_a_semicolon_parse() {
    run_clean("class C {\n    var x = 1; set\n    var y = 2; get; private set\n}\n");
}

#[test]
fn split_infix_continuations_and_reused_accessor_words_parse() {
    // `as`/`is`/`in` cannot start a statement, so `get\nas Int` is the
    // infix cast `get as Int` — the `;` separates and `get` is an
    // identifier. `isÉ` is a single identifier (unicode letters continue
    // words), so `isÉ()` is a call, not an `is` continuation.
    run_clean(
        "var a = 1; get\nas Int\n\
         var c = 1; get\nis\u{c9}()\n\
         var d = 1; set\nin list\n",
    );
    // Clean is not enough for `isÉ()`: the detached-getter misread parses
    // without errors too. The property must keep its `getter` and the
    // next line must be a `call_expression`.
    let sexp = raw_tree("var c = 1; get\nis\u{c9}()\n")
        .root_node()
        .to_sexp();
    assert!(
        sexp.contains("(getter") && sexp.contains("(call_expression"),
        "the bare getter must stay on the property while `isÉ()` calls:\n{sexp}"
    );
    // `x\nis Int` at file level still fails — a pre-existing upstream
    // newline-continuation gap (`is` also opens `when` entry conditions,
    // so the scanner cannot unconditionally continue on it). The `;`
    // probe behaves the same: `get` reads as an identifier, matching
    // what a plain `x` does there.
    // A used accessor slot cannot be taken again: after `get() = 1` only
    // `set` remains valid, so `; get` is a statement (`get\nfoo()` is a
    // call), and `; get(2)`/`get = 2` stay statements regardless.
    run_clean(
        "var x: Int get() = 1; get\nfoo()\n\
         var y = 2; get(2)\nvar z = 3; get = 2\n",
    );
    // A lone carriage return counts as a line break, matching LF:
    // `set\r42.toString()` splits like `set\n42.toString()`, and
    // `;\rget() = x` reads the same way as `;\nget() = x`.
    run_clean("var e = 1; set\r42.toString()\nvar f = 1; get\rfoo()\n");
}
#[test]
fn infix_word_calls_after_a_semicolon_stay_statements() {
    run_clean(
        "val get = 1\nval x = 1; get until 2\nval y = 2; get as? Int\n\
         val z = 3; get foo ::bar\nval w = 4; get context 2\nval v = 5; get context2 3\n",
    );
    // A `context(...)` member declaration may follow a bare accessor.
    run_clean("var f = 6\n    get\ncontext(String) fun g() {}\n");
}

/// Loop statements as `if`/`else` branch bodies: `if (m) for (c in ys) D()`
/// used to lex `for` as an identifier call (no ERROR for the braced form,
/// so "clean" was not proof). The tree must contain real `for_statement` /
/// `while_statement` nodes, and a destructured `((p, q) in m)` header keeps
/// working inside an unbraced branch.
#[test]
fn loops_as_if_branch_bodies_parse() {
    for src in [
        "fun f() {\n  if (m) for (c in ys) D()\n}\n",
        "fun f() {\n  if (m) for (c in ys) D() else while (d) g()\n}\n",
        "fun f() {\n  if (a) for ((p, q) in m) { g() } else { h() }\n}\n",
        "fun f() {\n  if (m) while (d) g()\n}\n",
        "fun f() {\n  if (m) do { g() } while (d)\n}\n",
    ] {
        run_clean(src);
    }
    let sexp = raw_tree("fun f() {\n  if (m) for (c in ys) D() else while (d) g()\n}\n")
        .root_node()
        .to_sexp();
    assert!(
        sexp.contains("(for_statement") && sexp.contains("(while_statement"),
        "the if branch must be a real for-statement, not a `for` call:\n{sexp}"
    );
}

/// `SLOPLINT.md` names each grammar round twice: once in the header summary
/// and once as a `## Round N` section. The two must agree, and the conflicts
/// the summary counts for that round must be the ones `grammar.js` declares.
#[test]
fn sloplint_md_round_notes_match_the_grammar() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/tree-sitter-kotlin-ng");
    let notes = std::fs::read_to_string(dir.join("SLOPLINT.md"))
        .expect("vendor/tree-sitter-kotlin-ng/SLOPLINT.md must be readable");
    let grammar = std::fs::read_to_string(dir.join("grammar.js"))
        .expect("vendor/tree-sitter-kotlin-ng/grammar.js must be readable");

    let round = notes
        .lines()
        .find_map(|line| {
            line.strip_prefix("Round ")
                .and_then(|rest| rest.split(' ').next())
                .and_then(|n| n.parse::<u32>().ok())
                .filter(|_| line.contains("if`/`else` branch bodies"))
        })
        .expect("SLOPLINT.md must open with a 'Round N ... if/else branch bodies' summary");
    assert!(
        notes.contains(&format!("## Round {round} (loop statements as if/else branch bodies)")),
        "the summary calls the branch-body change Round {round}, so the dedicated section must carry the same number"
    );

    let conflicts_at = grammar
        .find("conflicts:")
        .expect("grammar.js must declare a conflicts list");
    let conflicts = &grammar[conflicts_at..];
    for declared in [
        "[$.statement, $.assignment]",
        "[$._property_declaration_head]",
        "[$.delegation_specifiers]",
    ] {
        assert!(
            conflicts.contains(declared),
            "grammar.js conflicts must declare {declared} for the merged if-then state"
        );
    }
}
