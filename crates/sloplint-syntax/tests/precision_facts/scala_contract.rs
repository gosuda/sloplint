use sloplint_core::facts::{AmbientKind, ArgShape, Stmt};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn analyze_src<'s>(
    parsers: &mut Parsers<'_>,
    custom: &'s CustomRules,
    source: &'s str,
    path: &'s str,
) -> sloplint_syntax::Analysis<'s> {
    let input = FileInput {
        lang: Lang::Scala,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(parsers, &input, custom)
}

#[test]
fn generic_and_deep_receiver_chains_keep_outer_segment() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object D {\n  def a(): Unit = {\n    Log[Ctx].error(\"generic\")\n  }\n  def b(): Unit = {\n    services.getLogger().underlying.error(\"deeper message\")\n  }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/D.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let generic = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.callee == "error" && matches!(c.args[0].shape, ArgShape::Str { longest_static: 7 })
        })
        .expect("has generic-receiver call");
    assert!(
        generic.is_simple_path,
        "generic application is transparent like a name"
    );
    assert_eq!(
        generic.callee_root,
        Some("Log"),
        "generic chain keeps its leftmost root"
    );
    let deep = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.callee == "error" && matches!(c.args[0].shape, ArgShape::Str { longest_static: 14 })
        })
        .expect("has deep-receiver call");
    assert!(
        !deep.is_simple_path,
        "call link in receiver breaks the path"
    );
    assert_eq!(
        deep.callee_root, None,
        "call receiver has no identifier root"
    );
    let inner = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "services.getLogger")
        .expect("has inner receiver call");
    assert!(inner.is_simple_path, "call-free receiver path stays simple");
}

#[test]
fn parenthesized_receiver_is_not_simple() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object P {\n  def greet(n: String): String = (Render).greet(n)\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/P.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "greet")
        .expect("has parenthesized call");
    assert!(!call.is_simple_path, "parenthesized link breaks the path");
    assert_eq!(call.callee_root, None, "parenthesized root is opaque");
}

#[test]
fn concat_carries_measured_static_runs() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object W {\n  def a(id: String): Unit = {\n    logger.error(s\"Failed for $id\")\n  }\n  def b(x: Int): Unit = {\n    logger.warn(s\"err $x\")\n  }\n  def c(id: String): Unit = {\n    logger.debug(s\"$id\")\n  }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/W.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let long = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.error")
        .expect("has long-static call");
    assert!(
        matches!(long.args[0].shape, ArgShape::Concat { longest_static: 11 }),
        "long static run is carried"
    );
    let short = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.warn")
        .expect("has short-static call");
    assert!(
        matches!(short.args[0].shape, ArgShape::Concat { longest_static: 4 }),
        "short static run is carried"
    );
    let empty = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.debug")
        .expect("has all-dynamic call");
    assert!(
        matches!(empty.args[0].shape, ArgShape::Concat { longest_static: 0 }),
        "all-dynamic run is zero"
    );
}

#[test]
fn tuple_literal_is_collection_unless_it_holds_a_closure() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object U {\n  def run(): Unit = {\n    compute(\n      (\n        1,\n        2,\n        3\n      )\n    )\n  }\n  def other(): Unit = {\n    apply((1, (x: Int) => x))\n  }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/U.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let listed = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "compute")
        .expect("has tuple call");
    assert!(
        matches!(listed.args[0].shape, ArgShape::Collection),
        "plain tuple is a collection"
    );
    let closed = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "apply")
        .expect("has closure tuple call");
    assert!(
        matches!(closed.args[0].shape, ArgShape::CodeBody),
        "closure tuple is a code body"
    );
}

#[test]
fn mixed_type_prefixes_are_not_enum_like() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object V {\n  def a(x: Any): Int = x match {\n    case Color.Red => 1\n    case Shape.Circle => 2\n    case _ => 0\n  }\n  def b(x: Any): Int = x match {\n    case Color.Red => 1\n    case Color.Blue => 2\n    case _ => 0\n  }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/V.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let mixed = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.variant_count == 2 && !a.enum_like)
        .expect("has mixed-prefix arm");
    assert!(!mixed.enum_like, "different type prefixes are not one type");
    let same = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.variant_count == 2 && a.enum_like)
        .expect("has same-prefix arm");
    assert!(same.enum_like, "shared prefix stays enum-like");
}

