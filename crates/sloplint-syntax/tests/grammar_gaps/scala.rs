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
fn multi_pattern_val_and_var_definitions_parse() {
    // scalameta's Issue1872 semanticdb fixture binds a name and a pattern in a
    // single definition: `val a2_1, List(a2_2) = …`.
    let source = concat!(
        "object Test {\n",
        "  val a2_1, List(a2_2) = List(1, 2)\n",
        "  var e, List(f) = List(1, 2)\n",
        "  val a, b = (1, 2)\n",
        "}\n",
    );
    let analysis = run_clean(source);
    assert!(
        analysis.parse_error.is_none(),
        "multi-pattern val/var definitions must parse: {:?}",
        analysis.parse_error
    );
}

#[test]
fn multi_pattern_var_registers_each_binding() {
    // `var whole, Some(count) = …` repeats the `pattern` field; both names
    // must bind or later writes to `count` go untracked.
    let source = "object O {\n  def f() = {\n    var whole, Some(count) = Some(0)\n    count += 1\n    count\n  }\n}\n";
    let analysis = run_clean(source);
    let names: Vec<_> = analysis.facts.bindings.iter().map(|b| b.name).collect();
    assert!(
        names.contains(&"whole") && names.contains(&"count"),
        "every pattern field binds its names: {names:?}"
    );
}

#[test]
fn bare_infix_in_pattern_list_still_errors() {
    // `val a, b | c = 1` — a `|` alternative needs parens in a PatDef
    // comma list; `(b | c)` and `(b :: c)` stay legal.
    let source = "object O {\n  val a, b | c = 1\n}\n";
    assert!(
        run(source).parse_error.is_some(),
        "an unparenthesized | must not parse in a comma pattern list"
    );
    run_clean("object O {\n  val a, (b | c) = (1, 2)\n}\n");
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
