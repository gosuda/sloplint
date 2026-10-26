//! Python callee chains as receiver roots at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, on_default_stack, with_facts, within_linear_budget};

/// A chain of calls roots every call at its leftmost name, in time linear in the depth.
///
/// Each call finds its root by following the callees below it, so an unmemoized walk
/// costs the sum of the chain lengths and blows the budget at this depth.
#[test]
fn call_chains_keep_their_callee_root_in_linear_time() {
    on_default_stack(|| {
        let source = format!("a{}\n", ".f()".repeat(DEPTH));
        within_linear_budget(|| {
            with_facts(Lang::Python, "chain.py", &source, |facts| {
                assert_eq!(facts.calls.len(), DEPTH);
                assert!(
                    facts.calls.iter().all(|call| call.callee_root == Some("a")),
                    "every link roots at a"
                );
            });
        });
    });
}
