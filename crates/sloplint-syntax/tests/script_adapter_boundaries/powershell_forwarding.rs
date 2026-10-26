//! PowerShell forwarded calls: one command forwarding the parameter links;
//! a second statement breaks the link.
//!
//! Argument identifiers use the same normalized names as declared parameters
//! (the adapter strips the `$` sigil, as for PHP variables), so identity is
//! asserted by direct comparison.

use sloplint_core::facts::{ArgShape, FileFacts};
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const FORWARD: &str = "function Forward($x) {\n  Render-Greeting $x\n}\n";
const TWICE: &str = "function Forward($x) {\n  Render-Greeting $x\n  Render-Greeting $x\n}\n";
const SIDE_EFFECT: &str = "function Forward($x) {\n  $y = 1\n  Render-Greeting $x\n}\n";

fn observe<R>(source: &str, read: impl FnOnce(&FileFacts<'_>) -> R) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "a.ps1",
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "a.ps1 parses cleanly");
    read(&analysis.facts)
}

#[test]
fn genuine_forward_links_call_with_param_identity() {
    observe(FORWARD, |facts| {
        let function = facts
            .functions
            .iter()
            .find(|f| f.name == "Forward")
            .expect("a.ps1 has function Forward");
        let params: Vec<&str> = function.params.iter().map(|p| p.name).collect();
        assert_eq!(params, ["x"], "the wrapper declares its parameter");
        assert_eq!(
            function.sole_call,
            Some(0),
            "one forwarded call links calls[0]"
        );
        let calls: Vec<_> = facts
            .calls
            .iter()
            .filter(|c| c.callee == "Render-Greeting")
            .collect();
        assert_eq!(calls.len(), 1, "exactly one forwarded call exists");
        assert_eq!(calls[0].args.len(), 1, "the call takes one argument");
        let args: Vec<&str> = calls[0]
            .args
            .iter()
            .map(|a| match &a.shape {
                ArgShape::Ident(id) => *id,
                other => panic!("forwarded argument is an identifier, found {other:?}"),
            })
            .collect();
        assert_eq!(
            args, params,
            "the call forwards the declared parameters by name"
        );
    });
}

#[test]
fn second_statement_breaks_the_link() {
    observe(TWICE, |facts| {
        let function = facts
            .functions
            .iter()
            .find(|f| f.name == "Forward")
            .expect("a.ps1 has function Forward");
        let params: Vec<&str> = function.params.iter().map(|p| p.name).collect();
        assert_eq!(params, ["x"], "the two-call variant keeps its parameter");
        assert!(
            function.sole_call.is_none(),
            "two calls are not one forwarded call: {:?}",
            function.sole_call
        );
        assert_eq!(
            facts
                .calls
                .iter()
                .filter(|c| c.callee == "Render-Greeting")
                .count(),
            2,
            "both forwarded calls are still recorded"
        );
    });
}

#[test]
fn side_effect_breaks_the_link() {
    observe(SIDE_EFFECT, |facts| {
        let function = facts
            .functions
            .iter()
            .find(|f| f.name == "Forward")
            .expect("a.ps1 has function Forward");
        let params: Vec<&str> = function.params.iter().map(|p| p.name).collect();
        assert_eq!(params, ["x"], "the side-effect variant keeps its parameter");
        assert!(
            function.sole_call.is_none(),
            "an assignment plus a call is not one forwarded call: {:?}",
            function.sole_call
        );
        let calls: Vec<_> = facts
            .calls
            .iter()
            .filter(|c| c.callee == "Render-Greeting")
            .collect();
        assert_eq!(calls.len(), 1, "the forwarded call is still recorded");
        assert_eq!(calls[0].args.len(), 1, "the call takes one argument");
        let args: Vec<&str> = calls[0]
            .args
            .iter()
            .map(|a| match &a.shape {
                ArgShape::Ident(id) => *id,
                other => panic!("forwarded argument is an identifier, found {other:?}"),
            })
            .collect();
        assert_eq!(
            args, params,
            "the call forwards the declared parameters by name"
        );
    });
}
