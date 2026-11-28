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

/// `sloplint-grammar.patch` must carry every file the vendor tree diverges on,
/// not only `grammar.js`: the scanner edits (SC-SOFTENUM and the operand
/// probes) live in `src/scanner.c`, and a patch that omits them cannot be
/// replayed onto the published crate. This pins the file list so a scanner
/// change without a patch hunk fails here.
#[test]
fn grammar_patch_covers_the_scanner() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/tree-sitter-scala");
    let patch = std::fs::read_to_string(dir.join("sloplint-grammar.patch"))
        .expect("sloplint-grammar.patch must be readable");
    for file in ["a/grammar.js", "a/src/scanner.c"] {
        assert!(
            patch.contains(&format!("diff --git {file} b/")),
            "sloplint-grammar.patch must carry a diff --git section for {file}"
        );
    }
    // The scanner section must be a real unified diff against the published
    // crate: it needs at least one hunk header.
    let scanner = patch
        .split("diff --git a/src/scanner.c")
        .nth(1)
        .expect("scanner.c section exists");
    assert!(
        scanner.contains("@@ -"),
        "the scanner.c section must hold unified-diff hunks"
    );
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
    // Grouping is part of the contract, not just parse success: the division
    // keeps `export` as its right operand, while an assignment or a
    // declaration after the postfix `/` stays a separate statement.
    let sexp = sexp_of(source);
    assert_eq!(
        sexp.matches("(infix_expression").count(),
        2,
        "`10 / export` and `x / export` stay one infix expression each, sexp: {sexp}"
    );
    assert_eq!(
        sexp.matches("right: (identifier)").count(),
        2,
        "division keeps `export` as its right operand in both line shapes, sexp: {sexp}"
    );
    assert_eq!(
        sexp.matches("(postfix_expression").count(),
        2,
        "the `export = 1` and `export a.b` lines keep the postfix reading, sexp: {sexp}"
    );
    assert!(
        sexp.contains("(assignment_expression left: (identifier)"),
        "`export = 1` stays a separate assignment, not part of the initializer, sexp: {sexp}"
    );
    assert!(
        sexp.contains("(export_declaration"),
        "`export a.b` stays a separate declaration, sexp: {sexp}"
    );
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

    let separated = concat!(
        "object T {\n",
        "  val b = x --?\n",
        "\n",
        "  export()\n",
        "}\n"
    );
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

#[test]
fn export_declaration_lookahead_is_comment_and_name_aware() {
    // `export` is a plain Scala 2 identifier, so only its continuation makes
    // the operator postfix. The continuation check skips comments and reads
    // every path spelling the export declaration accepts, so a commented,
    // back-quoted or non-ASCII path still opens a declaration and a
    // commented assignment stays a separate statement.
    let source = concat!(
        "object T {\n",
        "  val a = x /\n",
        "  export /* facade */ delegate.value\n",
        "  val b = x /\n",
        "  export `weird`.value\n",
        "  val c = x /\n",
        "  export \u{c9}mile.value\n",
        "  val d = x /\n",
        "  export /* note */ = 1\n",
        "}\n",
    );
    run_clean(source);
    let sexp = sexp_of(source);
    assert_eq!(
        sexp.matches("(export_declaration").count(),
        3,
        "the commented, back-quoted and non-ASCII paths stay export declarations, sexp: {sexp}"
    );
    assert!(
        sexp.contains("(assignment_expression left: (identifier)"),
        "the commented assignment stays an assignment statement, sexp: {sexp}"
    );
    assert_eq!(
        sexp.matches("(postfix_expression").count(),
        4,
        "each `/` before a declaration reads as postfix, sexp: {sexp}"
    );
}

#[test]
fn comment_only_line_does_not_end_an_infix_operand() {
    // A line holding only a comment is content to scalac, not a blank
    // line, so `&&` still takes its operand from the next line — in a
    // block and inside an `if` condition alike.
    let source = concat!(
        "object T {\n",
        "  def f(a: Boolean, b: Boolean): Boolean = {\n",
        "    val ok = a &&\n",
        "      // b must hold too\n",
        "      b\n",
        "    if (a &&\n",
        "      // c\n",
        "      b) 1 else 0\n",
        "  }\n",
        "}\n",
    );
    run_clean(source);
    let sexp = sexp_of(source);
    assert!(
        !sexp.contains("postfix_expression"),
        "neither `&&` turns postfix across the comment-only line, sexp: {sexp}"
    );
    assert_eq!(
        sexp.matches("(infix_expression").count(),
        2,
        "both `&&` keep an infix right operand, sexp: {sexp}"
    );
}

