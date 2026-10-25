//! C# member-access roots and nested tuple declarations at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{
    DEPTH, binding, binding_names, nested_tuple, on_default_stack, tuple_leaves, with_facts,
};

/// A member-access chain roots at its leftmost identifier, for writes and calls alike.
#[test]
fn member_chains_keep_their_roots() {
    on_default_stack(|| {
        let write = format!(
            "class C {{\n    void G() {{\n        int a = 0;\n        a{} = 1;\n    }}\n}}\n",
            ".a".repeat(DEPTH)
        );
        with_facts(Lang::CSharp, "C.cs", &write, |facts| {
            assert_eq!(
                binding(facts, "a").last_write_line,
                4,
                "the write roots at a"
            );
        });
        let call = format!(
            "class C {{\n    void G() {{\n        a{}();\n    }}\n}}\n",
            ".a".repeat(DEPTH)
        );
        with_facts(Lang::CSharp, "C.cs", &call, |facts| {
            assert_eq!(facts.calls[0].callee_root, Some("a"));
        });
    });
}

/// A tuple pattern nested at every level declares each leaf, in source order.
#[test]
fn nested_tuple_patterns_declare_every_leaf_in_order() {
    on_default_stack(|| {
        let source = format!(
            "class C {{\n    void G() {{\n        var {} = x;\n    }}\n}}\n",
            nested_tuple(DEPTH)
        );
        with_facts(Lang::CSharp, "C.cs", &source, |facts| {
            assert_eq!(binding_names(facts), tuple_leaves(DEPTH));
        });
    });
}
