use std::sync::LazyLock;

use sloplint_core::facts::{AssertShape, PrimitiveKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Java,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn chained_callee_strips_receiver_call_text() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run() {\n    LoggerFactory.getLogger(Foo.class).error(\"boom\");\n    helper.render(1);\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let chained = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "error")
        .expect("has chained error call");
    assert!(
        !chained.callee.contains('('),
        "callee keeps no argument text"
    );
    assert_eq!(
        chained.callee_root, None,
        "receiver with a call has no root"
    );
    assert!(
        !chained.is_simple_path,
        "receiver with a call is not simple"
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "helper.render")
        .expect("has plain qualified call");
    assert_eq!(
        plain.callee, "helper.render",
        "pure path keeps its qualifier"
    );
    assert_eq!(
        plain.callee_root,
        Some("helper"),
        "pure path root is the receiver"
    );
    assert!(plain.is_simple_path, "pure path is simple");
}

#[test]
fn qualified_callee_is_simple_path_and_chained_is_not() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run() {\n    helper.render(1);\n    builder.add(1).build();\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let qualified = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "helper.render")
        .expect("has qualified call");
    assert!(
        qualified.is_simple_path,
        "identifiers joined by member access are simple"
    );
    assert_eq!(
        qualified.callee_root,
        Some("helper"),
        "qualified root is the receiver"
    );
    let chained = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "build")
        .expect("has chained build call");
    assert!(
        !chained.is_simple_path,
        "a receiver call breaks the simple path"
    );
    assert_eq!(chained.callee_root, None, "a receiver call has no root");
}

#[test]
fn constructor_callee_strips_type_arguments() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run() {\n    Object a = new HashMap<String, Integer>(4);\n    Object b = new Foo();\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let generic = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "HashMap")
        .expect("has generic constructor");
    assert_eq!(
        generic.callee_root,
        Some("HashMap"),
        "constructor root is the type name"
    );
    assert!(
        !generic.callee.contains('<'),
        "callee keeps no type arguments"
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|c| c.callee == "Foo")
        .expect("has plain constructor");
    assert_eq!(
        plain.callee_root,
        Some("Foo"),
        "plain constructor keeps its root"
    );
}

#[test]
fn array_initializer_argument_is_collection() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run() {\n    compute(new int[] { 1, 2, 3 });\n    compute(new int[3]);\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let with_init = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.callee == "compute"
                && c.args
                    .iter()
                    .any(|a| matches!(a.shape, sloplint_core::facts::ArgShape::Collection))
        })
        .expect("has collection initializer call");
    assert!(
        with_init
            .args
            .iter()
            .any(|a| matches!(a.shape, sloplint_core::facts::ArgShape::Collection)),
        "array initializer is a collection"
    );
    let sized = analysis
        .facts
        .calls
        .iter()
        .find(|c| {
            c.callee == "compute"
                && c.args
                    .iter()
                    .any(|a| matches!(a.shape, sloplint_core::facts::ArgShape::Other))
        })
        .expect("has sized array call");
    assert!(
        sized
            .args
            .iter()
            .any(|a| matches!(a.shape, sloplint_core::facts::ArgShape::Other)),
        "sized array without initializer stays other"
    );
}

#[test]
fn switch_two_constants_count_two_variants() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run(String s, String t) {\n    switch (s) {\n      case ACTIVE, IDLE -> run();\n      default -> stop();\n    }\n    switch (t) {\n      case ACTIVE -> run();\n      default -> stop();\n    }\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.wildcard_arms.len(),
        2,
        "both defaults are arms"
    );
    let two = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.variant_count == 2)
        .expect("has two-variant arm");
    assert!(two.enum_like, "two named constants are enum-like");
    let one = analysis
        .facts
        .wildcard_arms
        .iter()
        .find(|a| a.variant_count == 1)
        .expect("has one-variant arm");
    assert!(!one.enum_like, "one named constant is not enum-like");
}

#[test]
fn assert_true_bare_identifier_is_bare_name() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run(boolean flag) {\n    assertTrue(flag);\n    assertTrue(flag.isEmpty());\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let bare = analysis
        .facts
        .assertions
        .iter()
        .find(|a| a.shape == AssertShape::BareName)
        .expect("has bare-name assertion");
    assert!(!bare.comparison, "assertTrue does not compare");
    let other = analysis
        .facts
        .assertions
        .iter()
        .find(|a| a.shape == AssertShape::Other)
        .expect("has call-argument assertion");
    assert!(!other.comparison, "call argument stays other");
}

