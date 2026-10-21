//! Forwarded-call extraction for Zig and Odin, observed through `analyze` only.
//!
//! A wrapper whose body is exactly one forwarded call links that call through
//! `sole_call`; adding a second call breaks the link. Both cases pin their
//! parameters as non-empty, so the test cannot pass vacuously on a missing
//! parameter list.

use sloplint_core::facts::ArgShape;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

const ZIG_FORWARD: &str = "pub fn greet(name: []const u8) void {\n\trenderGreeting(name);\n}\n";

const ZIG_TWO_CALLS: &str =
    "pub fn greet(name: []const u8) void {\n\trenderGreeting(name);\n\trenderGreeting(name);\n}\n";

const ODIN_FORWARD: &str = "greet :: proc(name: string) {\n\trender_greeting(name)\n}\n";

const ODIN_TWO_CALLS: &str =
    "greet :: proc(name: string) {\n\trender_greeting(name)\n\trender_greeting(name)\n}\n";

/// The wrapper's parameter names, its linked call index, and the forwarded argument names.
fn forwarded<'s>(
    lang: Lang,
    path: &'s str,
    source: &'s str,
    name: &str,
) -> (Vec<&'s str>, Option<u32>, Vec<&'s str>) {
    let analysis = run_clean(lang, path, source);
    let function = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{path} has function {name}"));
    let params: Vec<&'s str> = function.params.iter().map(|p| p.name).collect();
    let linked = function.sole_call;
    let args = match linked {
        Some(index) => {
            let at = usize::try_from(index).expect("call index fits");
            let calls = &analysis.facts.calls;
            let call = calls
                .get(at)
                .unwrap_or_else(|| panic!("{path} sole_call {index} points at a call"));
            call.args
                .iter()
                .map(|a| match &a.shape {
                    ArgShape::Ident(id) => *id,
                    other => panic!("{path} forwards identifiers, found {other:?}"),
                })
                .collect()
        }
        None => Vec::new(),
    };
    (params, linked, args)
}

#[test]
fn zig_single_forwarded_call_links_and_a_second_call_breaks_the_link() {
    let (params, linked, args) = forwarded(Lang::Zig, "greet.zig", ZIG_FORWARD, "greet");
    assert_eq!(params, ["name"], "the wrapper takes a parameter");
    assert_eq!(args, ["name"], "the single call forwards that parameter");
    assert_eq!(linked, Some(0), "the single call is calls[0]: {linked:?}");

    let (params, linked, _) = forwarded(Lang::Zig, "greet.zig", ZIG_TWO_CALLS, "greet");
    assert_eq!(params, ["name"], "the two-call variant keeps its parameter");
    assert!(
        linked.is_none(),
        "two calls are not one forwarded call: {linked:?}"
    );
}

#[test]
fn odin_single_forwarded_call_links_and_a_second_call_breaks_the_link() {
    let (params, linked, args) = forwarded(Lang::Odin, "greet.odin", ODIN_FORWARD, "greet");
    assert_eq!(params, ["name"], "the wrapper takes a parameter");
    assert_eq!(args, ["name"], "the single call forwards that parameter");
    assert_eq!(linked, Some(0), "the single call is calls[0]: {linked:?}");

    let (params, linked, _) = forwarded(Lang::Odin, "greet.odin", ODIN_TWO_CALLS, "greet");
    assert_eq!(params, ["name"], "the two-call variant keeps its parameter");
    assert!(
        linked.is_none(),
        "two calls are not one forwarded call: {linked:?}"
    );
}
