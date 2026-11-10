//! PowerShell call-argument shapes: parameter values, parenthesized operands, plus concats.
use std::sync::LazyLock;

use sloplint_core::facts::ArgShape;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> sloplint_syntax::Analysis<'_> {
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "script.ps1",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn param_value_bare_word_is_not_a_message() {
    let analysis =
        facts_of("Write-Host -ForegroundColor Green \"Deployment finished successfully\"\n");
    assert!(analysis.parse_error.is_none(), "powershell parses");
    assert_eq!(analysis.facts.calls.len(), 1, "one log call recorded");
    let shapes: Vec<_> = analysis.facts.calls[0]
        .args
        .iter()
        .map(|a| a.shape.clone())
        .collect();
    assert_eq!(
        shapes,
        [
            ArgShape::Other,
            ArgShape::Other,
            ArgShape::Str { longest_static: 32 },
        ],
        "bare parameter value is not a message"
    );
    let direct = facts_of("Write-Host Green \"Deployment finished successfully\"\n");
    assert!(direct.parse_error.is_none(), "powershell parses");
    assert_eq!(direct.facts.calls.len(), 1, "one direct call recorded");
    let direct_shapes: Vec<_> = direct.facts.calls[0]
        .args
        .iter()
        .map(|a| a.shape.clone())
        .collect();
    assert_eq!(
        direct_shapes,
        [
            ArgShape::Str { longest_static: 5 },
            ArgShape::Str { longest_static: 32 },
        ],
        "leading bare word stays a message"
    );
}

#[test]
fn parenthesized_operands_measure_static_runs() {
    let analysis = facts_of("Write-Host (\"Failed to open file {0}\" -f $path)\n");
    assert!(analysis.parse_error.is_none(), "powershell parses");
    assert_eq!(analysis.facts.calls.len(), 1, "one log call recorded");
    let shapes: Vec<_> = analysis.facts.calls[0]
        .args
        .iter()
        .map(|a| a.shape.clone())
        .collect();
    assert_eq!(
        shapes,
        [ArgShape::Concat { longest_static: 20 }],
        "parenthesized template run excludes placeholder"
    );
    let concat = facts_of("Write-Host (\"Processing order \" + $id)\n");
    assert!(concat.parse_error.is_none(), "powershell parses");
    assert_eq!(concat.facts.calls.len(), 1, "one concat call recorded");
    let concat_shapes: Vec<_> = concat.facts.calls[0]
        .args
        .iter()
        .map(|a| a.shape.clone())
        .collect();
    assert_eq!(
        concat_shapes,
        [ArgShape::Concat { longest_static: 17 }],
        "parenthesized concat keeps operand run"
    );
}

#[test]
fn plus_concat_reports_operand_static_run() {
    let analysis = facts_of("$logger.Error(\"Processing order number \" + $id)\n");
    assert!(analysis.parse_error.is_none(), "powershell parses");
    assert_eq!(analysis.facts.calls.len(), 1, "one error call recorded");
    let shapes: Vec<_> = analysis.facts.calls[0]
        .args
        .iter()
        .map(|a| a.shape.clone())
        .collect();
    assert_eq!(
        shapes,
        [ArgShape::Concat { longest_static: 24 }],
        "plus operand run measured"
    );
    let sub = facts_of("$logger.Error($a - $b)\n");
    assert!(sub.parse_error.is_none(), "powershell parses");
    assert_eq!(sub.facts.calls.len(), 1, "one subtraction call recorded");
    let sub_shapes: Vec<_> = sub.facts.calls[0]
        .args
        .iter()
        .map(|a| a.shape.clone())
        .collect();
    assert_eq!(sub_shapes, [ArgShape::Other], "subtraction is not a concat");
}
