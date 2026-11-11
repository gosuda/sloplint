//! Python receiver-call chains have no root at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{on_default_stack, with_facts, within_linear_budget};

/// A chain of calls leaves every receiver link without a root, in time linear in the depth.
///
/// Each link whose receiver is itself a call bottoms out at that call, so only the
/// innermost `a.f()` keeps the leftmost name. An unmemoized walk costs the sum of the
/// chain lengths and blows the budget at this depth.
#[test]
fn call_chains_keep_their_callee_root_in_linear_time() {
    on_default_stack(|| {
        within_linear_budget(|depth| {
            let source = format!("a{}\n", ".f()".repeat(depth));
            with_facts(Lang::Python, "chain.py", &source, |facts| {
                assert_eq!(facts.calls.len(), depth, "one fact per chain link");
                assert!(
                    facts.calls[..depth - 1]
                        .iter()
                        .all(|call| call.callee_root.is_none()),
                    "receiver links have no root"
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

/// A 20,000-deep bracket target records its leaf without recursing.
///
/// Module-level destructuring walks the target with an explicit stack, so the
/// deepest leaf still lands in ambient facts on the default thread stack.
#[test]
fn deeply_nested_module_target_records_its_leaf() {
    on_default_stack(|| {
        let source = format!("{}a{} = 0\n", "[".repeat(20_000), "]".repeat(20_000));
        with_facts(Lang::Python, "deep.py", &source, |facts| {
            assert!(
                facts.ambient.iter().any(|a| a.kind
                    == sloplint_core::facts::AmbientKind::GlobalMutable
                    && a.name == "a"),
                "deep leaf is recorded"
            );
            assert!(
                !facts.ambient.iter().any(|a| a.name != "a"),
                "no phantom names escape the nest"
            );
        });
    });
}
