//! Scala call-site member facts: forced overrides, arrow-rename imports, single-variant matches, and string static runs.

use sloplint_core::facts::ArgShape;
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
fn override_without_extends_is_forced() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "case class P(x: Int) {\n  override def toString: String = \"P\"\n  def double(): Int = x * 2\n}\ntrait Shape {\n  def area(scale: Boolean, rounded: Boolean): Double\n}\nclass Sq extends Shape {\n  def area(scale: Boolean, rounded: Boolean): Double = 1.0\n  def other(): Int = 1\n}\nabstract class Base {\n  def need(x: Boolean, y: Boolean): Int\n}\nclass Impl extends Base {\n  def need(x: Boolean, y: Boolean): Int = 1\n  final def fin(x: Boolean, y: Boolean): Int = 2\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/P.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let named = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "toString")
        .expect("has toString");
    assert!(
        named.signature_forced,
        "override is forced without an extends clause"
    );
    let plain = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "double")
        .expect("has double");
    assert!(!plain.signature_forced, "plain method stays unforced");
    let area = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "area")
        .expect("has area");
    assert!(
        !area.signature_forced,
        "unmarked trait implementation owns its signature"
    );
    let other = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "other")
        .expect("has other");
    assert!(
        !other.signature_forced,
        "new method on an implementing class stays unforced"
    );
    let need = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "need")
        .expect("has need");
    assert!(
        !need.signature_forced,
        "abstract-class implementation without override is not forced"
    );
    let fin = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "fin")
        .expect("has fin");
    assert!(
        !fin.signature_forced,
        "final without override is not forced"
    );
}

#[test]
fn arrow_rename_import_emits_both_names() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "import java.util.{List => JList, Map}\nimport java.util.HashMap\nimport com.acme.Widget\nobject I {\n  def one(): Int = 1\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/I.scala");
    let renamed = analysis
        .facts
        .imports
        .iter()
        .find(|i| i.name == "List")
        .expect("has renamed List import");
    assert_eq!(renamed.alias, Some("JList"), "arrow rename keeps the alias");
    assert_eq!(
        renamed.module.as_ref(),
        "java.util",
        "rename keeps the module path"
    );
    let plain = analysis
        .facts
        .imports
        .iter()
        .find(|i| i.name == "Map")
        .expect("has Map import");
    assert_eq!(plain.alias, None, "plain selector keeps no alias");
    let direct = analysis
        .facts
        .imports
        .iter()
        .find(|i| i.name == "HashMap")
        .expect("has direct import");
    assert_eq!(direct.alias, None, "direct import keeps no alias");
    assert_eq!(direct.external_root, None, "JDK root is stdlib");
    assert_eq!(renamed.external_root, None, "selector JDK root is stdlib");
    let third = analysis
        .facts
        .imports
        .iter()
        .find(|i| i.name == "Widget")
        .expect("has third-party import");
    assert_eq!(third.external_root, Some("com"), "third-party root is kept");
}

#[test]
fn single_variant_match_is_not_enum_like() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object M {\n  def one(x: Any): Int = x match {\n    case Color.Red => 1\n    case _ => 0\n  }\n  def two(x: Any): Int = x match {\n    case Color.Red => 1\n    case Color.Blue => 2\n    case _ => 0\n  }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/M.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let single = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.variant_count == 1)
        .expect("has single-variant arm");
    assert!(!single.enum_like, "one variant is not enum-like");
    let pair = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.variant_count == 2)
        .expect("has two-variant arm");
    assert!(pair.enum_like, "two variants stay enum-like");
}

#[test]
fn escape_text_counts_as_static_and_interpolation_with_static_is_concat() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let src = "object S {\n  def a(): Unit = {\n    logger.warn(\"Bad \\\"id\\\" value\")\n  }\n  def b(): Unit = {\n    logger.info(\"plain value\")\n  }\n  def c(id: String): Unit = {\n    logger.error(s\"Failed for $id\")\n  }\n  def d(id: String): Unit = {\n    logger.debug(s\"$id\")\n  }\n  def e(): Unit = {\n    logger.trace(\"a\\tb\")\n  }\n  def f(): Unit = {\n    logger.mark(s\"a\\tb\")\n  }\n}\n";
    let analysis = analyze_src(&mut Parsers::new(), &custom, src, "src/S.scala");
    assert!(analysis.parse_error.is_none(), "parses");
    let escaped = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.warn")
        .expect("has escaped call");
    assert!(
        matches!(escaped.args[0].shape, ArgShape::Str { longest_static: 14 }),
        "escape text counts as static"
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.info")
        .expect("has plain call");
    assert!(
        matches!(plain.args[0].shape, ArgShape::Str { longest_static: 11 }),
        "escape-free literal keeps its run"
    );
    let mixed = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.error")
        .expect("has interpolated call");
    assert!(
        matches!(mixed.args[0].shape, ArgShape::Concat { longest_static: 11 }),
        "static plus hole is concat"
    );
    let bare = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.debug")
        .expect("has bare interpolation call");
    assert!(
        matches!(bare.args[0].shape, ArgShape::Concat { longest_static: 0 }),
        "hole-only carries zero"
    );
    let tabbed = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.trace")
        .expect("has tabbed call");
    assert!(
        matches!(tabbed.args[0].shape, ArgShape::Str { longest_static: 3 }),
        "plain escape counts one static char"
    );
    let marked = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "logger.mark")
        .expect("has interpolated escape call");
    assert!(
        matches!(marked.args[0].shape, ArgShape::Concat { longest_static: 3 }),
        "interpolated escape counts the same run"
    );
}