#[test]
fn assert_equals_string_message_counts_and_delta_does_not() {
    let analysis = run(
        "Case.java",
        "class A {\n  void run() {\n    assertEquals(\"reason\", 3, 4);\n    assertEquals(expected, 0.5, 0.01);\n    assertEquals(reason, 3, 4);\n  }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let with_message = analysis
        .facts
        .assertions
        .iter()
        .find(|a| a.has_message && a.comparison)
        .expect("has message-bearing equality");
    assert!(
        with_message.has_message,
        "string first argument is a message"
    );
    let delta = analysis
        .facts
        .assertions
        .iter()
        .filter(|a| a.comparison && !a.has_message)
        .count();
    assert_eq!(delta, 1, "only the floating point delta stays unflagged");
    let identified = analysis
        .facts
        .assertions
        .iter()
        .filter(|a| a.comparison && a.has_message)
        .count();
    assert_eq!(identified, 2, "string and integer identifier messages flag");
}

#[test]
fn interface_method_without_body_emits_function() {
    let analysis = run(
        "Case.java",
        "interface Store {\n  void setVerbose(boolean on);\n}\nclass Impl implements Store {\n  public void setVerbose(boolean on) {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let decl = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "setVerbose" && f.is_trait_item)
        .expect("has interface declaration");
    assert_eq!(
        decl.visibility,
        Visibility::Exported,
        "interface member is exported"
    );
    assert!(decl.is_trait_item, "interface member is a trait item");
    assert_eq!(decl.params.len(), 1, "one boolean parameter");
    assert_eq!(
        decl.params[0].primitive_kind,
        PrimitiveKind::Bool,
        "boolean maps to Bool"
    );
    let other = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "setVerbose" && !f.is_trait_item)
        .expect("has implementation");
    assert!(!other.is_trait_item, "implementation is not a trait item");
}

#[test]
fn supertype_marks_abstract_child_and_generic_child() {
    let analysis = run(
        "Case.java",
        "abstract class Base implements Shape {}\ninterface Sub extends Shape {}\nclass Box<T> implements Shape {}\nclass Plain implements Shape {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(analysis.facts.impls.len(), 4, "four supertype references");
    let abstracted = analysis
        .facts
        .impls
        .iter()
        .filter(|r| r.child_is_abstract)
        .count();
    assert_eq!(
        abstracted, 2,
        "abstract class and interface are abstract children"
    );
    let generic = analysis
        .facts
        .impls
        .iter()
        .find(|r| r.is_generic)
        .expect("has generic impl");
    assert!(generic.is_generic, "type-parameter child is generic");
    let plain = analysis
        .facts
        .impls
        .iter()
        .filter(|r| !r.is_generic && !r.child_is_abstract)
        .count();
    assert_eq!(plain, 1, "concrete non-generic child stays plain");
}

#[test]
fn anonymous_class_method_is_private() {
    let analysis = run(
        "Case.java",
        "public class Repo {\n  void load() {\n    listener = new Listener() {\n      public void onChange(boolean dirty) { save(dirty); }\n    };\n  }\n  public void top(boolean flag) {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let nested = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "onChange")
        .expect("has anonymous-class method");
    assert_eq!(
        nested.visibility,
        Visibility::Private,
        "method inside a method body is private"
    );
    let top = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "top")
        .expect("has top-level method");
    assert_eq!(
        top.visibility,
        Visibility::Exported,
        "public top-level method stays exported"
    );
}

#[test]
fn interface_member_type_is_exported() {
    let analysis = run(
        "Case.java",
        "public interface Api {\n  class Request {}\n}\npublic class Outer {\n  class Inner {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let request = analysis
        .facts
        .types
        .iter()
        .find(|t| t.name == "Request")
        .expect("has interface member type");
    assert_eq!(
        request.visibility,
        Visibility::Exported,
        "interface member type is implicitly public"
    );
    let inner = analysis
        .facts
        .types
        .iter()
        .find(|t| t.name == "Inner")
        .expect("has class member type");
    assert_eq!(
        inner.visibility,
        Visibility::Restricted,
        "class member type without modifier stays restricted"
    );
}

#[test]
fn only_static_string_main_is_entry_point() {
    let analysis = run(
        "Case.java",
        "class Cli {\n  void main(boolean verbose) {}\n}\npublic class App {\n  public static void main(String[] args) {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let plain = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "main" && !f.entry_point)
        .expect("has non-entry main");
    assert!(
        !plain.entry_point,
        "instance main with boolean param is not an entry point"
    );
    let entry = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "main" && f.entry_point)
        .expect("has program entry");
    assert!(
        entry.entry_point,
        "static String-array main is the entry point"
    );
}

#[test]
fn deprecated_and_decorated_marks_come_from_annotations() {
    let analysis = run(
        "Case.java",
        "class A {\n  @Deprecated\n  public static String greet(String n) { return render(n); }\n  @Bean\n  public String clock(String n) { return render(n); }\n  public String plain(String n) { return render(n); }\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let deprecated = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "greet")
        .expect("has deprecated forwarder");
    assert!(
        deprecated.is_deprecated,
        "Deprecated annotation marks deprecation"
    );
    assert!(deprecated.is_decorated, "any annotation marks decoration");
    let decorated = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "clock")
        .expect("has bean method");
    assert!(
        !decorated.is_deprecated,
        "Bean annotation is not deprecation"
    );
    assert!(decorated.is_decorated, "Bean annotation marks decoration");
    let plain = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "plain")
        .expect("has plain method");
    assert!(!plain.is_deprecated, "plain method is not deprecated");
    assert!(!plain.is_decorated, "plain method is not decorated");
}
