//! A `sloplint-allow` directive comment is never part of a function's doc. It
//! extends the header, so a doc comment on its other side stays attached, and the
//! doc's position is the doc comment's own, which is the line a directive covers.

use sloplint_core::lang::Lang;

use crate::support::{run, run_clean};

const DIRECTIVE_ABOVE_DOC: &str = "\
// sloplint-allow(slop/echo-comment): the name is the contract
/// Process the order.
pub fn process_order() {}
";

const DIRECTIVE_BELOW_DOC: &str = "\
/// Process the order.
// sloplint-allow(slop/echo-comment): the name is the contract
pub fn process_order() {}
";

const DIRECTIVE_AFTER_GAP: &str = "\
/// Process the order.

// sloplint-allow(slop/echo-comment): the name is the contract
pub fn process_order() {}
";

const DOC_MENTIONING_DIRECTIVES: &str = "\
/// Explains how `sloplint-allow` comments are written.
pub fn process_order() {}
";

const PYTHON_DIRECTIVE_ABOVE_DOC: &str = "\
# sloplint-allow(slop/echo-comment): the name is the contract
# Process the order.
def process_order():
    return 1
";

/// The trimmed doc lines and the doc start line of the first function in `source`.
fn header(lang: Lang, path: &str, source: &str) -> (Vec<String>, Option<u32>) {
    let analysis = run_clean(lang, path, source);
    let Some(function) = analysis.facts.functions.first() else {
        panic!("{path} declares a function");
    };
    let mut doc = Vec::new();
    for line in &function.doc {
        doc.push(line.trim().to_string());
    }
    (doc, function.doc_pos.map(|pos| pos.line))
}

#[test]
fn a_directive_above_a_doc_comment_is_not_documentation() {
    let (doc, doc_pos) = header(Lang::Rust, "case.rs", DIRECTIVE_ABOVE_DOC);
    assert_eq!(doc, ["/// Process the order."], "the directive is not doc");
    assert_eq!(doc_pos, Some(2), "the doc starts at its own comment line");
    let analysis = run(Lang::Rust, "case.rs", DIRECTIVE_ABOVE_DOC);
    let lines: Vec<u32> = analysis.comments.iter().map(|c| c.start_line).collect();
    assert_eq!(
        lines,
        [1],
        "the directive is still collected for suppression"
    );
}

#[test]
fn a_directive_below_a_doc_comment_keeps_the_doc_attached() {
    let (doc, doc_pos) = header(Lang::Rust, "case.rs", DIRECTIVE_BELOW_DOC);
    assert_eq!(
        doc,
        ["/// Process the order."],
        "the doc stays attached through the directive that follows it"
    );
    assert_eq!(doc_pos, Some(1), "the doc starts at its own comment line");
}

#[test]
fn a_directive_does_not_bridge_a_blank_line_to_the_doc() {
    let (doc, doc_pos) = header(Lang::Rust, "case.rs", DIRECTIVE_AFTER_GAP);
    assert!(
        doc.is_empty(),
        "a blank line detaches the doc and the directive is not doc: {doc:?}"
    );
    assert_eq!(doc_pos, None, "no doc comment means no doc position");
}

#[test]
fn a_doc_comment_that_mentions_directives_stays_documentation() {
    let (doc, doc_pos) = header(Lang::Rust, "case.rs", DOC_MENTIONING_DIRECTIVES);
    assert_eq!(
        doc,
        ["/// Explains how `sloplint-allow` comments are written."],
        "a doc comment only describes directives, so it remains doc"
    );
    assert_eq!(doc_pos, Some(1), "the doc starts at its own comment line");
    let analysis = run(Lang::Rust, "case.rs", DOC_MENTIONING_DIRECTIVES);
    assert!(
        analysis.comments.is_empty(),
        "a doc comment issues no directive: {:?}",
        analysis.comments
    );
}

#[test]
fn directive_exclusion_holds_in_hash_comment_languages() {
    let (doc, doc_pos) = header(Lang::Python, "case.py", PYTHON_DIRECTIVE_ABOVE_DOC);
    assert_eq!(doc, ["# Process the order."], "the directive is not doc");
    assert_eq!(doc_pos, Some(2), "the doc starts at its own comment line");
}
