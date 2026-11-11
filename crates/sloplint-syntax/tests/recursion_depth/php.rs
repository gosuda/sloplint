//! PHP object chains as callee roots at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{on_default_stack, with_facts, within_linear_budget};

/// A chain of member calls stays linear: only the innermost call has a plain receiver
/// and keeps its root; every call over a call receiver has none.
#[test]
fn member_call_chains_keep_their_callee_root_in_linear_time() {
    on_default_stack(|| {
        within_linear_budget(|depth| {
            let source = format!("<?php $a{};\n", "->b()".repeat(depth));
            with_facts(Lang::Php, "chain.php", &source, |facts| {
                assert_eq!(facts.calls.len(), depth);
                let rooted = facts
                    .calls
                    .iter()
                    .filter(|call| call.callee_root == Some("a"));
                assert_eq!(rooted.count(), 1, "only the innermost link roots at a");
                assert!(
                    facts
                        .calls
                        .iter()
                        .all(|call| call.callee_root.is_none_or(|root| root == "a")),
                    "a call over a call receiver has no root"
                );
            });
        });
    });
}
