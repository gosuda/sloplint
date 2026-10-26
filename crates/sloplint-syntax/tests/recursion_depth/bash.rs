//! Bash command substitutions nested at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, on_default_stack, with_facts, within_linear_budget};

/// A substitution nested at every level parses cleanly on a default stack, yields one call per
/// level, and analyzes within the linear budget.
///
/// This pins the syntax crate only: the Bash adapter and the walk. It held before the Ruby
/// chain fix too. The depth-squared cost of a deep Bash file under the strict rules sits in
/// callee matching in `sloplint-core`, which `analyze` never runs, so this test does not cover it.
#[test]
fn nested_command_substitutions_yield_one_call_per_level() {
    on_default_stack(|| {
        let source = format!("echo {}true{}\n", "$(".repeat(DEPTH), ")".repeat(DEPTH));
        within_linear_budget(|| {
            with_facts(Lang::Bash, "deep.sh", &source, |facts| {
                assert_eq!(facts.calls.len(), DEPTH + 1);
            });
        });
    });
}
