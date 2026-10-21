//! Objective-C message sends: the selector's keyword identifiers name the call and are
//! not arguments, so the first argument is the first expression after the first keyword's colon.

use crate::support::with_facts;
use sloplint_core::facts::ArgShape;
use sloplint_core::lang::Lang;

const SOURCE: &str = "void f(id logger, id store, int count) {
    [logger logMessage:@\"cache warmed before first request\"];
    [store setValue:count forKey:@\"cache.size\"];
    [logger flush];
    [logger relay:[store lookup:@\"inner key literal\"]];
}
";

fn describe(shape: &ArgShape<'_>) -> String {
    match shape {
        ArgShape::Str { longest_static } => format!("str:{longest_static}"),
        ArgShape::Concat => "concat".to_string(),
        ArgShape::Ident(name) => format!("ident:{name}"),
        ArgShape::Other => "other".to_string(),
    }
}

/// The argument shapes of the first send whose first selector keyword is `callee`.
fn shapes(callee: &str) -> Vec<String> {
    with_facts(Lang::ObjC, "case.m", SOURCE, |facts| {
        let call = facts
            .calls
            .iter()
            .find(|c| c.callee == callee)
            .unwrap_or_else(|| panic!("no send of {callee}"));
        call.args.iter().map(|a| describe(&a.shape)).collect()
    })
}

#[test]
fn a_literal_argument_is_the_first_argument_of_a_unary_keyword_send() {
    let literal = "cache warmed before first request".chars().count();
    let found = shapes("logMessage");
    assert_eq!(
        found,
        [format!("str:{literal}")],
        "the literal after the keyword is the only argument; observed {found:?}"
    );
}

#[test]
fn multi_keyword_sends_list_only_the_argument_expressions() {
    let literal = "cache.size".chars().count();
    let found = shapes("setValue");
    assert_eq!(
        found,
        ["ident:count".to_string(), format!("str:{literal}")],
        "only argument expressions are listed, no `setValue`/`forKey`; observed {found:?}"
    );
}

#[test]
fn sends_without_arguments_and_nested_sends_keep_their_shape() {
    assert!(shapes("flush").is_empty(), "no keyword has a colon");
    assert_eq!(shapes("relay"), ["other"], "one nested argument");
    let literal = "inner key literal".chars().count();
    let found = shapes("lookup");
    assert_eq!(
        found,
        [format!("str:{literal}")],
        "the nested send keeps its own literal argument; observed {found:?}"
    );
}
