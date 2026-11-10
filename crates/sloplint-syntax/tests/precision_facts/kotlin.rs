use sloplint_core::facts::{AssertShape, FileFacts, PrimitiveKind, ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"import foo.Bar
import foo.*

fun greet(name: String, count: Int = 3, flag: Boolean = true, vararg rest: String): String {
    return name
}

private fun hidden(): Int {
    return 0
}

internal fun internalHelper(): Int {
    return 0
}

fun main() {
    val v = greet("a")
    greet("b")
}

open class Base {
    open fun render(): String {
        return "x"
    }
}

class Child : Base() {
    override fun render(): String {
        return "y"
    }
}

fun checkIt(flag: Boolean) {
    assertEquals(1, 2)
    assertTrue(flag)
    fail("unreached")
}

fun decide(x: Any): Int {
    return when (x) {
        is String -> 1
        else -> 0
    }
}

fun decidePlain(x: Int): Int {
    return when {
        x > 1 -> 1
        else -> 0
    }
}
"#;

fn with_facts(source: &str, path: &str, check: impl FnOnce(&FileFacts<'_>)) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Kotlin,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "parses");
    check(&analysis.facts);
}

#[test]
fn default_functions_are_exported_private_are_not() {
    with_facts(SOURCE, "case.kt", |facts| {
        let greet = facts
            .functions
            .iter()
            .find(|f| f.name == "greet")
            .expect("has greet");
        assert_eq!(
            greet.visibility,
            Visibility::Exported,
            "default visibility is exported"
        );
        let hidden = facts
            .functions
            .iter()
            .find(|f| f.name == "hidden")
            .expect("has hidden");
        assert_eq!(
            hidden.visibility,
            Visibility::Private,
            "private fun is private"
        );
        let internal = facts
            .functions
            .iter()
            .find(|f| f.name == "internalHelper")
            .expect("has internalHelper");
        assert_eq!(
            internal.visibility,
            Visibility::Restricted,
            "internal fun is restricted"
        );
    });
}

#[test]
fn override_forces_signature_open_method_does_not() {
    with_facts(SOURCE, "case.kt", |facts| {
        let forced = facts
            .functions
            .iter()
            .filter(|f| f.name == "render")
            .find(|f| f.signature_forced)
            .expect("has forced render");
        assert!(forced.signature_forced, "override states the signature");
        let open = facts
            .functions
            .iter()
            .filter(|f| f.name == "render")
            .find(|f| !f.signature_forced)
            .expect("has open render");
        assert!(
            !open.signature_forced,
            "open method leaves the signature free"
        );
    });
}

#[test]
fn main_is_entry_point_other_functions_are_not() {
    with_facts(SOURCE, "case.kt", |facts| {
        let main = facts
            .functions
            .iter()
            .find(|f| f.name == "main")
            .expect("has main");
        assert!(main.entry_point, "main is the entry point");
        let greet = facts
            .functions
            .iter()
            .find(|f| f.name == "greet")
            .expect("has greet");
        assert!(
            !greet.entry_point,
            "ordinary function is not an entry point"
        );
    });
}

#[test]
fn params_record_defaults_variadics_and_primitive_kinds() {
    with_facts(SOURCE, "case.kt", |facts| {
        let greet = facts
            .functions
            .iter()
            .find(|f| f.name == "greet")
            .expect("has greet");
        assert_eq!(greet.params.len(), 4, "four params recorded");
        let name = &greet.params[0];
        assert!(!name.has_default, "plain param has no default");
        assert_eq!(
            name.primitive_kind,
            PrimitiveKind::Str,
            "String normalizes to Str"
        );
        let count = &greet.params[1];
        assert!(count.has_default, "count has a default");
        assert!(!count.bool_default, "numeric default is not a bool default");
        assert_eq!(
            count.primitive_kind,
            PrimitiveKind::Int,
            "Int normalizes to Int"
        );
        let flag = &greet.params[2];
        assert!(flag.has_default, "flag has a default");
        assert!(flag.bool_default, "true default is a bool default");
        assert_eq!(
            flag.primitive_kind,
            PrimitiveKind::Bool,
            "Boolean normalizes to Bool"
        );
        let rest = &greet.params[3];
        assert!(rest.is_variadic, "vararg is variadic");
        assert!(!rest.has_default, "variadic has no default");
    });
}

#[test]
fn assertions_shape_equality_and_bare_names() {
    with_facts(SOURCE, "case.kt", |facts| {
        let eq = facts
            .assertions
            .iter()
            .find(|a| a.shape == AssertShape::TopEqNe)
            .expect("has equality assertion");
        assert!(eq.comparison, "assertEquals compares");
        assert!(!eq.has_message, "two-arg assert has no message slot");
        let bare = facts
            .assertions
            .iter()
            .find(|a| a.shape == AssertShape::BareName)
            .expect("has bare-name assertion");
        assert!(!bare.comparison, "assertTrue does not compare");
        let fail = facts
            .assertions
            .iter()
            .find(|a| a.shape == AssertShape::Other)
            .expect("has fallback assertion");
        assert!(fail.has_message, "fail with text carries a message");
    });
}

#[test]
fn when_else_reports_scrutinee_and_variants() {
    with_facts(SOURCE, "case.kt", |facts| {
        assert_eq!(facts.wildcard_arms.len(), 2, "two else arms");
        let typed = facts
            .wildcard_arms
            .iter()
            .find(|a| a.scrutinee == ScrutineeKind::Ident)
            .expect("has subject arm");
        assert!(typed.enum_like, "type-test siblings are enum-like");
        assert_eq!(typed.variant_count, 1, "one named sibling variant");
        assert!(
            !typed.has_nullable_ctor,
            "no nullable constructors in scope"
        );
        let plain = facts
            .wildcard_arms
            .iter()
            .find(|a| a.scrutinee == ScrutineeKind::Other)
            .expect("has subjectless arm");
        assert!(!plain.enum_like, "condition arms are not enum-like");
        assert_eq!(plain.variant_count, 0, "no named sibling variants");
    });
}
