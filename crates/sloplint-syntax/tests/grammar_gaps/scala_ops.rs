//! Valid Scala operator and comment-adjacent shapes that the published
//! tree-sitter-scala 0.26.3 grammar rejected or misread, split out of
//! `scala.rs` to keep each file under the contributor line limit. Same
//! fixture rules: every repro is valid input for `scalac 3.10.0
//! -Ystop-after:parser`.

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

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
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

#[test]
fn reserved_operator_symbol_literals_parse() {
    // `'=`, `':`, `'<-`, `'->` name operators; `'='` stays a char literal.
    let source = concat!(
        "object O {\n",
        "  val a = '=\n",
        "  val b = ':\n",
        "  val c = '<-\n",
        "  val d = '->\n",
        "  val e = '/\n",
        "  val ch = '='\n",
        "}\n",
    );
    run_clean(source);
}

#[test]
fn leading_stray_semicolons_parse() {
    // `;;;def x = 1` — scalac skips a stray `;` run at the top of a file too.
    let source = concat!(";;;val h = 1\n", ";;;\n");
    run_clean(source);
}

#[test]
fn slash_run_stops_at_comment_before_operand() {
    // `x /+/* c */ y` and `x /+ // c\n y`: a `/`-led operator stops where a
    // comment opens — the internal run would swallow the comment's slashes.
    // `x /+ // c\n val` is a postfix operator before a new definition.
    let source = concat!(
        "object T {\n",
        "  val a = x /+/* note */ y\n",
        "  val b = x /+// note\n",
        "    y\n",
        "  val c = x /+// note\n",
        "  val d = y\n",
        "}\n",
    );
    run_clean(source);
}

