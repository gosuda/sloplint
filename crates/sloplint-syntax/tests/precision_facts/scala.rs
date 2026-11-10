use sloplint_core::facts::{AssertShape, PrimitiveKind, ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SUITE: &str = r#"class ParserSuite extends AnyFunSuite {
  test("parses") {
    assert(1 == 1)
  }
}
"#;

const FLAT: &str = r#"class ParserSpec extends AnyFlatSpec {
  "Parser" should "parse" in {
    assert(true)
  }
}
"#;

const PLAIN: &str = r#"class Plain {
  def run(): Int = 1
}
"#;

const MEMBERS: &str = r#"trait Greeter {
  def greet(name: String): String = "hi"
}

class Loud extends Greeter {
  override def greet(name: String): String = name
  private def whisper(msg: String): String = msg
  def plain(flag: Boolean = true, count: Int, name: String, ratio: Double): Int = count
}
"#;

const MATCHES: &str = r#"def handle(v: Any): Int = v match {
  case Point(x, y) => 1
  case Line(a) => 2
  case _ => 0
}

def other(): Int = compute() match {
  case _ => 0
}
"#;

const CALLS: &str = r#"val x = make()
val y = obj.make()

def check(ok: Boolean, v: Int): Boolean = {
  assert(v == 1)
  assert(ok)
  assert(compute(v))
  true
}
"#;

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

fn test_regions(source: &str, path: &str) -> Vec<(u32, u32)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(&mut Parsers::new(), &custom, source, path);
    assert!(analysis.parse_error.is_none(), "parses");
    analysis
        .facts
        .regions
        .iter()
        .filter(|r| r.role == CodeRole::Test)
        .map(|r| (r.start_line, r.end_line))
        .collect()
}

#[test]
fn suite_class_emits_one_test_region_for_whole_body() {
    let regions = test_regions(SUITE, "src/ParserSuite.scala");
    assert_eq!(regions.len(), 1, "one region for the suite class");
    assert_eq!(regions[0], (1, 5), "region covers the whole class body");
}

#[test]
fn flatspec_style_body_is_covered_by_class_region() {
    let regions = test_regions(FLAT, "src/ParserSpec.scala");
    assert_eq!(regions.len(), 1, "one region for the flat spec class");
    assert_eq!(regions[0], (1, 5), "region covers should/in blocks");
}

#[test]
fn plain_class_emits_no_test_region() {
    let regions = test_regions(PLAIN, "src/Plain.scala");
    assert!(regions.is_empty(), "plain class stays library");
}

#[test]
fn member_visibility_forced_signatures_and_params() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(&mut Parsers::new(), &custom, MEMBERS, "src/M.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let greets: Vec<_> = analysis
        .facts
        .functions
        .iter()
        .filter(|f| f.name == "greet")
        .collect();
    assert_eq!(greets.len(), 2, "trait declaration and impl both present");
    let decl = greets
        .iter()
        .find(|f| f.is_trait_item)
        .expect("trait method is marked");
    assert_eq!(
        decl.visibility,
        Visibility::Exported,
        "trait method is visible"
    );
    assert!(!decl.signature_forced, "trait declaration is not forced");
    let imp = greets
        .iter()
        .find(|f| !f.is_trait_item)
        .expect("impl method is present");
    assert!(imp.signature_forced, "override impl is forced");
    assert_eq!(
        imp.visibility,
        Visibility::Exported,
        "override impl stays visible"
    );
    let whisper = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "whisper")
        .expect("has whisper");
    assert_eq!(
        whisper.visibility,
        Visibility::Private,
        "private method is private"
    );
    assert!(!whisper.signature_forced, "private method is not forced");
    let plain = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "plain")
        .expect("has plain");
    let named_param = |name: &str| {
        plain
            .params
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("has param {name}"))
    };
    assert!(named_param("flag").has_default, "flag has a default");
    assert!(named_param("flag").bool_default, "flag defaults to true");
    assert_eq!(
        named_param("flag").primitive_kind,
        PrimitiveKind::Bool,
        "flag is boolean"
    );
    assert!(!named_param("count").has_default, "count has no default");
    assert_eq!(
        named_param("count").primitive_kind,
        PrimitiveKind::Int,
        "count is integer"
    );
    assert_eq!(
        named_param("name").primitive_kind,
        PrimitiveKind::Str,
        "name is string"
    );
    assert_eq!(
        named_param("ratio").primitive_kind,
        PrimitiveKind::Float,
        "ratio is float"
    );
}

#[test]
fn match_arm_reports_scrutinee_and_variant_count() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(&mut Parsers::new(), &custom, MATCHES, "src/M.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.wildcard_arms.len(),
        2,
        "one wildcard arm per match"
    );
    let ident_arm = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.variant_count == 2)
        .expect("identifier match has two named siblings");
    assert_eq!(
        ident_arm.scrutinee,
        ScrutineeKind::Ident,
        "identifier scrutinee"
    );
    assert!(ident_arm.enum_like, "named siblings make it enum-like");
    assert!(!ident_arm.has_nullable_ctor, "no Some/None/Ok/Err siblings");
    let other_arm = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.variant_count == 0)
        .expect("call match has no named siblings");
    assert_eq!(
        other_arm.scrutinee,
        ScrutineeKind::Other,
        "call scrutinee is opaque"
    );
    assert!(!other_arm.enum_like, "lone wildcard is not enum-like");
}

#[test]
fn call_binding_and_assertion_shapes() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_src(&mut Parsers::new(), &custom, CALLS, "src/M.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "make")
        .expect("has plain make call");
    assert!(plain.initializes_binding, "val x = make() initializes");
    assert!(plain.is_simple_path, "bare make is a simple path");
    let member = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee.ends_with("make") && c.callee != "make")
        .expect("has member make call");
    assert!(member.initializes_binding, "val y = obj.make() initializes");
    assert!(
        !member.is_simple_path,
        "field-selected callee is not simple"
    );
    let mut top_eq = 0;
    let mut bare = 0;
    let mut other = 0;
    for a in &analysis.facts.assertions {
        match a.shape {
            AssertShape::TopEqNe => top_eq += 1,
            AssertShape::BareName => bare += 1,
            AssertShape::Other => other += 1,
        }
    }
    assert_eq!(top_eq, 1, "v == 1 is a top-level comparison");
    assert_eq!(bare, 1, "ok is a bare name");
    assert_eq!(other, 1, "compute(v) is opaque");
}
