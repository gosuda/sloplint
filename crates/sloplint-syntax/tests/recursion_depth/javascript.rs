//! JavaScript member, subscript, and parenthesized callee roots, write roots, and destructuring
//! leaves at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, binding, binding_names, on_default_stack, with_facts};

/// Member, parenthesized, and subscript callees all root at their leftmost identifier.
#[test]
fn member_call_chains_keep_their_callee_roots() {
    on_default_stack(|| {
        let members = format!("{}f();\n", "a.".repeat(DEPTH));
        let parens = format!("{}a{}();\n", "(".repeat(DEPTH), ")".repeat(DEPTH));
        let subscripts = format!("a{}();\n", "[0]".repeat(DEPTH));
        for source in [members, parens, subscripts] {
            with_facts(Lang::JavaScript, "case.js", &source, |facts| {
                assert_eq!(facts.calls[0].callee_root, Some("a"));
            });
        }
    });
}

/// A member assignment chain writes its declared leftmost identifier.
#[test]
fn member_write_chains_keep_their_declared_root() {
    on_default_stack(|| {
        let source = format!(
            "function g() {{\n    let a = 0;\n    {}a = 1;\n}}\n",
            "a.".repeat(DEPTH)
        );
        with_facts(Lang::JavaScript, "case.js", &source, |facts| {
            assert_eq!(
                binding(facts, "a").last_write_line,
                3,
                "the write roots at a"
            );
        });
    });
}

/// Nested object and array patterns declare their leaves, in source order.
#[test]
fn nested_destructuring_declares_every_leaf_in_order() {
    on_default_stack(|| {
        let objects = format!(
            "function g(y) {{\n    let {}x{} = y;\n}}\n",
            "{a: ".repeat(DEPTH),
            "}".repeat(DEPTH)
        );
        with_facts(Lang::JavaScript, "case.js", &objects, |facts| {
            assert_eq!(binding_names(facts), ["x"]);
        });
        let arrays = format!(
            "function g(y) {{\n    let {}x, z{} = y;\n}}\n",
            "[".repeat(DEPTH),
            "]".repeat(DEPTH)
        );
        with_facts(Lang::JavaScript, "case.js", &arrays, |facts| {
            assert_eq!(binding_names(facts), ["x", "z"]);
        });
    });
}
