//! Rust closures are their own functions, observed through `analyze` only.
//!
//! A closure restarts the walk as a function fact with its own parameters, so the
//! cognitive and depth counters of the enclosing function never see through it.

use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn functions(source: &str) -> Vec<(String, Vec<String>)> {
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
        "closure source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .functions
        .iter()
        .map(|f| {
            (
                f.name.to_string(),
                f.params.iter().map(|p| p.name.to_string()).collect(),
            )
        })
        .collect()
}

#[test]
fn closures_read_as_functions_with_their_own_params() {
    let source = "fn f() {\nlet g = |x: i32, y| x;\n}\n";
    assert_eq!(
        functions(source),
        vec![
            ("f".to_string(), vec![]),
            ("".to_string(), vec!["x".to_string(), "y".to_string()]),
        ]
    );
}

#[test]
fn nested_closures_each_read_as_a_function() {
    let source = "fn f() {\nlet g = |x| {\n|x| x;\n};\n}\n";
    let found = functions(source);
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].0, "f");
    assert_eq!(found[1].1, vec!["x".to_string()]);
    assert_eq!(found[2].1, vec!["x".to_string()]);
}
