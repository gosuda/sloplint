//! Python scope placement: a `def` is a method only when its nearest enclosing scope
//! is a class, and a `def` inside any function body is never public.

use sloplint_core::lang::Lang;
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

fn signature(name: &str) -> (bool, Vec<String>) {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "case.py",
        source: SOURCE,
        is_test_file: false,
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
    (function.public, params)
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn helper_nested_in_a_method_is_private() {
    assert_eq!(signature("helper"), (false, names(&["flag"])));
}

#[test]
fn helper_nested_in_a_free_function_is_private() {
    assert_eq!(signature("inner"), (false, names(&["flag"])));
}

#[test]
fn method_of_a_class_local_to_a_function_is_private() {
    assert_eq!(signature("method"), (false, names(&["flag"])));
}

#[test]
fn class_members_stay_public_and_drop_the_receiver() {
    assert_eq!(signature("run"), (true, names(&["items"])));
    assert_eq!(signature("visible"), (true, names(&["flag"])));
    assert_eq!(signature("build"), (true, names(&["flag"])));
}

#[test]
fn method_under_a_class_level_branch_is_still_a_class_member() {
    assert_eq!(signature("conditional"), (true, names(&["flag"])));
}

#[test]
fn module_level_functions_stay_public() {
    assert_eq!(signature("outer"), (true, names(&["items"])));
    assert_eq!(signature("top"), (true, names(&["flag"])));
}
