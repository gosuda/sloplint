use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn forced_of(source: &str) -> Vec<(String, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "main.rs",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "rust parses");
    analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name.to_owned(), f.signature_forced))
        .collect()
}

fn forced_named(functions: &[(String, bool)], name: &str) -> bool {
    functions
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("function {name} found"))
        .1
}

#[test]
fn generic_lifetime_trait_impl_method_is_forced() {
    let functions = forced_of(
        "struct ValueVisitor;\nimpl<'de> de::Visitor<'de> for ValueVisitor {\n    fn visit_some<D>(self, deserializer: D) -> bool {\n        let _ = deserializer;\n        true\n    }\n}\n",
    );
    assert!(
        forced_named(&functions, "visit_some"),
        "trait-impl method keeps a forced signature"
    );
}

#[test]
fn inherent_impl_method_is_not_forced() {
    let functions =
        forced_of("struct Counter;\nimpl Counter {\n    fn bump(&mut self) {\n    }\n}\n");
    assert!(
        !forced_named(&functions, "bump"),
        "inherent-impl method keeps a free signature"
    );
}

#[test]
fn nested_fn_inside_trait_impl_method_is_not_forced() {
    let functions = forced_of(
        "struct ValueVisitor;\nimpl<'de> de::Visitor<'de> for ValueVisitor {\n    fn visit_some<D>(self, deserializer: D) -> bool {\n        fn helper(ok: bool) -> bool {\n            ok\n        }\n        let _ = deserializer;\n        helper(true)\n    }\n}\n",
    );
    assert!(
        forced_named(&functions, "visit_some"),
        "outer trait-impl method keeps a forced signature"
    );
    assert!(
        !forced_named(&functions, "helper"),
        "nested fn keeps a free signature"
    );
}
