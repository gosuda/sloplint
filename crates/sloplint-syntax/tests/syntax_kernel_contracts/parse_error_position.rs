//! First error position on multiline UTF-8 input, observed through `analyze` only.
//!
//! A file that fails to parse yields empty facts and the byte offset of the
//! first error node, so a valid declaration before the error contributes
//! nothing to later stages.

use sloplint_core::lang::Lang;

use crate::support::run;

const SOURCE: &str = "fn ok() {}\n// café\n@\n";

#[test]
fn a_valid_declaration_before_the_error_contributes_no_facts() {
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
