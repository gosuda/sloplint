//! PowerShell pipeline input: a tail command with no inline arguments takes its
//! message operand from the nearest preceding pipeline_chain sibling.

use sloplint_core::facts::{ArgShape, FileFacts};
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn observe<R>(source: &str, read: impl FnOnce(&FileFacts<'_>) -> R) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "a.ps1",
        source,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "a.ps1 parses cleanly");
    read(&analysis.facts)
}

fn describe(shape: &ArgShape<'_>) -> String {
    match shape {
        ArgShape::Str { longest_static } => format!("str:{longest_static}"),
        ArgShape::Concat => "concat".to_string(),
        ArgShape::Ident(name) => format!("ident:{name}"),
        ArgShape::Other => "other".to_string(),
    }
}

fn shapes_of(source: &str, callee: &str) -> Vec<Vec<String>> {
    observe(source, |facts| {
        facts
            .calls
            .iter()
            .filter(|c| c.callee == callee)
            .map(|c| c.args.iter().map(|a| describe(&a.shape)).collect())
            .collect()
    })
}

#[test]
fn piped_literal_is_a_searchable_first_argument() {
    let source =
        "function Invoke-Log($msg) {\n  \"deployment completed successfully\" | Write-Host\n}\n";
    assert_eq!(shapes_of(source, "Write-Host"), [["str:33".to_string()]]);
}

#[test]
fn piped_variable_stays_dynamic() {
    let source = "function Invoke-Log($msg) {\n  $msg | Write-Host\n}\n";
    assert_eq!(shapes_of(source, "Write-Host"), [["ident:msg".to_string()]]);
}

#[test]
fn multi_stage_pipeline_uses_the_nearest_command() {
    let source = "\"lit\" | Get-Thing | Write-Host\n";
    assert_eq!(shapes_of(source, "Get-Thing"), [["str:3".to_string()]]);
    assert_eq!(shapes_of(source, "Write-Host"), [["other".to_string()]]);
}

#[test]
fn standalone_call_keeps_no_message() {
    assert_eq!(
        shapes_of("Write-Host\n", "Write-Host"),
        [Vec::<String>::new()]
    );
}

#[test]
fn explicit_argument_beats_pipeline_input() {
    let piped = "function Invoke-Log($msg) {\n  \"deployment completed successfully\" | Write-Host $msg\n}\n";
    let standalone = "function Invoke-Log($msg) {\n  Write-Host $msg\n}\n";
    assert_eq!(
        shapes_of(piped, "Write-Host"),
        shapes_of(standalone, "Write-Host"),
        "explicit args are preserved with no extra piped argument"
    );
    assert_eq!(
        shapes_of(piped, "Write-Host")
            .iter()
            .map(|args| args.len())
            .collect::<Vec<_>>(),
        [1],
        "exactly one explicit argument; separator whitespace is not an argument"
    );
}

#[test]
fn piped_body_is_not_a_single_forwarded_call() {
    let source = "function Forward($x) {\n  $x | Inner\n}\n";
    observe(source, |facts| {
        let function = facts
            .functions
            .iter()
            .find(|f| f.name == "Forward")
            .expect("a.ps1 has function Forward");
        assert!(
            function.sole_call.is_none(),
            "piped body is not a sole call"
        );
        let call = facts
            .calls
            .iter()
            .find(|c| c.callee == "Inner")
            .expect("a.ps1 has a call to Inner");
        assert_eq!(
            call.args
                .iter()
                .map(|a| describe(&a.shape))
                .collect::<Vec<_>>(),
            ["ident:x".to_string()]
        );
    });
}

#[test]
fn conditional_chain_is_not_pipeline_input() {
    let source = "Get-Thing && Write-Host\nGet-Thing || Write-Host\n";
    assert_eq!(
        shapes_of(source, "Write-Host"),
        [Vec::<String>::new(), Vec::<String>::new()]
    );
}

#[test]
fn separate_statements_do_not_share_pipeline_input() {
    let source = "\"deployment completed successfully\"\nWrite-Host\n";
    assert_eq!(shapes_of(source, "Write-Host"), [Vec::<String>::new()]);
}

#[test]
fn unary_operated_literal_stays_computed() {
    let source = "-not \"deployment completed successfully\" | Write-Host\n";
    assert_eq!(shapes_of(source, "Write-Host"), [["other".to_string()]]);
}

#[test]
fn multiline_operand_keeps_its_written_span() {
    let source = "(\n  Get-Thing\n) | Write-Host\n";
    observe(source, |facts| {
        let call = facts
            .calls
            .iter()
            .find(|c| c.callee == "Write-Host")
            .expect("a.ps1 has a call to Write-Host");
        assert_eq!(call.args.len(), 1, "piped input is the only argument");
        assert_eq!(describe(&call.args[0].shape), "other");
        assert_eq!(
            (call.args[0].start_line, call.args[0].end_line),
            (1, 3),
            "span comes from the written operand, not the unwrapped leaf"
        );
    });
}
