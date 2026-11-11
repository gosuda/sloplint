use sloplint_core::facts::{
    AmbientKind, AssertShape, FileFacts, PrimitiveKind, ScrutineeKind, Visibility,
};
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
        assert!(!typed.enum_like, "one named variant is not enum-like");
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

#[test]
fn in_function_write_to_top_level_var_is_one_global_write() {
    with_facts(
        "var count = 0\nfun bump() {\ncount = 1\n}\n",
        "case.kt",
        |facts| {
            let mutables: Vec<_> = facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalMutable)
                .collect();
            assert_eq!(mutables.len(), 1, "one top-level var is a global mutable");
            assert_eq!(mutables[0].name, "count", "mutable keeps the declared name");
            let writes: Vec<_> = facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalWrite)
                .collect();
            assert_eq!(writes.len(), 1, "in-function write joins the top-level var");
            assert_eq!(writes[0].name, "count", "write keeps the assigned name");
            assert!(
                writes[0].in_function.is_some(),
                "write is attributed inside the function"
            );
        },
    );
}

#[test]
fn chained_callee_keeps_suffix_root_but_plain_paths() {
    with_facts(
        "fun chain(repo: Repo, id: Int): String {\nreturn repo.find(id).orEmpty()\n}\nfun direct(helper: Helper, n: Int): Int {\nreturn helper.render(n)\n}\nfun paren(svc: Svc, n: Int): Int {\nreturn (svc).render(n)\n}\n",
        "case.kt",
        |facts| {
            let chained = facts
                .calls
                .iter()
                .find(|c| c.callee == "orEmpty")
                .expect("chained call keeps its suffix");
            assert_eq!(
                chained.callee_root,
                Some("repo"),
                "root descends through the receiver call"
            );
            assert!(
                !chained.is_simple_path,
                "a callee through a call is never simple"
            );
            let inner = facts
                .calls
                .iter()
                .find(|c| c.callee == "repo.find")
                .expect("inner call keeps its path");
            assert!(inner.is_simple_path, "identifier joins stay simple paths");
            let plain = facts
                .calls
                .iter()
                .find(|c| c.callee == "helper.render")
                .expect("plain path callee");
            assert_eq!(
                plain.callee_root,
                Some("helper"),
                "plain path keeps its root"
            );
            assert!(plain.is_simple_path, "qualified names are simple paths");
            let paren = facts
                .calls
                .iter()
                .find(|c| c.callee == "render")
                .expect("parenthesized suffix callee");
            assert!(
                !paren.is_simple_path,
                "parenthesized receivers are not simple"
            );
            assert_eq!(
                paren.callee_root, None,
                "a parenthesized bottom is not an identifier"
            );
        },
    );
}

#[test]
fn interpolated_reflection_is_dynamic_but_literals_are_not() {
    with_facts(
        "fun load(kind: String): Class<*> {\nreturn Class.forName(\"com.app.$kind\")\n}\nfun known(): Class<*> {\nreturn Class.forName(\"com.app.Fixed\")\n}\n",
        "case.kt",
        |facts| {
            assert_eq!(
                facts.dynamic_symbols.len(),
                1,
                "only the interpolated name is dynamic"
            );
            assert_eq!(
                facts.dynamic_symbols[0].what, "reflection by name",
                "interpolated Class.forName reports"
            );
        },
    );
}

#[test]
fn when_arms_count_conditions_and_subjectless_is_not_enum_like() {
    with_facts(
        "fun handle(s: State): Int {\nreturn when (s) {\nState.ACTIVE, State.IDLE -> 1\nelse -> 0\n}\n}\nfun decide2(verbose: Boolean, debug: Boolean): Int {\nreturn when {\nverbose -> 1\ndebug -> 2\nelse -> 0\n}\n}\n",
        "case.kt",
        |facts| {
            assert_eq!(facts.wildcard_arms.len(), 2, "one else arm per when");
            let multi = facts
                .wildcard_arms
                .iter()
                .find(|a| a.scrutinee == ScrutineeKind::Ident)
                .expect("has subject arm");
            assert_eq!(
                multi.variant_count, 2,
                "comma-separated conditions count separately"
            );
            assert!(multi.enum_like, "subject arm with variants is enum-like");
            let bare = facts
                .wildcard_arms
                .iter()
                .find(|a| a.scrutinee == ScrutineeKind::Other)
                .expect("has subjectless arm");
            assert_eq!(bare.variant_count, 2, "subjectless conditions still count");
            assert!(!bare.enum_like, "subjectless arms name no enum type");
        },
    );
}

