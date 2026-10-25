//! Starlark nested tuple assignment targets at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{
    DEPTH, binding_names, nested_tuple, on_default_stack, tuple_leaves, with_facts,
};

/// A tuple target nested at every level declares each leaf, in source order.
#[test]
fn nested_tuple_targets_declare_every_leaf_in_order() {
    on_default_stack(|| {
        let source = format!("def g():\n    {} = x\n", nested_tuple(DEPTH));
        with_facts(Lang::Starlark, "defs.bzl", &source, |facts| {
            assert_eq!(binding_names(facts), tuple_leaves(DEPTH));
        });
    });
}
