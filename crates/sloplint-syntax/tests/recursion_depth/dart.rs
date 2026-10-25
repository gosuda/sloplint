//! Dart member-chain roots for writes and calls at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, binding, on_default_stack, with_facts};

/// A member chain roots at its leftmost identifier, for writes and calls alike.
#[test]
fn member_chains_keep_their_roots() {
    on_default_stack(|| {
        let write = format!(
            "void g() {{\n  var a = 0;\n  {}a = 1;\n}}\n",
            "a.".repeat(DEPTH)
        );
        with_facts(Lang::Dart, "case.dart", &write, |facts| {
            assert_eq!(
                binding(facts, "a").last_write_line,
                3,
                "the write roots at a"
            );
        });
        let call = format!("void g() {{\n  {}f();\n}}\n", "a.".repeat(DEPTH));
        with_facts(Lang::Dart, "case.dart", &call, |facts| {
            assert_eq!(facts.calls[0].callee_root, Some("a"));
        });
    });
}
