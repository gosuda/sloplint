//! Python scope placement: a `def` is a method only when its nearest enclosing scope
//! is a class, and a `def` inside any function body is never public.

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"class Service:
    def run(self, items):
        def helper(flag: bool):
            return flag

        return helper(True)

    def visible(self, flag: bool):
        return flag

    @staticmethod
    def build(flag: bool):
        return flag

    if True:
        def conditional(self, flag: bool):
            return flag


def outer(items):
    def inner(flag: bool):
        return flag

    class Local:
        def method(self, flag: bool):
            return flag

    return inner(True)


def top(flag: bool):
    return flag
"#;

fn signature(name: &str) -> (Visibility, Vec<String>) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source: SOURCE,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "case.py parses");
    let function = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("case.py has function {name}"));
    let params = function.params.iter().map(|p| p.name.to_owned()).collect();
    (function.visibility, params)
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn helper_nested_in_a_method_is_private() {
    assert_eq!(signature("helper"), (Visibility::Private, names(&["flag"])));
}

#[test]
fn helper_nested_in_a_free_function_is_private() {
    assert_eq!(signature("inner"), (Visibility::Private, names(&["flag"])));
}

#[test]
fn method_of_a_class_local_to_a_function_is_private() {
    assert_eq!(signature("method"), (Visibility::Private, names(&["flag"])));
}

#[test]
fn class_members_stay_public_and_drop_the_receiver() {
    assert_eq!(signature("run"), (Visibility::Exported, names(&["items"])));
    assert_eq!(
        signature("visible"),
        (Visibility::Exported, names(&["flag"]))
    );
    assert_eq!(signature("build"), (Visibility::Exported, names(&["flag"])));
}

#[test]
fn method_under_a_class_level_branch_is_still_a_class_member() {
    assert_eq!(
        signature("conditional"),
        (Visibility::Exported, names(&["flag"]))
    );
}

#[test]
fn module_level_functions_stay_public() {
    assert_eq!(
        signature("outer"),
        (Visibility::Exported, names(&["items"]))
    );
    assert_eq!(signature("top"), (Visibility::Exported, names(&["flag"])));
}
