//! Rust error-handler contracts: empty unit bodies, qualified `Err`, returned calls.
//!
//! A `()` arm body is an empty handler, a `Result::Err` pattern binds like `Err`,
//! and a returned plain call stays visible while nullable constructors stay `Other`.

use std::sync::LazyLock;

use sloplint_core::facts::{CallKind, Stmt};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static EMPTY_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn rust_analyze<'s>(source: &'s str) -> Analysis<'s> {
    let mut parsers = Parsers::new();
    let input = FileInput {
        lang: Lang::Rust,
        path: "case.rs",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut parsers, &input, &EMPTY_CUSTOM)
}

#[test]
fn unit_arm_body_is_empty() {
    let source = "fn save(p: &str) -> std::io::Result<()> {\n    match std::fs::remove_file(p) {\n        Ok(()) => {}\n        Err(_) => (),\n    }\n}\n";
    let analysis = rust_analyze(source);
    assert!(
        analysis.parse_error.is_none(),
        "unit arm source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "only the Err arm is a handler"
    );
    assert!(
        analysis.facts.handlers[0].stmts.is_empty(),
        "a unit body stays empty, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let source = "fn save(p: &str) -> std::io::Result<()> {\n    match std::fs::remove_file(p) {\n        Ok(()) => {}\n        Err(_) => {}\n    }\n}\n";
    let analysis = rust_analyze(source);
    assert!(
        analysis.parse_error.is_none(),
        "empty block source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    assert!(
        analysis.facts.handlers[0].stmts.is_empty(),
        "an empty block body stays empty, got {:?}",
        analysis.facts.handlers[0].stmts
    );
}

#[test]
fn qualified_err_path_binds() {
    let source = "fn load(r: Result<String, String>) -> Result<String, String> {\n    match r {\n        Result::Err(_) => {}\n        Result::Ok(_) => {}\n    }\n}\n";
    let analysis = rust_analyze(source);
    assert!(
        analysis.parse_error.is_none(),
        "qualified source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "only the qualified Err arm is a handler"
    );
    assert!(
        analysis.facts.handlers[0].stmts.is_empty(),
        "the qualified Err body stays empty, got {:?}",
        analysis.facts.handlers[0].stmts
    );
}

#[test]
fn returned_plain_call_is_visible_but_constructors_stay_other() {
    let source = "fn load(x: Result<String, String>) -> Result<String, String> {\n    match x {\n        Ok(s) => Ok(s),\n        Err(e) => return log_and_exit(e),\n    }\n}\n";
    let analysis = rust_analyze(source);
    assert!(
        analysis.parse_error.is_none(),
        "return call source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.handlers.len(),
        1,
        "only the Err arm is a handler"
    );
    assert!(
        matches!(
            &analysis.facts.handlers[0].stmts[..],
            [Stmt::Call { callee, kind }]
            if *callee == "log_and_exit" && *kind == CallKind::Plain
        ),
        "a returned plain call stays visible, got {:?}",
        analysis.facts.handlers[0].stmts
    );
    let source = "fn load(x: Result<String, String>) -> Result<String, String> {\n    match x {\n        Ok(s) => Ok(s),\n        Err(e) => return Ok(String::new()),\n    }\n}\n";
    let analysis = rust_analyze(source);
    assert!(
        analysis.parse_error.is_none(),
        "constructor source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    assert!(
        matches!(&analysis.facts.handlers[0].stmts[..], [Stmt::Other]),
        "a returned constructor stays Other, got {:?}",
        analysis.facts.handlers[0].stmts
    );
}
