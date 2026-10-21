//! Header comments attach to the declaration they precede: adjacent comments and
//! attributes chain, a wrapper statement around the declaration is the anchor,
//! and every sibling declaration owns only the comments directly above it.

use sloplint_core::lang::Lang;

use crate::support::run_clean;

const CHAIN: &str = "\
/// First line.
/// Second line.
#[inline]
// Plain note.
pub fn documented() {}

pub fn bare() {}
";

const NESTED: &str = "\
/// Outer doc.
pub fn outer() {
    // Inner note.
    fn inner() {}
}

impl S {
    /// Method doc.
    pub fn method(&self) {}

    pub fn plain(&self) {}
}
";

const TYPESCRIPT_EXPORTS: &str = "\
// Counts items.
export function count(): number {
  return 1;
}
export function other(): number {
  return 2;
}
";

const PYTHON_DECORATED: &str = "\
# About f.
@decorator
def f():
    return 1


def g():
    return 2
";

/// One function's name, trimmed doc lines, and doc start line.
struct Header {
    name: String,
    doc: Vec<String>,
    doc_line: Option<u32>,
}

/// The header of every function in `source`, in source order.
fn headers(lang: Lang, path: &str, source: &str) -> Vec<Header> {
    let analysis = run_clean(lang, path, source);
    let mut out = Vec::new();
    for function in &analysis.facts.functions {
        let mut doc = Vec::new();
        for line in &function.doc {
            doc.push(line.trim().to_string());
        }
        out.push(Header {
            name: function.name.to_string(),
            doc,
            doc_line: function.doc_pos.map(|pos| pos.line),
        });
    }
    out
}

#[test]
fn adjacent_comments_and_attributes_form_one_header() {
    let found = headers(Lang::Rust, "case.rs", CHAIN);
    assert_eq!(found.len(), 2, "two functions are declared");
    assert_eq!(found[0].name, "documented", "functions keep source order");
    assert_eq!(
        found[0].doc,
        ["/// First line.", "/// Second line.", "// Plain note."],
        "comments chain through the attribute between them and the function"
    );
    assert_eq!(
        found[0].doc_line,
        Some(1),
        "the doc starts at its first line"
    );
    assert!(
        found[1].doc.is_empty(),
        "a blank line and a function end the chain: {:?}",
        found[1].doc
    );
    assert_eq!(found[1].doc_line, None, "no comment means no doc position");
}

#[test]
fn nested_and_member_declarations_keep_their_own_comments() {
    let found = headers(Lang::Rust, "case.rs", NESTED);
    let names: Vec<&str> = found.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(
        names,
        ["outer", "inner", "method", "plain"],
        "functions keep source order"
    );
    assert_eq!(
        found[0].doc,
        ["/// Outer doc."],
        "a top-level function owns the comment above it"
    );
    assert_eq!(
        found[1].doc,
        ["// Inner note."],
        "a function nested in a body owns the comment above it"
    );
    assert_eq!(
        found[2].doc,
        ["/// Method doc."],
        "a method owns the comment above it"
    );
    assert!(
        found[3].doc.is_empty(),
        "the previous sibling is a function, which ends the comment chain: {:?}",
        found[3].doc
    );
}

#[test]
fn an_export_statement_is_the_header_anchor() {
    let found = headers(Lang::TypeScript, "case.ts", TYPESCRIPT_EXPORTS);
    assert_eq!(found.len(), 2, "two exported functions are declared");
    assert_eq!(
        found[0].doc,
        ["// Counts items."],
        "the comment above the export statement documents the function inside it"
    );
    assert!(
        found[1].doc.is_empty(),
        "the previous export statement is not a comment: {:?}",
        found[1].doc
    );
}

#[test]
fn a_decorated_definition_is_the_header_anchor() {
    let found = headers(Lang::Python, "case.py", PYTHON_DECORATED);
    assert_eq!(found.len(), 2, "two functions are declared");
    assert_eq!(
        found[0].doc,
        ["# About f."],
        "the comment above the decorator documents the function"
    );
    assert!(
        found[1].doc.is_empty(),
        "the previous definition is not a comment: {:?}",
        found[1].doc
    );
}

#[test]
fn every_sibling_in_a_long_run_keeps_only_its_own_comment() {
    let mut source = String::new();
    for index in 0..40 {
        source.push_str(&format!("// doc {index}\npub fn f{index}() {{}}\n"));
    }
    let found = headers(Lang::Rust, "case.rs", &source);
    assert_eq!(found.len(), 40, "forty functions are declared");
    for (index, header) in found.iter().enumerate() {
        assert_eq!(
            header.doc,
            [format!("// doc {index}")],
            "function {} must see only the comment above it",
            header.name
        );
    }
}
