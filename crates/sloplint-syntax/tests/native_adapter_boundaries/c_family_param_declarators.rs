//! Parameter names and types of C-family function definitions.
//!
//! A parameter whose declarator is a function pointer or an unnamed pointer is not its bare
//! base type: `int (*cmp)(...)` is not an `int`, and an unnamed `void *` is not `void`.

use crate::support::{C_FAMILY, with_facts};
use sloplint_core::lang::Lang;

/// `(name, type)` of every parameter of the function called `function`.
fn params_of(lang: Lang, path: &str, source: &str, function: &str) -> Vec<(String, String)> {
    with_facts(lang, path, source, |facts| {
        let found = facts
            .functions
            .iter()
            .find(|f| f.name == function)
            .unwrap_or_else(|| panic!("{path} has function {function}"));
        found
            .params
            .iter()
            .map(|p| (p.name.to_string(), p.ty.unwrap_or("").to_string()))
            .collect()
    })
}

#[test]
fn function_pointer_parameter_keeps_its_name_and_is_not_an_int() {
    let source = "void sort(int n, int sz, int (*cmp)(const void *, const void *)) {
    (void)n;
}
";
    for (lang, path) in C_FAMILY {
        let params = params_of(lang, path, source, "sort");
        assert_eq!(params.len(), 3, "{path}: {params:?}");
        assert_eq!(params[0], ("n".to_string(), "int".to_string()), "{path}");
        assert_eq!(params[1], ("sz".to_string(), "int".to_string()), "{path}");
        assert_eq!(params[2].0, "cmp", "{path}: {params:?}");
        assert!(
            params[2].1.starts_with("int (*cmp)("),
            "{path}: function pointer typed as its return type: {params:?}"
        );
    }
}

#[test]
fn unnamed_pointer_parameters_keep_their_pointer_type() {
    let source = "void ptrs(int *, void *, int a) {\n}\n";
    for (lang, path) in C_FAMILY {
        let params = params_of(lang, path, source, "ptrs");
        assert_eq!(
            params,
            vec![
                (String::new(), "int *".to_string()),
                (String::new(), "void *".to_string()),
                ("a".to_string(), "int".to_string()),
            ],
            "{path}"
        );
    }
}

#[test]
fn named_data_pointers_and_void_parameter_lists_are_unchanged() {
    let source = "void ctl(int n, char *s, void *cmp) {
    (void)n;
}

void none(void) {
}
";
    for (lang, path) in C_FAMILY {
        assert_eq!(
            params_of(lang, path, source, "ctl"),
            vec![
                ("n".to_string(), "int".to_string()),
                ("s".to_string(), "char *".to_string()),
                ("cmp".to_string(), "void *".to_string()),
            ],
            "{path}"
        );
        assert!(
            params_of(lang, path, source, "none").is_empty(),
            "{path}: `(void)` is an empty parameter list"
        );
    }
}
