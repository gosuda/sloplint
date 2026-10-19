//! First error position on multiline UTF-8 input, observed through `analyze` only.
//!
//! A file that fails to parse yields empty facts and the byte offset of the
//! first error node, so a valid declaration before the error contributes
//! nothing to later stages.
//!
//! A tree can also fail invisibly: a grammar that recovers by inserting hidden
//! MISSING nodes sets `has_error` without exposing an error or missing node to
//! the cursor. The analysis still reports a position so the file is not clean.

use sloplint_core::lang::Lang;

use crate::support::run;

const SOURCE: &str = "fn ok() {}\n// café\n@\n";

#[test]
fn a_valid_declaration_before_the_error_contributes_no_facts() {
    let analysis = run(Lang::Rust, "broken.rs", SOURCE);
    let pos = analysis.parse_error.expect("broken.rs must fail to parse");
    assert_eq!(pos.line, 3, "the `@` line is the third line");
    let want = u32::try_from(SOURCE.find('@').expect("source holds `@`")).expect("offset fits");
    assert_eq!(
        pos.byte, 20,
        "byte 20: 11 for the declaration line plus newline, 9 for the comment line plus its two byte e-acute plus newline"
    );
    let at = usize::try_from(pos.byte).expect("byte offset fits");
    assert_eq!(
        &SOURCE[at..at + 1],
        "@",
        "the byte offset slices the error character"
    );
    assert!(
        analysis.facts.functions.is_empty(),
        "the valid `fn ok` yields no fact once the parse fails: {:?}",
        analysis.facts.functions
    );
    assert!(
        analysis.facts.bindings.is_empty(),
        "bindings are empty: {:?}",
        analysis.facts.bindings
    );
    assert!(
        analysis.facts.calls.is_empty(),
        "calls are empty: {:?}",
        analysis.facts.calls
    );
    assert!(
        analysis.facts.imports.is_empty(),
        "imports are empty: {:?}",
        analysis.facts.imports
    );
    assert!(
        analysis.facts.types.is_empty(),
        "types are empty: {:?}",
        analysis.facts.types
    );
    assert!(
        analysis.facts.impls.is_empty(),
        "impls are empty: {:?}",
        analysis.facts.impls
    );
    assert!(
        analysis.custom_hits.is_empty(),
        "custom hits are empty on parse failure"
    );
}

/// A statement glued to a label makes the vendored Go grammar insert a hidden
/// MISSING terminator: the tree is flagged `has_error`, yet no error or
/// missing node is reachable through the cursor.
const GO_HIDDEN_RECOVERY: &str = "func f() { go f()L1: }\n";

#[test]
fn a_recovery_that_hides_every_error_still_reports_a_position() {
    let analysis = run(Lang::Go, "recovered.go", GO_HIDDEN_RECOVERY);
    let pos = analysis
        .parse_error
        .expect("a failed parse reports a position even when recovery hides every error node");
    let want = usize::try_from(GO_HIDDEN_RECOVERY.len()).expect("length fits");
    assert_eq!(
        usize::try_from(pos.byte).expect("byte offset fits"),
        want,
        "recovery hides every fault, so the reported position falls back to the end of the tree"
    );
    assert_eq!(
        pos.line, 2,
        "the end of the tree is the line after the source"
    );
    assert!(
        analysis.facts.functions.is_empty(),
        "a failed parse yields no facts: {:?}",
        analysis.facts.functions
    );
    assert!(
        analysis.facts.calls.is_empty(),
        "a failed parse yields no calls: {:?}",
        analysis.facts.calls
    );
}
