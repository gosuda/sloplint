//! Rust use trees, `self` chains, and left-nested or-patterns at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, on_default_stack, with_facts, within_linear_budget};

/// Every leaf of a use tree nested at every level becomes its own import, in source order.
#[test]
fn use_trees_report_every_leaf() {
    on_default_stack(|| {
        let source = format!(
            "use a::{}c, d{};\n",
            "{b::".repeat(DEPTH),
            "}".repeat(DEPTH)
        );
        with_facts(Lang::Rust, "case.rs", &source, |facts| {
            let names: Vec<&str> = facts.imports.iter().map(|i| i.name).collect();
            assert_eq!(names, ["c", "d"]);
        });
    });
}

/// A `pub use self::` chain nested at every level resolves to the module named at its innermost
/// identifier, in time linear in the depth.
#[test]
fn self_use_chains_find_their_module() {
    on_default_stack(|| {
        within_linear_budget(|depth| {
            let source = format!(
                "mod a;\npub use self::{}a{};\n",
                "{self::".repeat(depth),
                "}".repeat(depth)
            );
            with_facts(Lang::Rust, "case.rs", &source, |facts| {
                assert!(
                    facts.barrel.is_none(),
                    "a mod-tree re-export file is idiomatic, not a barrel"
                );
                let names: Vec<&str> = facts.imports.iter().map(|i| i.name).collect();
                assert_eq!(
                    names,
                    ["a"],
                    "the self chain resolves to its innermost module"
                );
            });
        });
    });
}

/// Handler and wildcard-arm counts of a match whose first arm is `predecessor` and whose second
/// arm is `Err(e) => return Err(e)`.
fn match_facts(predecessor: &str) -> (usize, usize) {
    let source = format!(
        "fn g(x: Result<i32, i32>) -> Result<i32, i32> {{\n    match x {{\n        {predecessor} => 1,\n        Err(e) => return Err(e),\n    }}\n}}\n"
    );
    with_facts(Lang::Rust, "case.rs", &source, |facts| {
        (facts.handlers.len(), facts.wildcard_arms.len())
    })
}

/// A predecessor with thousands of alternatives yields the same facts as a two-alternative
/// predecessor of the same shape, so the or-pattern walk decides exactly what a shallow walk does.
#[test]
fn or_pattern_predecessors_match_their_shallow_form() {
    on_default_stack(|| {
        let oks = vec!["Ok(1)"; DEPTH].join(" | ");
        let oks_then_err = format!("{oks} | Err(_)");
        assert_eq!(match_facts(&oks), match_facts("Ok(1) | Ok(1)"));
        assert_eq!(match_facts(&oks_then_err), match_facts("Ok(1) | Err(_)"));
    });
}

/// A method-call chain keeps every call while staying linear in depth.
///
/// Each `env::` check used to scan the whole callee text per call for
/// whitespace; the check now reads only the longest-target tail, so the chain
/// below costs O(depth) instead of O(depth^2).
#[test]
fn call_chains_keep_every_call_in_linear_time() {
    on_default_stack(|| {
        within_linear_budget(|depth| {
            let source = "fn g() {\n    a".to_owned() + &".b()".repeat(depth) + ";\n}\n";
            with_facts(Lang::Rust, "chain.rs", &source, |facts| {
                assert_eq!(facts.calls.len(), depth);
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

/// Env callees match with or without whitespace around the separators,
/// including through a receiver that itself ends with a target name.
#[test]
fn env_callees_match_with_or_without_whitespace() {
    on_default_stack(|| {
        let source = [
            "fn g() {",
            "    std::env::var(\"A\");",
            "    std::env :: var(\"B\");",
            "    myenv::var(\"C\");",
            "    env::vars_os();",
            "    other::get(\"D\");",
            "}",
            "",
        ]
        .join("\n");
        let names = with_facts(Lang::Rust, "env.rs", &source, |facts| {
            facts
                .ambient
                .iter()
                .map(|site| site.name.to_owned())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            names,
            [
                "std::env::var",
                "std::env :: var",
                "myenv::var",
                "env::vars_os",
            ]
        );
    });
}
