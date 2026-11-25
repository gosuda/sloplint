//! The full tactic-heavy Lean fixture that formerly replayed zero-width
//! layout tokens must complete and retain declaration facts. Reusing the
//! same parser for a normal file must also remain safe.
//!
//! The Mathlib fixture keeps its license and attribution in testdata.
//! The wall cap is a backstop; assertions concern parser outcomes, not time.

use std::sync::LazyLock;
use std::sync::mpsc::channel;
use std::time::Duration;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

const CAP: Duration = Duration::from_secs(120);
const FORMERLY_DIVERGENT: &str = include_str!("../testdata/finprod.lean");
const HEALTHY: &str = "def answer : Nat := 42\n";

struct Summary {
    formerly_divergent_timed_out: bool,
    retained_functions: usize,
    retained_imports: usize,
    healthy_clean: bool,
    healthy_fns: usize,
    healthy_first: String,
}

#[test]
fn formerly_divergent_lean_retains_facts_and_parser_reuse() {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let mut parsers = Parsers::new();
        let formerly_divergent = FileInput {
            lang: Lang::Lean,
            path: "formerly_divergent.lean",
            source: FORMERLY_DIVERGENT,
            role: CodeRole::Library,
        };
        let parsed = analyze(&mut parsers, &formerly_divergent, &NO_CUSTOM);
        let healthy = FileInput {
            lang: Lang::Lean,
            path: "healthy.lean",
            source: HEALTHY,
            role: CodeRole::Library,
        };
        let ok = analyze(&mut parsers, &healthy, &NO_CUSTOM);
        let summary = Summary {
            formerly_divergent_timed_out: parsed.timed_out,
            retained_functions: parsed.facts.functions.len(),
            retained_imports: parsed.facts.imports.len(),
            healthy_clean: ok.parse_error.is_none(),
            healthy_fns: ok.facts.functions.len(),
            healthy_first: ok
                .facts
                .functions
                .first()
                .map(|f| f.name.to_string())
                .unwrap_or_default(),
        };
        let _ = sender.send(summary);
    });
    let summary = receiver
        .recv_timeout(CAP)
        .expect("Lean analysis must advance instead of replaying layout tokens");
    assert!(
        !summary.formerly_divergent_timed_out,
        "the repaired scanner must complete without exhausting the parse budget"
    );
    assert!(
        summary.retained_functions > 0,
        "the full fixture retains declaration facts"
    );
    assert!(
        summary.retained_imports > 0,
        "the full fixture retains import facts"
    );
    assert!(
        summary.healthy_clean,
        "the next file on the shared parser parses cleanly"
    );
    assert_eq!(
        summary.healthy_fns, 1,
        "healthy def contributes one function"
    );
    assert_eq!(summary.healthy_first, "answer");
}

#[test]
fn late_fixture_declarations_survive_recovery() {
    let mut parsers = Parsers::new();
    let input = FileInput {
        lang: Lang::Lean,
        path: "finprod.lean",
        source: FORMERLY_DIVERGENT,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut parsers, &input, &NO_CUSTOM);
    assert!(
        !analysis.timed_out,
        "the full fixture completes instead of exhausting the parse budget"
    );
    let names: Vec<_> = analysis.facts.functions.iter().map(|f| f.name).collect();
    for late in [
        "mulSupport_fun_pow_count_subset",
        "hasFiniteMulSupport_fun_pow_count",
    ] {
        assert!(
            names.contains(&late),
            "late fixture declaration {late} survives recovery, retained {} functions",
            names.len()
        );
    }
    let mut late_lines: Vec<u32> = analysis
        .facts
        .functions
        .iter()
        .filter(|f| {
            f.name == "mulSupport_fun_pow_count_subset"
                || f.name == "hasFiniteMulSupport_fun_pow_count"
        })
        .map(|f| f.pos.line)
        .collect();
    late_lines.sort_unstable();
    assert_eq!(
        late_lines,
        [1397, 1402],
        "the retained late declarations keep their tail positions"
    );
}
