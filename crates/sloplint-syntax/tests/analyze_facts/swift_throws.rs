//! A Swift function's return type is the type after `->` and nothing else; the throws
//! clause is its own fact, and only plain `throws` is untyped.

use sloplint_core::lang::Lang;

use crate::support::run_clean;

const SIGNATURES: &str = "\
public func load() throws {}
public func fetch() throws -> Int { return 1 }
public func typed() throws(LoadError) -> Int { return 1 }
public func relay() rethrows {}
public func plain() -> String { return \"\" }
public func loadAsync() async throws -> Int { return 1 }
public func optional() -> Int! { return 1 }
public class Loader {
    public init() throws {}
}
";

#[test]
fn the_throws_clause_never_leaks_into_the_return_type() {
    let analysis = run_clean(Lang::Swift, "case.swift", SIGNATURES);
    let functions = &analysis.facts.functions;
    let names: Vec<&str> = functions.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        [
            "load",
            "fetch",
            "typed",
            "relay",
            "plain",
            "loadAsync",
            "optional",
            "init",
        ],
        "functions keep source order"
    );
    let returns: Vec<Option<&str>> = functions.iter().map(|f| f.return_type).collect();
    assert_eq!(
        returns,
        [
            None,
            Some("Int"),
            Some("Int"),
            None,
            Some("String"),
            Some("Int"),
            Some("Int!"),
            None,
        ],
        "the return type is only the type after the arrow, with its implicit-unwrap mark"
    );
}

#[test]
fn only_plain_throws_is_untyped() {
    let analysis = run_clean(Lang::Swift, "case.swift", SIGNATURES);
    let functions = &analysis.facts.functions;
    let flags: Vec<bool> = functions.iter().map(|f| f.untyped_throws).collect();
    assert_eq!(
        flags,
        [true, true, false, false, false, true, false, true],
        "plain throws is untyped, also with async or in an init; other forms are not"
    );
}
