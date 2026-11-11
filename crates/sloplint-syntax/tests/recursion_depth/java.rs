//! Java receiver roots at 20,000 levels, and field-access write roots.
//!
//! A method chain computes each call's receiver root by walking its whole receiver, so a chain of
//! `n` calls costs `n²` steps. The depth case therefore nests the 20,000 levels under a single call
//! (a field-access receiver), and the method-chain case checks correctness on a short chain.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, binding, on_default_stack, with_facts, within_linear_budget};

/// A field-access receiver nested at every level roots the one call at the leftmost identifier.
#[test]
fn field_receiver_chains_keep_their_receiver_root() {
    on_default_stack(|| {
        let source = format!(
            "class C {{\n    void g() {{\n        x = a{}.m();\n    }}\n}}\n",
            ".a".repeat(DEPTH)
        );
        with_facts(Lang::Java, "C.java", &source, |facts| {
            assert_eq!(facts.calls.len(), 1, "one call");
            assert_eq!(facts.calls[0].callee_root, Some("a"));
        });
    });
}

/// Only the innermost link of a 20,000-link method chain roots at the leftmost identifier, in linear time.
///
/// Each link whose receiver is itself a call has no root per Ruling 17, so only the
/// innermost `a.m()` keeps `a`. An unmemoized walk costs the sum of the chain lengths
/// and blows the budget at this depth.
#[test]
fn method_chains_keep_their_receiver_root_in_linear_time() {
    on_default_stack(|| {
        within_linear_budget(|depth| {
            let source = format!(
                "class C {{\n    void g() {{\n        x = a{};\n    }}\n}}\n",
                ".m()".repeat(depth)
            );
            with_facts(Lang::Java, "C.java", &source, |facts| {
                assert_eq!(facts.calls.len(), depth, "one call per link");
                assert!(
                    facts.calls[..depth - 1]
                        .iter()
                        .all(|c| c.callee_root.is_none()),
                    "chained links have no root: {:?}",
                    facts
                        .calls
                        .iter()
                        .map(|c| c.callee_root)
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    facts.calls[depth - 1].callee_root,
                    Some("a"),
                    "innermost link roots at a"
                );
            });
        });
    });
}

/// Only the innermost call in a short method chain roots at the receiver's leftmost identifier.
#[test]
fn method_chains_keep_their_receiver_root() {
    with_facts(
        Lang::Java,
        "C.java",
        "class C {\n    void g() {\n        x = a.m().m().m();\n    }\n}\n",
        |facts| {
            assert_eq!(facts.calls.len(), 3, "one call per link");
            assert!(
                facts.calls[..2].iter().all(|c| c.callee_root.is_none()),
                "chained links have no root: {:?}",
                facts
                    .calls
                    .iter()
                    .map(|c| c.callee_root)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                facts.calls[2].callee_root,
                Some("a"),
                "innermost link roots at a"
            );
            assert_eq!(facts.calls[2].callee, "a.m", "innermost keeps its path");
            assert!(
                facts.calls[..2]
                    .iter()
                    .all(|c| c.callee == "m" && !c.is_simple_path),
                "chained links keep the bare member: {:?}",
                facts
                    .calls
                    .iter()
                    .map(|c| (c.callee, c.is_simple_path))
                    .collect::<Vec<_>>()
            );
        },
    );
}

/// A field-access chain assigned to a declared local writes that local.
#[test]
fn field_write_chains_keep_their_declared_root() {
    on_default_stack(|| {
        let source = format!(
            "class C {{\n    void g() {{\n        int a = 0;\n        a{} = 1;\n    }}\n}}\n",
            ".a".repeat(DEPTH)
        );
        with_facts(Lang::Java, "C.java", &source, |facts| {
            assert_eq!(
                binding(facts, "a").last_write_line,
                4,
                "the write roots at a"
            );
        });
    });
}
