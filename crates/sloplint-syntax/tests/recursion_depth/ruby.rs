//! Ruby receiver chains as callee roots at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{on_default_stack, with_facts, within_linear_budget};

/// A chain of calls roots every call at its leftmost receiver, in time linear in the depth.
///
/// Each call finds its root by following the receivers below it, so an unmemoized walk
/// costs the sum of the chain lengths and blows the budget at this depth.
#[test]
fn call_chains_keep_their_callee_root_in_linear_time() {
    on_default_stack(|| {
        within_linear_budget(|depth| {
            let source = format!("def g\n  a{}\nend\n", ".b".repeat(depth));
            with_facts(Lang::Ruby, "chain.rb", &source, |facts| {
                assert_eq!(facts.calls.len(), depth, "every nested call must be facted");
                assert!(
                    facts.calls.iter().all(|call| call.callee_root == Some("a")),
                    "every call in the chain must root at a"
                );
            });
        });
    });
}

/// A chain roots at its leftmost bare receiver; a chain that bottoms at a call
/// with no receiver gives the outer call no bare root, while the bare call
/// itself roots at its method name.
#[test]
fn chain_roots_are_bare_identifiers_or_none() {
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
            ("w", Some("x")),
            ("z", Some("x")),
            ("x.y", Some("x")),
            ("baz", None),
            ("bar", Some("bar")),
            ("c", None),
            ("a.b", Some("a")),
        ];
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(callee, root)| (callee.to_owned(), root.map(str::to_owned)))
            .collect();
        assert_eq!(roots, expected);
    });
}
