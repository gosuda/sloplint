//! Rust global writes join by name, observed through `analyze` only.
//!
//! A `static mut` item is a global mutable, and an in-function assignment to that same
//! name is the write half of the pair. Reads, locals, and immutable statics stay quiet.

use sloplint_core::facts::AmbientKind;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn ambient(source: &str) -> Vec<(AmbientKind, String)> {
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
        "ambient source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .ambient
        .iter()
        .map(|a| (a.kind, a.name.to_string()))
        .collect()
}

#[test]
fn static_mut_written_in_a_function_joins_by_name() {
    let source = "static mut COUNTER: u32 = 0;\nfn bump() {\nCOUNTER += 1;\n}\n";
    assert_eq!(
        ambient(source),
        vec![
            (AmbientKind::GlobalMutable, "COUNTER".to_string()),
            (AmbientKind::GlobalWrite, "COUNTER".to_string()),
        ]
    );
}

#[test]
fn reads_locals_and_immutable_statics_stay_quiet() {
    let source = "static mut COUNTER: u32 = 0;\nstatic LIMIT: u32 = 10;\nfn get() -> u32 {\nCOUNTER\n}\nfn local() {\nlet mut x = 0;\nx += 1;\n}\n";
    assert_eq!(
        ambient(source),
        vec![(AmbientKind::GlobalMutable, "COUNTER".to_string())]
    );
}

#[test]
fn two_functions_writing_the_same_static_both_join() {
    let source = "static mut COUNTER: u32 = 0;\nfn bump() {\nCOUNTER += 1;\n}\nfn reset() {\nCOUNTER = 0;\n}\n";
    assert_eq!(
        ambient(source),
        vec![
            (AmbientKind::GlobalMutable, "COUNTER".to_string()),
            (AmbientKind::GlobalWrite, "COUNTER".to_string()),
            (AmbientKind::GlobalWrite, "COUNTER".to_string()),
        ]
    );
}
