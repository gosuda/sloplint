//! The sibling-`.mli` gate: a top-level `let` is public only while no interface
//! hides it, and the interface-bound and test flags stay off either way.

use crate::support::{with_lone_facts, with_paired_facts};

const SOURCE: &str = "let handle x = x\n";

fn lone_public(name: &str) -> bool {
    with_lone_facts("lib.ml", SOURCE, |facts| {
        facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no function {name}"))
            .public
    })
}

fn paired_public(name: &str) -> bool {
    with_paired_facts("lib.ml", SOURCE, |facts| {
        facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no function {name}"))
            .public
    })
}

#[test]
fn toplevel_binding_is_public_without_sibling_interface() {
    assert!(lone_public("handle"), "`handle` is exposed without lib.mli");
}

#[test]
fn sibling_interface_hides_toplevel_binding() {
    assert!(
        !paired_public("handle"),
        "`handle` is hidden once lib.mli exists"
    );
}

#[test]
fn interface_gate_changes_only_visibility() {
    let plain = with_lone_facts("lib.ml", SOURCE, |facts| {
        let found = facts
            .functions
            .iter()
            .find(|f| f.name == "handle")
            .expect("`handle` is recorded");
        (
            found.interface_bound,
            found.in_test,
            found.params.len(),
            found.missing_annotations.len(),
        )
    });
    let gated = with_paired_facts("lib.ml", SOURCE, |facts| {
        let found = facts
            .functions
            .iter()
            .find(|f| f.name == "handle")
            .expect("`handle` is recorded");
        (
            found.interface_bound,
            found.in_test,
            found.params.len(),
            found.missing_annotations.len(),
        )
    });
    assert_eq!(plain, gated, "only `public` may change with the gate");
    assert_eq!(
        plain,
        (false, false, 1, 2),
        "`handle` keeps one untyped parameter and no return type"
    );
}
