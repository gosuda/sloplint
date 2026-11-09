//! `agent/typed-error-boundary` reads Swift's `throws` clause from its own fact and the
//! return type of Rust and Zig functions, and quotes the return type only when it applies.

use sloplint_core::lang::Lang;
use sloplint_core::suppress::RulePattern;
use sloplint_core::{Config, FileFacts, Finding, RuleFilter, RuleSet};

use crate::support::{facts_for, function, pos_of};

const SWIFT: &str = "\
public func load() throws {}
public func fetch() throws -> Int { 1 }
public func typed() throws(LoadError) -> Int { 1 }
";

const RUST: &str = "pub fn run() {}\n";

fn boundary_findings(facts: &FileFacts<'_>) -> Vec<Finding> {
    let exact = RulePattern::Exact("agent/typed-error-boundary".into());
    let filter = RuleFilter(vec![exact]);
    let rules = RuleSet::new(&Config::default(), &filter).expect("the catalog rule resolves");
    rules.check_file(facts)
}

#[test]
fn swift_flags_plain_throws_whatever_the_return_type() {
    let mut facts = facts_for(Lang::Swift, "Sources/Load.swift", SWIFT);
    let mut load = function("load", pos_of(SWIFT, "load"));
    load.untyped_throws = true;
    let mut fetch = function("fetch", pos_of(SWIFT, "fetch"));
    fetch.untyped_throws = true;
    fetch.return_type = Some("Int");
    let mut typed = function("typed", pos_of(SWIFT, "typed"));
    typed.return_type = Some("Int");
    facts.functions = vec![load, fetch, typed];
    let found = boundary_findings(&facts);
    let mut lines = Vec::new();
    for finding in &found {
        lines.push((finding.line, finding.message.as_str()));
    }
    assert_eq!(
        lines,
        [
            (1, "public `load` throws an untyped error"),
            (2, "public `fetch` throws an untyped error"),
        ],
        "only the plain throws clauses are flagged, and no return type is quoted"
    );
    assert_eq!(
        found[0].fix, "throw a typed error enum and declare it in the throws clause",
        "the Swift fix names the typed throws clause"
    );
}

#[test]
fn rust_still_flags_an_untyped_return_and_quotes_it() {
    let mut facts = facts_for(Lang::Rust, "src/api.rs", RUST);
    let mut run = function("run", pos_of(RUST, "run"));
    run.return_type = Some("Result<(), Box<dyn std::error::Error>>");
    facts.functions = vec![run];
    let found = boundary_findings(&facts);
    let messages: Vec<&str> = found.iter().map(|f| f.message.as_str()).collect();
    assert_eq!(
        messages,
        ["public `run` returns an untyped error (`Result<(), Box<dyn std::error::Error>>`)"],
        "a Rust function is judged by its return type alone"
    );
}
