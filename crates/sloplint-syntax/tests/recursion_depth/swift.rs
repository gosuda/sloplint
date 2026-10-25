//! Swift `try`/`await` unwrapping and member-chain roots at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, binding, function, on_default_stack, with_facts};

/// A `try` chain around one statement call unwraps to that call: `sole_call` links calls[0].
#[test]
fn try_chains_unwrap_to_their_call() {
    on_default_stack(|| {
        let source = format!("func g() throws {{\n    {}f()\n}}\n", "try ".repeat(DEPTH));
        with_facts(Lang::Swift, "case.swift", &source, |facts| {
            assert_eq!(
                function(facts, "g").sole_call,
                Some(0),
                "the chain unwraps to the call"
            );
            assert_eq!(facts.calls[0].callee, "f");
        });
    });
}

/// A `try` run followed by an `await` run, both at depth, unwraps to the same call.
#[test]
fn mixed_try_await_chains_unwrap_to_their_call() {
    on_default_stack(|| {
        let source = format!(
            "func g() async throws {{\n    {}{}f()\n}}\n",
            "try ".repeat(DEPTH / 2),
            "await ".repeat(DEPTH / 2)
        );
        with_facts(Lang::Swift, "case.swift", &source, |facts| {
            assert_eq!(
                function(facts, "g").sole_call,
                Some(0),
                "the chain unwraps to the call"
            );
            assert_eq!(facts.calls[0].callee, "f");
        });
    });
}

/// A `return` whose result is a `try` chain around one call unwraps through the control transfer.
#[test]
fn returned_try_chains_unwrap_to_their_call() {
    on_default_stack(|| {
        let source = format!(
            "func g() throws -> Int {{\n    return {}f()\n}}\n",
            "try ".repeat(DEPTH)
        );
        with_facts(Lang::Swift, "case.swift", &source, |facts| {
            assert_eq!(
                function(facts, "g").sole_call,
                Some(0),
                "the return unwraps to the call"
            );
            assert_eq!(facts.calls[0].callee, "f");
        });
    });
}

/// A `navigation_expression` chain roots at its leftmost identifier, for writes and calls alike.
#[test]
fn member_chains_keep_their_roots() {
    on_default_stack(|| {
        let write = format!(
            "func g() {{\n    var a = 0\n    {}a = 1\n}}\n",
            "a.".repeat(DEPTH)
        );
        with_facts(Lang::Swift, "case.swift", &write, |facts| {
            assert_eq!(
                binding(facts, "a").last_write_line,
                3,
                "the write roots at the declared a"
            );
        });
        let call = format!("func g() {{\n    {}f()\n}}\n", "a.".repeat(DEPTH));
        with_facts(Lang::Swift, "case.swift", &call, |facts| {
            assert_eq!(facts.calls[0].callee_root, Some("a"));
        });
    });
}
