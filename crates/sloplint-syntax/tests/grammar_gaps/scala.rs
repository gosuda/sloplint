//! Valid Scala that the published tree-sitter-scala 0.26.3 grammar rejected.
//!
//! Every repro is valid input for `scalac 3.10.0 -Ystop-after:parser`. The
//! vendored grammar under `vendor/tree-sitter-scala` accepts it; the published
//! crate reports a parse error for each fix case. The control tests read facts
//! from the parsed tree, so a node-shape regression changes a fact and not only
//! the error status.

use std::sync::LazyLock;

use sloplint_core::facts::CallKind;
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

fn callees<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.calls.iter().map(|c| c.callee).collect()
}

fn function_names<'a>(analysis: &Analysis<'a>) -> Vec<&'a str> {
    analysis.facts.functions.iter().map(|f| f.name).collect()
}

#[test]
fn typed_term_parameter_polymorphic_function_still_parses() {
    let analysis = run_clean("object O:\n  val c: [T] => T => T = [T] => (x: T) => x\n");
    assert!(
        analysis.facts.calls.is_empty(),
        "polymorphic function value records no calls: {:?}",
        analysis.facts.calls
    );
}

#[test]
fn dangling_polymorphic_function_body_still_errors() {
    let analysis = run("object T:\n  val f = lift([X] =>)\n");
    assert!(
        analysis.parse_error.is_some(),
        "a missing polymorphic function body must not parse"
    );
}

