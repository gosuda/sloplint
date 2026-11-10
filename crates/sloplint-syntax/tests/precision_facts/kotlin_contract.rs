use sloplint_core::facts::{ArgShape, AssertShape, FileFacts, PrimitiveKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

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
fn interface_members_without_bodies_are_functions() {
    with_facts(
        "interface Store {\nfun setVerbose(on: Boolean)\n}\nfun full(x: Int): Int {\nreturn x\n}\n",
        "case.kt",
        |facts| {
            let member = facts
                .functions
                .iter()
                .find(|f| f.name == "setVerbose")
                .expect("body-less member is a function");
            assert_eq!(member.params.len(), 1, "member keeps its parameter");
            assert_eq!(
                member.params[0].primitive_kind,
                PrimitiveKind::Bool,
                "boolean member parameter is Bool"
            );
            assert!(member.is_trait_item, "interface member is a trait item");
            assert_eq!(
                member.visibility,
                Visibility::Exported,
                "interface member is exported"
            );
            let bodied = facts
                .functions
                .iter()
                .find(|f| f.name == "full")
                .expect("bodied function exists");
            assert!(
                !bodied.is_trait_item,
                "top-level function is not a trait item"
            );
        },
    );
}

#[test]
fn delta_keyed_on_numeric_third() {
    with_facts(
        "fun checkDelta(expected: Double, actual: Double): Boolean {\nassertEquals(expected, actual, 0.01)\nreturn true\n}\nfun checkMsg(a: Int, b: Int): Boolean {\nassertEquals(\"bad\", a, b)\nreturn true\n}\nfun checkVar(reason: String): Boolean {\nassertEquals(reason, 3, 4)\nreturn true\n}\n",
        "case.kt",
        |facts| {
            assert_eq!(facts.assertions.len(), 3, "three equality assertions");
            let delta = facts
                .assertions
                .iter()
                .find(|a| !a.has_message && a.shape == AssertShape::TopEqNe)
                .expect("variable-operand delta has no message");
            assert!(delta.comparison, "delta form still compares");
            let text = facts
                .assertions
                .iter()
                .find(|a| a.has_message)
                .expect("text-first form has a message");
            assert!(text.comparison, "text form still compares");
            let reasoned = facts.assertions.iter().filter(|a| !a.has_message).count();
            assert_eq!(
                reasoned, 2,
                "numeric third wins over a message variable, mirroring java"
            );
        },
    );
}

#[test]
fn call_of_call_uses_the_innermost_name() {
    with_facts(
        "fun boot(): String {\nreturn makeLogger(Foo::class)(\"boom\")\n}\nfun plain(): String {\nreturn greet(\"hi\")\n}\n",
        "case.kt",
        |facts| {
            let nested = facts
                .calls
                .iter()
                .find(|c| c.callee == "makeLogger")
                .expect("call of call keeps its name");
            assert!(
                !nested.is_simple_path,
                "a callee through a call is never simple"
            );
            let plain = facts
                .calls
                .iter()
                .find(|c| c.callee == "greet")
                .expect("plain callee exists");
            assert!(plain.is_simple_path, "bare names stay simple");
            assert!(
                facts.calls.iter().all(|c| !c.callee.contains('(')),
                "no callee carries argument text"
            );
        },
    );
}

#[test]
fn anonymous_object_members_are_private() {
    with_facts(
        "val listener = object : Any() {\nfun onChange(dirty: Boolean) {}\n}\nfun top(x: Boolean) {}\n",
        "case.kt",
        |facts| {
            let member = facts
                .functions
                .iter()
                .find(|f| f.name == "onChange")
                .expect("object member exists");
            assert!(member.is_method, "object literal body is a type body");
            assert_eq!(
                member.visibility,
                Visibility::Private,
                "anonymous object member is not public API"
            );
            let top = facts
                .functions
                .iter()
                .find(|f| f.name == "top")
                .expect("top-level function exists");
            assert_eq!(
                top.visibility,
                Visibility::Exported,
                "top-level function stays exported"
            );
        },
    );
}

#[test]
fn concat_carries_static_lengths() {
    with_facts(
        "fun longGreet(name: String): String {\nreturn greet(\"hello there, \" + name)\n}\nfun shortGreet(name: String): String {\nreturn greet(\"hi \" + name)\n}\nfun dynGreet(a: String, b: String): String {\nreturn greet(\"$a$b\")\n}\nfun plainMsg(): String {\nreturn greet(\"hello\")\n}\n",
        "case.kt",
        |facts| {
            let mut concats: Vec<u32> = facts
                .calls
                .iter()
                .filter(|c| c.callee == "greet")
                .filter_map(|c| match c.args.first().map(|a| &a.shape) {
                    Some(ArgShape::Concat { longest_static }) => Some(*longest_static),
                    _ => None,
                })
                .collect();
            concats.sort_unstable();
            assert_eq!(
                concats,
                vec![0, 3, 13],
                "hand-computed static runs: empty, short, long"
            );
            let plain = facts
                .calls
                .iter()
                .filter(|c| c.callee == "greet")
                .filter(|c| matches!(c.args.first().map(|a| &a.shape), Some(ArgShape::Str { .. })))
                .count();
            assert_eq!(plain, 1, "plain literal stays Str");
        },
    );
}

#[test]
fn collection_with_closure_is_code_body() {
    with_facts(
        "fun runCb(): String {\nreturn format(\n[\n{ x -> x },\n2,\n],\n)\n}\nfun runPlain(): String {\nreturn format(\n[\n1,\n2,\n],\n)\n}\n",
        "case.kt",
        |facts| {
            let body = facts
                .calls
                .iter()
                .filter(|c| c.callee == "format")
                .find(|c| matches!(c.args.first().map(|a| &a.shape), Some(ArgShape::CodeBody)))
                .expect("closure collection is code");
            assert!(
                body.args.first().is_some_and(|a| a.start_line < a.end_line),
                "closure collection still spans lines"
            );
            assert!(
                facts
                    .calls
                    .iter()
                    .filter(|c| c.callee == "format")
                    .any(|c| matches!(
                        c.args.first().map(|a| &a.shape),
                        Some(ArgShape::Collection)
                    )),
                "plain collection stays Collection"
            );
        },
    );
}

#[test]
fn nested_types_inherit_the_tightest_visibility() {
    with_facts(
        "private class Outer {\nclass Inner {\nfun configure(verbose: Boolean, force: Boolean) {}\n}\n}\nclass Open {\nclass Nested {\nfun setup(flag: Boolean) {}\n}\n}\n",
        "case.kt",
        |facts| {
            let hidden = facts
                .functions
                .iter()
                .find(|f| f.name == "configure")
                .expect("nested member exists");
            assert_eq!(
                hidden.visibility,
                Visibility::Private,
                "member under a private outer is private"
            );
            let shown = facts
                .functions
                .iter()
                .find(|f| f.name == "setup")
                .expect("open nested member exists");
            assert_eq!(
                shown.visibility,
                Visibility::Exported,
                "member under public outers stays exported"
            );
            let inner = facts
                .types
                .iter()
                .find(|t| t.name == "Inner")
                .expect("nested type exists");
            assert_eq!(
                inner.visibility,
                Visibility::Private,
                "nested type inherits the private outer"
            );
        },
    );
}

#[test]
fn only_override_forces_signature() {
    with_facts(
        "open class B {\nopen fun g(x: Boolean) {}\n}\nabstract class A {\nabstract fun f(x: Boolean)\n}\nclass C {\nfinal fun h(x: Boolean) {}\n}\nclass D : B() {\noverride fun g(x: Boolean) {}\n}\ninterface S {\nfun s(x: Boolean)\n}\n",
        "case.kt",
        |facts| {
            let forced: Vec<&str> = facts
                .functions
                .iter()
                .filter(|f| f.signature_forced)
                .map(|f| f.name)
                .collect();
            assert_eq!(forced.len(), 1, "one forced signature in the file");
            assert_eq!(
                forced[0], "g",
                "only the overrider is forced, bases stay actionable"
            );
            assert_eq!(facts.functions.len(), 5, "all five declarations emit");
        },
    );
}

#[test]
fn parenthesized_and_triple_quoted_operands_count() {
    with_facts(
        "fun parenLog(step: String): String {\nreturn log.error(\"boom: \" + (step + \" while connecting to the upstream service\"))\n}\nfun triGreet(name: String): String {\nreturn greet(\"\"\"hello there, friend \"\"\" + name)\n}\n",
        "case.kt",
        |facts| {
            let logged = facts
                .calls
                .iter()
                .find(|c| c.callee == "log.error")
                .expect("logger call exists");
            assert_eq!(
                logged.args.first().map(|a| &a.shape),
                Some(&ArgShape::Concat { longest_static: 41 }),
                "parenthesized operand holds the longest run"
            );
            let tri = facts
                .calls
                .iter()
                .find(|c| c.callee == "greet")
                .expect("triple-quoted call exists");
            assert_eq!(
                tri.args.first().map(|a| &a.shape),
                Some(&ArgShape::Concat { longest_static: 20 }),
                "triple-quoted operand concatenates"
            );
        },
    );
}
