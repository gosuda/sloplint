//! A `&mut` borrow is not a write, observed through `analyze` only.
//!
//! Only assignments and compound assignments record post-declaration writes on a
//! binding. Taking a mutable borrow of the same name leaves its write lines alone.

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn writes(source: &str) -> Vec<(String, u32, u32)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "borrow source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .bindings
        .iter()
        .map(|b| (b.name.to_string(), b.first_write_line, b.write_lines))
        .collect()
}

#[test]
fn mutable_borrow_records_no_write() {
    let source = "fn f() {\nlet mut x = 0;\nlet y = &mut x;\nx = 1;\n}\n";
    assert_eq!(
        writes(source),
        vec![("x".to_string(), 4, 1)],
        "the borrow on line 3 is not a write; only the assignment counts"
    );
}

#[test]
fn borrow_only_leaves_zero_write_lines() {
    let source = "fn f() {\nlet mut x = 0;\nlet y = &mut x;\n}\n";
    assert_eq!(
        writes(source),
        vec![("x".to_string(), 2, 0)],
        "with no assignment the borrow records nothing"
    );
}
