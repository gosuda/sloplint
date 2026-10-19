//! A `virtual` method or an `override`/`final` method has a name and signature that its
//! class hierarchy dictates, so it is interface-bound; a non-virtual member is not.

use crate::support::with_facts;
use sloplint_core::lang::Lang;

const SOURCE: &str = "struct Base {
    virtual void process(bool a) {
        (void)a;
    }
    virtual ~Base() {}
    void plain(bool a) {
        (void)a;
    }
    void overrides(bool a) override {
        (void)a;
    }
};

void helper(bool a) {
    (void)a;
}
";

fn interface_bound(name: &str) -> bool {
    with_facts(Lang::Cpp, "case.cpp", SOURCE, |facts| {
        facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no function {name}"))
            .interface_bound
    })
}

#[test]
fn plain_virtual_methods_are_interface_bound() {
    assert!(interface_bound("process"), "`virtual` without `override`");
    assert!(interface_bound("~Base"), "`virtual` destructor");
}

#[test]
fn override_methods_stay_interface_bound() {
    assert!(interface_bound("overrides"), "`override` specifier");
}

#[test]
fn non_virtual_members_and_free_functions_are_not_interface_bound() {
    assert!(!interface_bound("plain"), "non-virtual member");
    assert!(!interface_bound("helper"), "free function");
}
