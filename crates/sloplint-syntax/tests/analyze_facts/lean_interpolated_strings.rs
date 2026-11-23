//! Lean interpolated-string argument shapes, observed through `analyze` only.
//!
//! The grammar surfaces no literal-chunk children inside `interpolated_str`
//! (only the opener, escape sequences, interpolation holes, and the closer),
//! so the adapter measures literal gaps in the node's own text. A
//! string whose longest literal run clears the 8-char floor must report it,
//! and calls nested in `{...}` holes must still appear as call facts.

use sloplint_core::facts::ArgShape;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

const PARSED: &str = "def main : IO Unit := do\n  IO.println s!\"parsed {r.size} rows of data\"\n";

const INVALID: &str = "def main : IO Unit := do\n  IO.println s!\"invalid csv data: {e}\"\n";

const NESTED_CALL: &str = "def main : IO Unit := do\n  IO.println s!\"{f x} tail literal here\"\n";
const PRIMED_SUFFIX: &str = "def main : IO Unit := do\n  IO.println s!\"{x'} rows of data\"\n";

const PERCENT_LITERAL: &str = "def main : IO Unit := do\n  IO.println s!\"disk 95% done: {n}\"\n";

const HOLE_ONLY: &str = "def aux : IO Unit := do\n  logInfo m!\"{value}\"\n";

/// The longest static run of the single argument of the `name` call.
fn arg_run(source: &str, name: &str) -> u32 {
    let analysis = run_clean(Lang::Lean, "main.lean", source);
    let call = analysis
        .facts
        .calls
        .iter()
        .find(|call| call.callee == name)
        .unwrap_or_else(|| panic!("{name} call is missing"));
    assert!(
        call.args.len() == 1,
        "{name} takes one argument, found {}",
        call.args.len()
    );
    match &call.args[0].shape {
        ArgShape::Str { longest_static } => *longest_static,
        shape => panic!("{name} argument is a string, found {shape:?}"),
    }
}

#[test]
fn interpolated_runs_count_text_between_holes() {
    assert_eq!(
        arg_run(PARSED, "IO.println"),
        13,
        "` rows of data` is 13 chars"
    );
    assert_eq!(
        arg_run(INVALID, "IO.println"),
        18,
        "`invalid csv data: ` is 18 chars"
    );
}

#[test]
fn hole_only_message_measures_zero() {
    assert_eq!(arg_run(HOLE_ONLY, "logInfo"), 0, "no literal run survives");
}

#[test]
fn calls_inside_holes_stay_visible() {
    let analysis = run_clean(Lang::Lean, "main.lean", NESTED_CALL);
    assert!(
        analysis.facts.calls.iter().any(|call| call.callee == "f"),
        "the hole still records its call"
    );
    assert_eq!(
        arg_run(NESTED_CALL, "IO.println"),
        18,
        "` tail literal here` is 18 chars"
    );
}

#[test]
fn primed_hole_keeps_trailing_literal() {
    assert_eq!(
        arg_run(PRIMED_SUFFIX, "IO.println"),
        13,
        "` rows of data` is 13 chars"
    );
}

#[test]
fn percent_before_word_stays_literal() {
    assert_eq!(
        arg_run(PERCENT_LITERAL, "IO.println"),
        15,
        "`disk 95% done: ` is 15 chars"
    );
}

const ESCAPED_UNICODE: &str = "def main : IO Unit := do\n  IO.println s!\"\\u{1F600}!\"\n";

#[test]
fn unicode_escape_spelling_does_not_inflate_the_run() {
    assert_eq!(
        arg_run(ESCAPED_UNICODE, "IO.println"),
        1,
        "`\\u{{1F600}}` is one output char, so only `!` survives the break"
    );
}

#[test]
fn fixed_width_escapes_break_interpolated_runs() {
    for prefix in ["s!", "m!"] {
        for (literal, expected) in [
            (r"\x41!", 1),
            (r"\u0041!", 1),
            (r"head\x41tail", 4),
            (r"head\u0041tail", 4),
            (r"\x41\u0041", 0),
            (r"λ\x41π", 1),
        ] {
            let source = format!("def main : IO Unit := do\n  IO.println {prefix}\"{literal}\"\n");
            assert_eq!(
                arg_run(&source, "IO.println"),
                expected,
                "{prefix}{literal}"
            );
        }
    }
}
