//! First error position on multiline UTF-8 input, observed through `analyze` only.
//!
//! A file whose broken region rescues out cleanly yields a partial analysis:
//! the first error position is still reported and the surviving declarations
//! contribute their facts. A file that cannot be rescued yields empty facts.
//!
//! A tree can also fail invisibly: a grammar that recovers by inserting hidden
//! MISSING nodes sets `has_error` without exposing an error or missing node to
//! the cursor. The analysis still reports a position so the file is not clean.

use sloplint_core::lang::Lang;

use crate::support::run;

const SOURCE: &str = "fn ok() {}\n// café\n@\n";

#[test]
fn a_valid_declaration_before_the_error_survives_the_rescue() {
    // The `@` is excised and the file reparsed: the error position is still
    // reported, the analysis is marked `partial`, and the surviving `fn ok`
    // now contributes its facts — that is the rescue working as designed.
    let analysis = run(Lang::Rust, "broken.rs", SOURCE);
    let pos = analysis.parse_error.expect("broken.rs must fail to parse");
    assert_eq!(pos.line, 3, "the `@` line is the third line");
    let want = u32::try_from(SOURCE.find('@').expect("source holds `@`")).expect("offset fits");
    assert_eq!(pos.byte, want, "byte offset points at `@`");
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
    assert!(analysis.partial, "a rescued parse marks itself partial");
    assert_eq!(
        analysis.facts.functions.len(),
        1,
        "the surviving `fn ok` contributes its fact after the error is excised: {:?}",
        analysis.facts.functions
    );
    assert_eq!(analysis.facts.functions[0].name.as_ref(), "ok");
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
    assert_eq!(
        usize::try_from(pos.byte).expect("byte offset fits"),
        GO_HIDDEN_RECOVERY.len(),
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