#[test]
fn comment_openers_are_not_operators() {
    // `//` and `/*` at statement position must not lex as phantom `/`-class
    // operators; `x +/*c*/ y` keeps `+` internal with a comment node between
    // operator and operand, and `???/*c*/()` still calls.
    let source = concat!(
        "// a line comment\n",
        "/* a block comment */\n",
        "object T {\n",
        "  val a = x +/*note*/ y\n",
        "  def m = ???/*c*/()\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["m"]);
}

#[test]
fn slash_symbol_literals_parse() {
    // `'/:/`, `'+/`, `'/` name slash-class operators as symbol literals.
    let source = concat!(
        "object O {\n",
        "  val a = '/:/\n",
        "  val b = '+/\n",
        "  val c = '/\n",
        "}\n",
    );
    run_clean(source);
}

#[test]
fn comment_stopped_operator_keeps_its_class() {
    // A `/*` after an operator's `/`-run cuts the token there, so the
    // internal run cannot spell it: `x /+/* note */ y` is `x /+ y`, and a
    // `:`- or `=`-tailed spelling keeps its precedence class out of the
    // external tokens.
    let source = concat!(
        "val a = x /+/*note*/ y\n",
        "val b = x */+:/*note*/ y\n",
        "val c = x */+=/*note*/ y\n",
        // Right-associative `:`-tail: the second `/+:` binds `y /+: z`.
        "val d = x /+:/*note*/ y /+: z\n",
    );
    run_clean(source);
    // `*/+:` chains right: `x /+: (y /+: z)`, not `(x /+: y) /+: z`.
    let language: tree_sitter::Language = tree_sitter_scala::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let tree = parser.parse("val d = x /+: y /+: z", None).unwrap();
    let sexp = tree.root_node().to_sexp();
    assert!(
        sexp.contains("right: (infix_expression"),
        "a `:`-tail operator must associate right, sexp: {sexp}"
    );
}

#[test]
fn quote_symbol_ending_in_slash() {
    // `'op` may end in `/` when the slash does not open a comment; `'+/*c*/`
    // is the `'+'` quote followed by a block comment.
    let source = concat!("val a = '+/\n", "val b = '/:/\n", "val c = '+/*c*/\n",);
    run_clean(source);
}

#[test]
fn operator_names_keep_their_precedence_classes() {
    // SLS 6.12.3: the first character fixes the precedence class and the
    // ending `:` picks right-associativity, so a wrong class changes the
    // nesting even though every line still parses. `/` binds in the
    // `*`/`%` class and left-associates; `:/` (starts `:`, ends `/`) sits
    // below `*` but still left-associates; `★`/`☃` are So-category
    // operator chars scalac accepts.
    let language: tree_sitter::Language = tree_sitter_scala::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("scala language loads");
    let source = concat!(
        "val l = a / b / c\n",
        "val m = a + b / c\n",
        "val r = x :/ y :/ z\n",
        "val s = a ★ b ☃ c\n",
        "val t = f(1, 2) : // trailing note\n  y => g(y)\n",
        "val u = '×/☃\n",
        // Comment touching the colon, closing on its own line.
        "val v = f(1, 2) :/*c*/ Int => b\n",
        // Multi-line comment after the colon: the `*/` line carries code.
        "val w = (x: /* two\n lines */ Int => x)\n",
        // A lone `/` after the comment is code, not swallowed whitespace.
        "val z = Map[String, /*c*/ / => Int]()\n",
        // A newline inside the colon's comment ends the colon's line:
        // `y => g(y)` is the fewer-braces body, not a lambda argument
        // that would swallow the `f(1, 2)` call.
        "def cb() = { f(1, 2) : /* two\nlines */ y => g(y) }\n",
    );
    let tree = parser.parse(source, None).expect("parser returns a tree");
    let sexp = tree.root_node().to_sexp();
    assert!(!tree.root_node().has_error(), "sexp: {sexp}");
    // `a / b / c`, `x :/ y :/ z` and `a ★ b ☃ c` nest on the left — none
    // of the three ends in `:`, so all left-associate.
    assert!(
        sexp.matches("left: (infix_expression").count() == 3,
        "the repeated operators must nest on the left, sexp: {sexp}"
    );
    // `a + b / c` keeps `/` tighter than `+`: the nested infix sits on
    // the right of the `+`.
    assert!(
        sexp.matches("right: (infix_expression").count() == 1,
        "`a + b / c` must bind `/` tighter than `+`, sexp: {sexp}"
    );
    assert!(
        sexp.contains("colon_argument"),
        "the comment after an EOL colon keeps the indented argument, sexp: {sexp}"
    );
    assert_eq!(
        sexp.matches("(ascription_expression").count(),
        3,
        "the adjacent, multi-line and in-brace comments keep their type ascriptions, sexp: {sexp}"
    );
    assert!(
        sexp.contains("function_type"),
        "the lone `/` after a comment still ascripts to an operator type, sexp: {sexp}"
    );
    assert!(
        sexp.contains("quote_expression"),
        "`'×/☃` stays one symbol name, sexp: {sexp}"
    );
    // `{ f(1,2) : /* a\nb */ y => g(y) }` — inside braces there is no
    // indentation syntax, so the colon stays a plain ascription (same read
    // upstream gives); what matters is the `f` call surviving instead of
    // being swallowed as a lambda parameter.
    let cb_idx = sexp.find("(function_definition").unwrap_or(sexp.len());
    let cb = &sexp[cb_idx..];
    assert!(
        cb.matches("(call_expression").count() >= 1,
        "the newline inside the colon's comment must not merge the call into a lambda, sexp: {sexp}"
    );
}

#[test]
fn export_stays_an_operand_after_slash() {
    // `export` is a plain identifier in Scala 2: `10 / export` is division,
    // not a postfix `/` before a declaration. The declaration read still
    // wins when a member selector or assignment follows.
    let source = concat!(
        "object T {\n",
        "  val export = 2\n",
        "  val a = 10 / export\n",
        "  val b = x /\n",
        "    export\n",
        "  val c = y /\n",
        "    export = 1\n",
        "  val d = z /\n",
        "    export a.b\n",
        "}\n",
    );
    run_clean(source);
}

#[test]
fn slash_tail_operator_before_definition_is_postfix() {
    // `x /` followed by a definition keyword is the postfix operator, not an
    // infix waiting for `val` as its right operand.
    let source = concat!(
        "object T {\n",
        "  val a = x /\n",
        "  val v = 1\n",
        "  val b = y /\n",
        "  private val p = 2\n",
        "}\n",
    );
    run_clean(source);
}

#[test]
fn multi_slash_operator_keeps_assign_class() {
    // `+?/!=`: the slash-run continues the operator past the first `/`, so
    // the length passed to the assign check must count the whole run — the
    // two-character `!=` comparison carve-out must not fire.
    let source = concat!(
        "object T {\n",
        "  val a = x +?/!=/* note */ y + z\n",
        "  val b = x +?/!= y\n",
        "}\n",
    );
    run_clean(source);
    // `x +?/!= y + z` in the assignment class groups `y + z` as the right
    // operand; the misread `x +?/!=` comparison form would surface `+ z`
    // as a separate infix level instead.
    let sexp = sexp_of(source);
    assert!(
        sexp.contains("right: (infix_expression"),
        "the comment-cut `+?/!=` must keep assignment precedence so `y + z` stays its right operand, sexp: {sexp}"
    );
}

#[test]
fn line_comment_does_not_end_an_infix_operand() {
    // `x +/ // note` on one line and `y` on the next: the line comment is
    // transparent to scalac, so `y` is still the right operand and the
    // expression stays infix. Reading the comment as the line's end turned
    // the operator postfix and left `y` a stray statement.
    let source = concat!(
        "object T {\n",
        "  val a = x +/ // note\n",
        "    y\n",
        "  val b = x /+// note\n",
        "    y\n",
        "}\n",
    );
    run_clean(source);
    let sexp = sexp_of(source);
    assert!(
        !sexp.contains("postfix_expression"),
        "both operators keep an infix right operand across the line comment, sexp: {sexp}"
    );
    assert_eq!(
        sexp.matches("(infix_expression").count(),
        2,
        "each val is one infix expression, sexp: {sexp}"
    );
}

#[test]
fn blank_line_ends_the_expression_before_a_soft_keyword_operand() {
    // `export` is a plain Scala 2 identifier, so `x --?` followed by one
    // newline and `export()` is the postfix operator's right operand. A
    // blank line is a statement separator: the initializer ends at `--?` and
    // `export()` is its own statement. The scanner must not let the operand
    // probe cross the blank line.
    let operand = concat!("object T {\n", "  val a = x --?\n", "  export()\n", "}\n");
    run_clean(operand);
    let operand_sexp = sexp_of(operand);
    assert!(
        operand_sexp.contains("right: (call_expression"),
        "a single newline keeps `export()` as the operand, sexp: {operand_sexp}"
    );

    let separated = concat!("object T {\n", "  val b = x --?\n", "\n", "  export()\n", "}\n");
    run_clean(separated);
    let separated_sexp = sexp_of(separated);
    assert!(
        separated_sexp.contains("(postfix_expression"),
        "a blank line makes `x --?` postfix, sexp: {separated_sexp}"
    );
    assert!(
        separated_sexp.contains("(call_expression"),
        "`export()` stays its own statement, sexp: {separated_sexp}"
    );
    assert!(
        !separated_sexp.contains("(infix_expression"),
        "the call does not become the operator's operand, sexp: {separated_sexp}"
    );
}
