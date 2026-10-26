//! Ruby receiver chains as callee roots at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, on_default_stack, with_facts, within_linear_budget};

/// A chain of calls roots every call at its leftmost receiver, in time linear in the depth.
///
/// Each call finds its root by following the receivers below it, so an unmemoized walk
/// costs the sum of the chain lengths and blows the budget at this depth.
#[test]
fn call_chains_keep_their_callee_root_in_linear_time() {
    on_default_stack(|| {
        let source = format!("def g\n  a{}\nend\n", ".b".repeat(DEPTH));
        within_linear_budget(|| {
            with_facts(Lang::Ruby, "chain.rb", &source, |facts| {
                assert_eq!(
                    facts.calls.len(),
                    DEPTH,
                    "every nested call must be facted"
                );
                assert!(
                    facts.calls.iter().all(|call| call.callee_root == Some("a")),
                    "every call in the chain must root at a"
                );
            });
        });
    });
}

/// A chain stops at its first node that is not a call with a receiver, whether it is
/// reached from the outermost call or from an inner one.
#[test]
fn chain_roots_stop_at_the_first_node_that_is_not_a_receiver_call() {
    on_default_stack(|| {
        let source = [
            "def g",
            "  a.b.c(1)",
            "  Foo.new.run",
            "  x.y(1).z(2).w",
            "  bar(1).baz",
            "  (a.b).c",
            "end",
            "",
        ]
        .join("\n");
        let roots = with_facts(Lang::Ruby, "chain.rb", &source, |facts| {
            facts
                .calls
                .iter()
                .map(|call| (call.callee.to_owned(), call.callee_root.map(str::to_owned)))
                .collect::<Vec<_>>()
        });
        let expected = [
            ("a.b.c", Some("a")),
            ("a.b", Some("a")),
            ("Foo.new.run", Some("Foo")),
            ("Foo", Some("Foo")),
            ("x.y(1).z(2).w", Some("x")),
            ("x.y(1).z", Some("x")),
            ("x.y", Some("x")),
            ("bar(1).baz", Some("bar(1)")),
            ("bar", None),
            ("(a.b).c", Some("(a.b)")),
            ("a.b", Some("a")),
        ];
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(callee, root)| (callee.to_owned(), root.map(str::to_owned)))
            .collect();
        assert_eq!(roots, expected);
    });
}
