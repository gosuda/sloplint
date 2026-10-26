//! Go member-call chains and assertion tails at depth.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, on_default_stack, with_facts, within_linear_budget};

/// A member-call chain keeps every call while staying linear in depth.
///
/// Each assertion check used to walk the whole callee text per call; the tail
/// scan now reads only the last two segments, so the chain below costs
/// O(depth) instead of O(depth^2).
#[test]
fn call_chains_keep_every_call_in_linear_time() {
    on_default_stack(|| {
        let source = "package main\n\nfunc g() {\n\ta".to_owned() + &".B()".repeat(DEPTH) + "\n}\n";
        within_linear_budget(|| {
            with_facts(Lang::Go, "chain.go", &source, |facts| {
                assert_eq!(facts.calls.len(), DEPTH);
                assert!(
                    facts.calls.iter().all(|call| call.callee_root.is_none()),
                    "go records no callee roots"
                );
                assert!(
                    facts.calls.iter().any(|call| call.callee == "a.B"),
                    "the innermost call keeps its short callee"
                );
            });
        });
    });
}

/// Assertion tails match on the last two segments: qualified testify names hit
/// while other single-segment receivers do not.
#[test]
fn assertion_tails_match_the_last_two_segments() {
    on_default_stack(|| {
        let source = [
            "package main",
            "",
            "func g(t any) {",
            "\tassert.True(t, true)",
            "\trequire.False(t, false)",
            "\tfoo.True(t, true)",
            "}",
            "",
        ]
        .join("\n");
        with_facts(Lang::Go, "assert.go", &source, |facts| {
            assert_eq!(facts.assertions.len(), 2, "assert.True and require.False");
        });
    });
}
