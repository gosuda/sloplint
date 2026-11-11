//! A `virtual` method or an `override`/`final` method has a name and signature that its
//! class hierarchy dictates, so its signature is forced; a non-virtual member's is not.

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

fn signature_forced(name: &str) -> bool {
    with_facts(Lang::Cpp, "case.cpp", SOURCE, |facts| {
        facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no function {name}"))
            .signature_forced
    })
}

#[test]
fn plain_virtual_methods_have_forced_signatures() {
    assert!(
        signature_forced("process"),
        "`virtual` without `override` forces the signature"
    );
    assert!(
        signature_forced("~Base"),
        "`virtual` destructor forces the signature"
    );
}

#[test]
fn override_methods_have_forced_signatures() {
    assert!(
        signature_forced("overrides"),
        "`override` specifier forces the signature"
    );
}

#[test]
fn non_virtual_members_and_free_functions_have_no_forced_signature() {
    assert!(
        !signature_forced("plain"),
        "non-virtual member has no forced signature"
    );
    assert!(
        !signature_forced("helper"),
        "free function has no forced signature"
    );
}
