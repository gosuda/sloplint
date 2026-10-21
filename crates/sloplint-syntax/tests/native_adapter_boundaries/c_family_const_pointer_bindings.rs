//! Const pointers are not mutable bindings or mutable globals in the C family.
//!
//! `char *const p` forbids rebinding `p`; `char *const *pp` and `char *volatile v` do not.
//! Only the pointer level that directly declares the name decides.

use crate::support::{C_FAMILY, with_facts};
use sloplint_core::facts::AmbientKind;
use sloplint_core::lang::Lang;

const SOURCE: &str = "char *global_mut = 0;
char *const global_const = 0;
char *const *global_ptr_to_const = 0;

void f(char *s) {
    char *const fixed = s;
    char *plain = s;
    char *const *via_const = &s;
    char *volatile vol = s;
    char *const first = s, *second = s;
    *fixed = 'x';
    *plain = 'y';
}
";

fn binding_names(lang: Lang, path: &str) -> Vec<String> {
    with_facts(lang, path, SOURCE, |facts| {
        facts.bindings.iter().map(|b| b.name.to_string()).collect()
    })
}

fn global_names(lang: Lang, path: &str) -> Vec<String> {
    with_facts(lang, path, SOURCE, |facts| {
        facts
            .ambient
            .iter()
            .filter(|a| a.kind == AmbientKind::GlobalMutable)
            .map(|a| a.name.to_string())
            .collect()
    })
}

#[test]
fn const_pointer_locals_are_not_mutable_bindings() {
    for (lang, path) in C_FAMILY {
        assert_eq!(
            binding_names(lang, path),
            ["plain", "via_const", "vol", "second"],
            "{path}: `fixed` and `first` are const pointers; the others can be rebound"
        );
    }
}

#[test]
fn const_pointer_globals_are_not_mutable_state() {
    for (lang, path) in C_FAMILY {
        assert_eq!(
            global_names(lang, path),
            ["global_mut", "global_ptr_to_const"],
            "{path}: `global_const` cannot be rebound"
        );
    }
}