#[test]
fn assert_equals_delta_has_no_message_but_text_does() {
    with_facts(
        "fun checkDelta(x: Double): Boolean {\nassertEquals(1.0, x, 0.01)\nreturn true\n}\nfun checkMsg(a: Int, b: Int): Boolean {\nassertEquals(\"bad\", a, b)\nreturn true\n}\n",
        "case.kt",
        |facts| {
            assert_eq!(facts.assertions.len(), 2, "two equality assertions");
            let delta = facts
                .assertions
                .iter()
                .find(|a| !a.has_message)
                .expect("delta form has no message");
            assert!(delta.comparison, "delta form still compares");
            assert_eq!(
                delta.shape,
                AssertShape::TopEqNe,
                "delta form is an equality assertion"
            );
            let text = facts
                .assertions
                .iter()
                .find(|a| a.has_message)
                .expect("text-first form has a message");
            assert!(text.comparison, "text form still compares");
        },
    );
}

#[test]
fn primary_constructor_params_and_nested_main() {
    with_facts(
        "class Config(val verbose: Boolean)\nclass Cli {\nfun main(verbose: Boolean) {}\n}\nfun main() {}\n",
        "case.kt",
        |facts| {
            let ctor = facts
                .functions
                .iter()
                .find(|f| f.name == "constructor")
                .expect("primary constructor is a function");
            assert_eq!(ctor.params.len(), 1, "constructor keeps its parameter");
            assert_eq!(
                ctor.params[0].primitive_kind,
                PrimitiveKind::Bool,
                "boolean constructor parameter is Bool"
            );
            assert_eq!(
                ctor.visibility,
                Visibility::Exported,
                "public constructor is exported"
            );
            assert!(!ctor.entry_point, "a constructor is never an entry point");
            let nested = facts
                .functions
                .iter()
                .find(|f| f.name == "main" && !f.entry_point)
                .expect("nested main exists");
            assert_eq!(
                nested.visibility,
                Visibility::Exported,
                "member main of a public class is exported"
            );
            assert!(
                facts
                    .functions
                    .iter()
                    .any(|f| f.name == "main" && f.entry_point),
                "top-level main stays the entry point"
            );
        },
    );
}

#[test]
fn object_methods_destructuring_and_generic_impls() {
    with_facts(
        "fun make(): Listener = object : Listener {\nfun helper(x: Int) = x\n}\nopen class Shape\nclass Box<T> : Shape()\nclass Impl : Shape()\nfun pair(): Int {\nval (a, b) = compute(1, 2)\nreturn a + b\n}\nfun single(): Int {\nval v = compute(1, 2)\nreturn v\n}\nfun compute(a: Int, b: Int): Int {\nreturn a + b\n}\n",
        "case.kt",
        |facts| {
            let helper = facts
                .functions
                .iter()
                .find(|f| f.name == "helper")
                .expect("anonymous object method exists");
            assert!(helper.is_method, "object literal body is a type body");
            let make = facts
                .functions
                .iter()
                .find(|f| f.name == "make")
                .expect("factory exists");
            assert!(!make.is_method, "top-level factory is not a method");
            assert_eq!(facts.impls.len(), 2, "two Shape impls");
            assert!(
                facts.impls.iter().any(|i| i.is_generic),
                "generic child marks its impl generic"
            );
            assert!(
                facts.impls.iter().any(|i| !i.is_generic),
                "plain child keeps a concrete impl"
            );
            let bound: Vec<_> = facts
                .calls
                .iter()
                .filter(|c| c.callee == "compute")
                .map(|c| c.initializes_binding)
                .collect();
            assert_eq!(bound.len(), 2, "two compute calls");
            assert!(
                bound.iter().any(|b| !b),
                "destructuring is not a plain binding"
            );
            assert!(bound.iter().any(|b| *b), "plain val keeps its initializer");
        },
    );
}