#[test]
fn writes_through_parameters_are_not_global() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "var config = Settings()\nvar total = 0\nobject Svc {\n  def apply(config: Settings): Unit = { config.retries = 3 }\n  def bump(): Unit = { total = 1 }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/Svc.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let writes: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .collect();
    assert_eq!(writes.len(), 1, "parameter write drops, plain write stays");
    assert_eq!(writes[0].name, "total", "unshadowed write is kept");
}

#[test]
fn nested_right_operand_static_run_is_measured() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object H {\n  def a(reason: String): Unit = {\n    logger.error(\"E: \" + (reason + \" while parsing the header\"))\n  }\n  def b(reason: String): Unit = {\n    logger.warn(\"E: \" + reason)\n  }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/H.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let nested = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.error")
        .expect("has nested call");
    assert!(
        matches!(
            nested.args[0].shape,
            ArgShape::Concat { longest_static: 25 }
        ),
        "parenthesized right operand run is measured"
    );
    let flat = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.warn")
        .expect("has flat call");
    assert!(
        matches!(flat.args[0].shape, ArgShape::Concat { longest_static: 3 }),
        "left-spine run still measured"
    );
}

#[test]
fn curried_outer_call_carries_no_callee() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object Q {\n  def load(cause: String): Unit = {\n    logger.error(\"Failed to load\")(cause)\n  }\n  def other(msg: String): Unit = {\n    logger.error(msg)\n  }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/Q.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let outer = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee.is_empty())
        .expect("has neutered outer call");
    assert!(!outer.is_simple_path, "outer call is not a path");
    assert_eq!(outer.callee_root, None, "outer call has no root");
    assert_eq!(outer.args.len(), 1, "outer call keeps the second list");
    let inner = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.error" && matches!(c.args[0].shape, ArgShape::Str { .. }))
        .expect("inner call keeps the literal");
    assert!(
        matches!(inner.args[0].shape, ArgShape::Str { longest_static: 14 }),
        "inner literal run is intact"
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.error" && matches!(c.args[0].shape, ArgShape::Ident(_)))
        .expect("has plain call");
    assert!(plain.is_simple_path, "uncurried call stays simple");
}

#[test]
fn curried_statement_callee_carries_no_argument_text() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object Z {\n  def load(): Unit = {\n    try {\n      work()\n    } catch {\n      case e: Exception =>\n        logger.warn(\"vital\")\n        logger.error(\"boom\")(e)\n        throw e\n    }\n  }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/Z.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(analysis.facts.handlers.len(), 1, "one catch handler");
    let stmts = &analysis.facts.handlers[0].stmts;
    assert_eq!(stmts.len(), 3, "two calls and a rethrow");
    assert!(
        matches!(
            stmts[0],
            Stmt::Call {
                callee: "logger.warn",
                ..
            }
        ),
        "plain call keeps callee"
    );
    assert!(
        matches!(stmts[1], Stmt::Call { callee: "", .. }),
        "curried call yields empty callee"
    );
    assert!(matches!(stmts[2], Stmt::Rethrow), "rethrow stays");
}

#[test]
fn comment_arguments_do_not_shadow_the_message() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object N {\n  def run(): Unit = {\n    logger.error(/* request id */ \"Failed to load configuration\")\n    logger.warn(// reason\n      \"disk almost full\")\n  }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/N.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let blocked = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.error")
        .expect("has block-comment call");
    assert_eq!(blocked.args.len(), 1, "block comment is not an argument");
    assert!(
        matches!(blocked.args[0].shape, ArgShape::Str { .. }),
        "literal stays the message"
    );
    let lined = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.warn")
        .expect("has line-comment call");
    assert_eq!(lined.args.len(), 1, "line comment is not an argument");
    assert!(
        matches!(lined.args[0].shape, ArgShape::Str { .. }),
        "literal stays the message"
    );
}

#[test]
fn compound_assignment_counts_as_a_write() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "var total = 0\nobject A {\n  def add(x: Int): Unit = { total += x }\n  def check(x: Int): Boolean = { total == x }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/A.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let writes: Vec<_> = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite)
        .collect();
    assert_eq!(writes.len(), 1, "only the compound write reports");
    assert_eq!(writes[0].name, "total", "compound write keeps the target");
}
