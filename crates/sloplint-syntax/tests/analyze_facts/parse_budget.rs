//! A divergent Lean parse is abandoned with one parse-error finding, and the
//! reused parser starts the next file clean.
//!
//! The vendored Lean grammar (18,895 states) diverges on some tactic-heavy
//! files. `Mathlib/Algebra/BigOperators/Finprod.lean` (67 KB, mathlib4 at
//! `90e6eec`, Apache-2.0, Kexing Ying / Kevin Buzzard / Yury Kudryashov —
//! its header below keeps the attribution) never finishes: the unmodified
//! binary burns 202 s of parser CPU and 30 GB of RSS past a 300 s wall cap
//! without producing a tree, and the reference tree-sitter CLI needs 11 GB
//! RSS / 30 GB of address space in 4 minutes with no output in 5. Without
//! a parse budget the analysis never returns for such a file, so a whole
//! root run holds gigabytes per worker until the OOM-killer (or the
//! address-space cap) aborts it with no JSON at all. With the budget, the
//! file keeps exactly one `meta/parse-error` timeout finding and the
//! worker's parser is reset, so the next file parses from a clean state
//! instead of resuming the abandoned multi-GB stack — which would corrupt
//! its tree without any error signal.
//!
//! The fixture is the complete file, not a truncation: small valid Lean
//! inputs (including every snippet of the baseline's shape ranking) parse
//! in milliseconds, so only a full-size divergent input proves the budget
//! fires on the real phenomenon. It runs past the production budget on any
//! plausible host (202 s of parser CPU here — a 15x faster machine still
//! exceeds it), so this test never depends on timing: it only asserts the
//! analysis *returns* — past the fix in about one budget — as a timeout,
//! not a syntax error, with no facts, and that a normal file analyzed next
//! on the same `Parsers` is unaffected. Before the fix the first analysis
//! never returns and the test fails at the cap below.
//!
//! Threading: `Parsers` (and tree-sitter `Parser`) are `Send`, but `spawn`
//! needs `'static`, so the `Parsers` is built *inside* the thread, runs
//! divergent-then-healthy on that one instance, and only plain `Send`
//! summaries cross the channel. `recv_timeout(CAP)` is a backstop, not a
//! measurement — the test asserts returned shapes, never durations.

use std::sync::LazyLock;
use std::sync::mpsc::channel;
use std::time::Duration;

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

/// Backstop only: past the fix both analyzes return in about one parse
/// budget; the cap exists so a regression fails instead of hanging CI.
const CAP: Duration = Duration::from_secs(120);

const DIVERGENT: &str = include_str!("../testdata/finprod.lean");

const HEALTHY: &str = "def answer : Nat := 42\n";

struct Summary {
    divergent_had_error: bool,
    divergent_facts_empty: bool,
    divergent_partial: bool,
    divergent_timed_out: bool,
    healthy_clean: bool,
    healthy_fns: usize,
    healthy_first: String,
}

#[test]
fn divergent_lean_parse_is_abandoned_with_a_parse_error() {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let mut parsers = Parsers::new();
        let divergent = FileInput {
            lang: Lang::Lean,
            path: "divergent.lean",
            source: DIVERGENT,
            role: CodeRole::Library,
        };
        let div = analyze(&mut parsers, &divergent, &NO_CUSTOM);
        let healthy = FileInput {
            lang: Lang::Lean,
            path: "healthy.lean",
            source: HEALTHY,
            role: CodeRole::Library,
        };
        let ok = analyze(&mut parsers, &healthy, &NO_CUSTOM);
        let summary = Summary {
            divergent_had_error: div.parse_error.is_some(),
            divergent_facts_empty: div.facts.functions.is_empty()
                && div.facts.calls.is_empty()
                && div.facts.imports.is_empty(),
            divergent_partial: div.partial,
            divergent_timed_out: div.timed_out,
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
    let s = receiver
        .recv_timeout(CAP)
        .expect("divergent analysis must return (it hangs without the parse budget)");
    assert!(
        s.divergent_had_error,
        "divergent file reports a parse error"
    );
    assert!(
        s.divergent_facts_empty,
        "a timed-out parse yields no facts to judge"
    );
    assert!(
        !s.divergent_partial,
        "timeout is not a rescue: nothing was salvaged"
    );
    // Every assertion above also holds for an ordinary syntax failure;
    // only the timeout status tells the budget apart from one.
    assert!(
        s.divergent_timed_out,
        "the divergent file reports a timeout, not an ordinary syntax error"
    );
    // The shared parser was reset after the cancel: the next file parses
    // from a clean state. Without the reset this file resumes the abandoned
    // stack and its tree (and these assertions) corrupt silently.
    assert!(s.healthy_clean, "file after a timeout still parses clean");
    assert_eq!(s.healthy_fns, 1, "healthy `def` contributes one function");
    assert_eq!(s.healthy_first, "answer");
}
