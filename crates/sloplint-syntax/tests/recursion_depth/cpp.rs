//! C++ member-call chains and env-callee gating at depth.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, on_default_stack, with_facts, within_linear_budget};

/// A member-call chain keeps every call in linear adapter time.
///
/// The env check used to flatten the whole callee text per call; callees with
/// parens can never match the paren-free env names, so they skip the flatten
/// and the chain below costs the adapter O(depth) instead of O(depth^2).
#[test]
fn call_chains_keep_every_call_in_linear_time() {
    on_default_stack(|| {
        let source = "void g() {\n    a".to_owned() + &".b()".repeat(DEPTH) + ";\n}\n";
        within_linear_budget(|| {
            with_facts(Lang::Cpp, "chain.cpp", &source, |facts| {
                assert_eq!(facts.calls.len(), DEPTH);
                let inner = facts
                    .calls
                    .iter()
                    .find(|call| call.callee == "a.b")
                    .expect("the innermost call keeps its short callee");
                assert_eq!(inner.callee_root, Some("a"));
                assert!(
                    facts
                        .calls
                        .iter()
                        .filter(|call| call.callee != "a.b")
                        .all(|call| call.callee_root.is_none()),
                    "chained calls through other calls keep no root"
                );
                assert!(facts.ambient.is_empty(), "no env reads in a plain chain");
            });
        });
    });
}

/// Env callees match plain and whitespace-padded names, but member chains
/// holding parens never reach the matcher and report nothing.
#[test]
fn env_callees_match_plain_names_but_not_chains() {
    on_default_stack(|| {
        let source = [
            "void g() {",
            "    std::getenv(\"A\");",
            "    ::getenv(\"B\");",
            "    getenv(\"C\");",
            "    std :: getenv(\"D\");",
            "    x.getenv(\"E\");",
            "    a.b().getenv(\"F\");",
            "}",
            "",
        ]
        .join("\n");
        let names = with_facts(Lang::Cpp, "env.cpp", &source, |facts| {
            facts
                .ambient
                .iter()
                .map(|site| site.name.to_owned())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            names,
            ["std::getenv", "::getenv", "getenv", "std :: getenv",]
        );
    });
}
