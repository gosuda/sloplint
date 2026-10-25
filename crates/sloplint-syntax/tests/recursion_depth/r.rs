//! R `$` extract chains as callee roots at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, on_default_stack, with_facts};

/// An extract chain roots at its leftmost identifier.
#[test]
fn extract_chains_keep_their_callee_root() {
    on_default_stack(|| {
        let source = format!("x <- {}f()\n", "a$".repeat(DEPTH));
        with_facts(Lang::R, "case.R", &source, |facts| {
            assert_eq!(facts.calls[0].callee_root, Some("a"));
        });
    });
}
