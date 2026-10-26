//! PHP object chains as callee roots at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, on_default_stack, with_facts, within_linear_budget};

/// A chain of member calls roots every call at its leftmost variable, in linear time.
///
/// Each call finds its root by following the objects below it, so an unmemoized walk
/// costs the sum of the chain lengths and blows the budget at this depth.
#[test]
fn member_call_chains_keep_their_callee_root_in_linear_time() {
    on_default_stack(|| {
        let source = format!("<?php $a{};\n", "->b()".repeat(DEPTH));
        within_linear_budget(|| {
            with_facts(Lang::Php, "chain.php", &source, |facts| {
                assert_eq!(facts.calls.len(), DEPTH);
                assert!(
                    facts.calls.iter().all(|call| call.callee_root == Some("a")),
                    "every link roots at a"
                );
            });
        });
    });
}
