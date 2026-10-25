//! Rust use trees, `self` barrels, and left-nested or-patterns at 20,000 levels.

use sloplint_core::lang::Lang;

use super::support::{DEPTH, on_default_stack, with_facts};

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
/// identifier.
#[test]
fn self_use_chains_find_their_module() {
    on_default_stack(|| {
        let source = format!(
            "mod a;\npub use self::{}a{};\n",
            "{self::".repeat(DEPTH),
            "}".repeat(DEPTH)
        );
        with_facts(Lang::Rust, "case.rs", &source, |facts| {
            let barrel = facts
                .barrel
                .as_ref()
                .expect("a file of mods and pub uses is a barrel");
            assert_eq!(barrel.targets.len(), 1, "one use declaration");
            assert_eq!(barrel.targets[0].len(), 2, "a.rs and a/mod.rs");
            assert!(
                barrel.targets[0][0].ends_with("a.rs"),
                "{:?}",
                barrel.targets
            );
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