#[test]
fn split_applied_if_condition_parses() {
    let source = concat!(
        "val x = Some(10)\n",
        "def f =\n",
        "   if x.exists\n",
        "         (x => x == 10) then\n",
        "      println(\"Yes\")\n",
        "   else\n",
        "      println(\"No\")\n",
    );
    let analysis = run_clean(source);
    assert!(
        callees(&analysis).contains(&"x.exists"),
        "applied if condition calls x.exists: {:?}",
        callees(&analysis)
    );
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn single_line_applied_if_condition_still_parses() {
    let source = concat!(
        "val x = Some(10)\n",
        "def f =\n",
        "   if x.exists(x => x == 10) then\n",
        "      println(\"Yes\")\n",
        "   else\n",
        "      println(\"No\")\n",
    );
    let analysis = run_clean(source);
    assert!(
        callees(&analysis).contains(&"x.exists"),
        "applied if condition calls x.exists: {:?}",
        callees(&analysis)
    );
    assert_eq!(function_names(&analysis), ["f"]);
}

#[test]
fn empty_new_template_with_end_marker_parses() {
    let source = "trait Foo\nval foo =\n  new Foo:\n  end new\n";
    let analysis = run_clean(source);
    assert!(
        analysis
            .facts
            .calls
            .iter()
            .any(|c| c.callee == "Foo" && c.kind == CallKind::New),
        "the `new Foo` instance call must surface, callees: {:?}",
        callees(&analysis)
    );
}

#[test]
fn comment_only_new_template_with_end_marker_parses() {
    let source = "trait Foo\nval foo =\n  new Foo:\n    // comment\n  end new\n";
    let analysis = run_clean(source);
    assert!(
        analysis
            .facts
            .calls
            .iter()
            .any(|c| c.callee == "Foo" && c.kind == CallKind::New),
        "the `new Foo` instance call must surface, callees: {:?}",
        callees(&analysis)
    );
}

#[test]
fn nonempty_new_template_with_end_marker_still_parses() {
    run_clean("trait Foo\nval foo =\n  new Foo:\n    val x = 1\n  end new\n");
}

#[test]
fn empty_new_template_without_end_marker_still_errors() {
    let source = concat!(
        "object T:\n",
        "  trait A:\n",
        "    val x = new Foo:\n",
        "  val y = 2\n",
        "  trait Foo\n",
    );
    let analysis = run(source);
    assert!(
        analysis.parse_error.is_some(),
        "a marker-less empty `new:` template must not parse"
    );
}

#[test]
fn infix_star_extractor_patterns_parse() {
    let source = concat!(
        "object Test:\n",
        "  case class *(a: Int, b: Int)\n",
        "  case class P(a: Int, b: Any)\n",
        "  def m(p: P, v: *): Unit =\n",
        "    val x * y = v\n",
        "    val P(a, b * c) = p\n",
        "    println(a)\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["m"]);
    assert!(
        callees(&analysis).contains(&"println"),
        "infix star extractor calls println: {:?}",
        callees(&analysis)
    );
}

#[test]
fn star_repeat_and_vararg_still_parse() {
    let source = concat!(
        "object Test:\n",
        "  def f(xs: Int*) = xs\n",
        "  def g(s: Any): Int =\n",
        "    s match\n",
        "      case Array(a: Int, _@_*) => a\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["f", "g"]);
}

#[test]
fn slash_tail_operator_names_parse() {
    // breeze / twitter-algebird define mul-level operators that end in `/`
    // after a slash-led head, e.g. `def /:/`. The external scanner never
    // claims a `/`-led token, so the regex partition must accept it.
    let source = concat!(
        "trait Foldable[F[_]] {\n",
        "  def /:/[B](b: B)(f: (B, B) => B): B\n",
        "  def |/[B](b: B): B\n",
        "  def +/[B](b: B): B\n",
        "}\n",
    );
    // Operator names land as `operator_identifier` heads, not `identifier`
    // function names; the contract is that the definitions parse cleanly.
    run_clean(source);
}

#[test]
fn unparenthesized_typed_lambda_parses() {
    // scalding writes `case c => a: Array[Long] => new W(a)`: a typed
    // parameter is a lambda in expression position, not an ascription —
    // the `=> new` tail cannot parse as a type.
    let source = concat!(
        "class W[A](a: A)\n",
        "object W {\n",
        "  def m(xs: Any) = xs match {\n",
        "    case c if classOf[Array[Long]].equals(c) =>\n",
        "      a: Array[Long] => new W(a)\n",
        "    case w => w\n",
        "  }\n",
        "  val f = (a: Long) => a\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["m"]);
}

#[test]
fn wildcard_typed_lambda_parses() {
    // algebird Aggregator: `{ _: Any => () }` — a wildcard may carry a type
    // ascription inside a braced lambda.
    let source = concat!(
        "object O {\n",
        "  def const[T](t: T) = prepareMonoid { _: Any => () }.andThenPresent(_ => t)\n",
        "  def prepareMonoid[A, B](f: A => B) = f\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["const", "prepareMonoid"]);
}

#[test]
fn keyword_and_operator_symbol_literals_parse() {
    // scalding writes `'row, 'col, 'val` column literals and `x == '*)`:
    // a quote head can name `val` or an operator.
    let source = concat!(
        "object M {\n",
        "  implicit def symbolToFields(x: Symbol) =\n",
        "    if (x == '*) x else x\n",
        "  val m = new Matrix('row, 'col, 'val, p)\n",
        "}\n",
        "class Matrix(a: Any, b: Any, c: Any, d: Any)\n",
    );
    let analysis = run_clean(source);
    assert_eq!(function_names(&analysis), ["symbolToFields"]);
}

#[test]
fn stray_semicolons_after_package_parse() {
    // breeze ships `package breeze.plot;;;` — scalac skips the extra `;`.
    let source = concat!("package breeze.plot;;;\n", "\n", "trait Dataset\n");
    run_clean(source);
}

#[test]
fn wildcard_self_type_in_new_template_parses() {
    // `new T { _: (A, B) => }` declares a wildcard self-type, not a lambda:
    // the template body's self_type must win over the block-argument read.
    let source = concat!(
        "trait T\n",
        "class K {\n",
        "  val m = new T { _: (Any, Any) => }\n",
        "  val n = new T { _: (Any, Any) => go() }\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert!(
        analysis.facts.calls.iter().any(|c| c.callee == "go"),
        "the self-type body keeps its statement calls: {:?}",
        analysis.facts.calls
    );
}

#[test]
fn spaced_quote_operator_still_errors() {
    // `' +` is not a symbol literal — the quote's operator name must be
    // adjacent to the `'`.
    let source = "object O {\n  val s = ' +\n}\n";
    assert!(
        run(source).parse_error.is_some(),
        "a spaced quote operator must not parse as a symbol literal"
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