#[test]
fn slash_operator_before_annotation_and_enum_stays_postfix() {
    // `@` starts a definition, never an operand, and `enum Name` opens
    // one when a name follows — with or without a line comment after the
    // slash-tailed operator.
    let source = concat!(
        "object T {\n",
        "  val x = 2\n",
        "  val a = x / // note\n",
        "  @deprecated val y = 0\n",
        "  val b = x /\n",
        "  enum E { case A }\n",
        "  val z = a\n",
        "}\n",
    );
    run_clean(source);
    let sexp = sexp_of(source);
    assert!(
        sexp.matches("(postfix_expression").count() >= 2,
        "both slashes read as postfix before the definitions, sexp: {sexp}"
    );
    assert!(
        !sexp.contains("(infix_expression"),
        "no slash takes the definition as its operand, sexp: {sexp}"
    );
}

#[test]
fn export_before_expression_tail_stays_infix() {
    // `export` is a plain Scala 2 identifier, but `match` continues the
    // operand instead of opening an export path — with or without a block
    // comment between them.
    let source = concat!(
        "object T {\n",
        "  def m: Int = {\n",
        "    val a = 10 / export /* note */ match { case 2 => 1; case _ => 0 }\n",
        "    val b = 10 / export match { case 2 => 1; case _ => 0 }\n",
        "    a + b\n",
        "  }\n",
        "}\n",
    );
    run_clean(source);
    let sexp = sexp_of(source);
    assert!(
        !sexp.contains("(export_declaration"),
        "no `export` opens a declaration before `match`, sexp: {sexp}"
    );
    assert_eq!(
        sexp.matches("(infix_expression").count(),
        3,
        "both divisions and the final sum stay infix, sexp: {sexp}"
    );
}

#[test]
fn using_clause_with_tuple_type_after_postfix() {
    // A `using` clause with tuple context parameters is a definition even
    // when it follows a line-ending postfix operator.
    let source = concat!(
        "object T {\n",
        "  val flags = \"x\"\n",
        "  val cached = flags !\n",
        "  given (using (Int, Int)): String = \"a\"\n",
        "}\n",
    );
    run_clean(source);
    let sexp = sexp_of(source);
    assert!(
        sexp.contains("(postfix_expression"),
        "`!` stays postfix before the given definition, sexp: {sexp}"
    );
}

#[test]
fn using_clause_with_parenthesized_and_literal_types_after_postfix() {
    // Parenthesized and literal singleton types open a `using` clause the
    // same way a plain type name does.
    let source = concat!(
        "object T {\n",
        "  val flags = \"x\"\n",
        "  val cached = flags !\n",
        "  given (using (Int, String)): String = \"v\"\n",
        "  val other = flags !\n",
        "  given (using 42): Int = 1\n",
        "}\n",
    );
    run_clean(source);
    let sexp = sexp_of(source);
    assert_eq!(
        sexp.matches("(postfix_expression").count(),
        2,
        "both `!` stay postfix before the given definitions, sexp: {sexp}"
    );
}

#[test]
fn blank_line_inside_call_parens_keeps_the_infix() {
    // Newlines — blank or not — never separate statements inside
    // parentheses, so the operator still takes its operand from the
    // next line.
    let source = concat!(
        "object T {\n",
        "  def f(a: Int, b: Int): Int = g(a +\n",
        "\n",
        "    b)\n",
        "  def g(x: Int): Int = x\n",
        "}\n",
    );
    run_clean(source);
    let sexp = sexp_of(source);
    assert!(
        !sexp.contains("postfix_expression"),
        "`+` stays infix across the blank line in parens, sexp: {sexp}"
    );
    assert!(
        sexp.contains("right: (identifier)"),
        "`b` is still the right operand, sexp: {sexp}"
    );
}

#[test]
fn blank_line_inside_grouped_parens_keeps_the_infix() {
    // The same transparency holds for a parenthesized group: the blank
    // line must not turn the operator postfix.
    let source = concat!("object T {\n", "  val n = (1 +\n", "\n", "    2)\n", "}\n",);
    run_clean(source);
    let sexp = sexp_of(source);
    assert!(
        !sexp.contains("postfix_expression"),
        "`+` stays infix across the blank line in the group, sexp: {sexp}"
    );
}
