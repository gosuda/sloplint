//! The sibling-`.mli` gate: a top-level `let` is public only while no interface
//! hides it, and the interface-bound marker and regions stay off either way.

use crate::support::{with_lone_facts, with_paired_facts};
use sloplint_core::facts::Visibility;

const SOURCE: &str = "let handle x = x\n";

fn lone_visibility(name: &str) -> Visibility {
    with_lone_facts("lib.ml", SOURCE, |facts| {
        facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no function {name}"))
            .visibility
    })
}

fn paired_visibility(name: &str) -> Visibility {
    with_paired_facts("lib.ml", SOURCE, |facts| {
        facts
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no function {name}"))
            .visibility
    })
}

#[test]
fn toplevel_binding_is_exported_without_sibling_interface() {
    assert_eq!(
        lone_visibility("handle"),
        Visibility::Exported,
        "`handle` is exposed without lib.mli"
    );
}

#[test]
fn sibling_interface_hides_toplevel_binding_to_private() {
    assert_eq!(
        paired_visibility("handle"),
        Visibility::Private,
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
            found.signature_forced,
            found.params.len(),
            found.missing_annotations.len(),
            facts.regions.is_empty(),
        )
    });
    let gated = with_paired_facts("lib.ml", SOURCE, |facts| {
        let found = facts
            .functions
            .iter()
            .find(|f| f.name == "handle")
            .expect("`handle` is recorded");
        (
            found.signature_forced,
            found.params.len(),
            found.missing_annotations.len(),
            facts.regions.is_empty(),
        )
    });
    assert_eq!(plain, gated, "only visibility may change with the gate");
    assert_eq!(
        plain,
        (false, 1, 2, true),
        "`handle` keeps one untyped parameter and no return type"
    );
}
