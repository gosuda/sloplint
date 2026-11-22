//! Scala 2 soft-keyword shapes: `enum`, `given` and `using` read as plain
//! names wherever the corpus needs them, while a real definition keeps the
//! keyword reading. Split out of `scala.rs` to keep each file under the
//! contributor line limit. Same fixture rules: every repro is valid input
//! for `scalac 3.10.0 -Ystop-after:parser`.

use std::sync::LazyLock;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::Scala,
        path: "gap.scala",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

/// first_error_pos cannot see hidden MISSING recovery nodes, so a clean
/// `parse_error` alone could false-green; the raw tree must be healthy too.
fn assert_raw_clean(source: &str) {
    let language: tree_sitter::Language = tree_sitter_scala::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("scala language loads");
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
        "valid Scala must parse cleanly, first error at {:?} in:\n{source}",
        analysis.parse_error
    );
    analysis
}

fn sexp_of(source: &str) -> String {
    let language: tree_sitter::Language = tree_sitter_scala::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("scala language loads");
    parser
        .parse(source, None)
        .expect("parser returns a tree")
        .root_node()
        .to_sexp()
}

fn callees<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.calls.iter().map(|c| c.callee).collect()
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}
#[test]
fn given_call_after_line_broken_operator_is_an_operand() {
    // `x +` then a newline then `given(1)`: the `given` is a call operand,
    // not the start of a given definition — `(1)` cannot begin a definition.
    let source = concat!("def given(x: Int) = x\n", "val c = 1 +\n", "  given(2)\n",);
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["given"]);
    // `function_names` cannot tell a call block from a given body — the
    // `given(2)` call itself must be in the callee table.
    assert_eq!(callees(&analysis), ["given"]);
}

#[test]
fn given_call_result_keeps_infix_and_match_continuations() {
    // On a continuation line, `given(2) == 3` compares the call result —
    // `==` is not a declaration suffix — and `given[Int] match {…}` matches
    // on it. Both must stay one initializer, so the `given` call survives.
    let source = concat!(
        "def given[T](x: T) = x\n",
        "val ok = 1 +\n",
        "  given(2) == 3\n",
        "val result = 1 +\n",
        "  given[Int] match { case _ => 0 }\n",
    );
    let analysis = run_clean(source);
    assert_eq!(callees(&analysis), ["given"]);
    let sexp = sexp_of(source);
    assert!(
        sexp.contains("value: (match_expression"),
        "the match must stay inside the initializer, sexp: {sexp}"
    );
}

#[test]
fn given_operand_accepts_unicode_infix_operators() {
    // `≤` is \p{Sm} — an operator, not an identifier start. The `given`
    // operand must not swallow it as the given's name.
    let source = concat!("def given(x: Int) = x\n", "val n = 1 +\n", "  given ≤ 2\n",);
    run_clean(source);
    let sexp = sexp_of(source);
    assert!(
        sexp.contains("right: (infix_expression"),
        "the `≤` comparison must stay the infix's right operand, sexp: {sexp}"
    );
}

#[test]
fn given_call_group_ignores_delimiters_in_literals() {
    // `) =` inside a string argument must not read as a declaration
    // suffix; the group is balanced with literals and comments skipped.
    let source = concat!(
        "def given(x: String) = x\n",
        "val n = 1 +\n",
        "  given(\"x) = y\")\n",
        "val m = 1 +\n",
        "  given(\"\"\"a) = b\"\"\")\n",
    );
    let analysis = run_clean(source);
    assert_eq!(callees(&analysis), ["given", "given"]);
}

#[test]
fn annotation_arguments_named_using_stay_annotation_arguments() {
    // `@Mark(using = 1)` is the annotation's argument list; the class's own
    // parameters start at the next list. An untyped `using`/`given` name can
    // never be a parameter (Scala requires a type there).
    let source = "class E @deprecated(using = 1)(x: Int)\n";
    let analysis = run_clean(source);
    assert!(
        analysis.parse_error.is_none(),
        "annotation named arguments must parse: {:?}",
        analysis.parse_error
    );
}

#[test]
fn given_is_definition_before_a_following_statement() {
    // `given T is U:` with an indented body must keep the given-definition
    // reading when another top-level statement follows.
    let source = concat!("given ListSet is Set:\n", "  def f = 1\n", "x = 1\n",);
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn given_after_postfix_covers_types_and_unicode_names() {
    // After a line-broken operator each `given` shape is a new declaration:
    // `[A] =>` type params, `(Int, Int)` tuple/function type, and a
    // non-ASCII name; `given(using = 2)` stays a call's named argument.
    let source = concat!(
        "val r = x +\n",
        "  given [A] => a\n",
        "val s = y +\n",
        "  given (Int, Int) = (1, 2)\n",
        "val t = z +\n",
        "  given É = e\n",
        "val u = w +\n",
        "  given(using = 2)\n",
        "val v = q +\n",
        "  given (Int, Int) with { def f = 1 }\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn enum_stays_a_soft_identifier() {
    // Scala 2 code uses `enum` as a parameter or named argument; only a real
    // `enum` definition should claim the keyword reading.
    let source = concat!(
        "def f(enum: Int): Int = enum\n",
        "trait T { def g(enum: E): Unit }\n",
        "enum Foo { case A, B }\n",
        "val m = Map(enum = 1)\n",
        // `enum` after a line-broken operator is the operand, not a definition.
        "val n = 1 +\n",
        "  enum\n",
        // But `enum` still starts a definition or a standalone assignment
        // after a postfix operator, even across a blank line.
        "var enum = 0\n",
        "val r = x --?\n",
        "\n",
        "enum = 1\n",
        "val s = y --?\n",
        "enum Bar { case C }\n",
        // `enum(2)` after a line-broken operator calls the function `enum`:
        // an enum declaration always needs a name, so `(` stays an operand.
        "def e(enum: Int) = enum\n",
        "def enum(x: Int) = x\n",
        "val c = (1 +\n",
        "  enum(2))\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f", "e", "enum"]);
}
