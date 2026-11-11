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
fn field_copy_reports_hashtable_of_member_access() {
    let source = "function Convert-User {\n    param($User)\n    @{\n        Name = $User.Name\n        Email = $User.Email\n        Age = $User.Age\n        City = $User.City\n    }\n}\n";
    let analysis = facts_of(source);
    assert!(analysis.parse_error.is_none(), "powershell parses");
    assert_eq!(
        analysis.facts.field_copies.len(),
        1,
        "one field copy recorded"
    );
    let copy = &analysis.facts.field_copies[0];
    assert_eq!(copy.fields, 4, "four entries counted");
    assert_eq!(copy.source, Some("User"), "source variable kept");
    assert_eq!(copy.pos.line, 3, "copy starts at hashtable line");
}

#[test]
fn parenthesized_format_template_measures_static_run() {
    let source =
        "function Invoke-Log($msg) {\n  Write-Host (\"Failed to open file {0}\" -f $path)\n}\n";
    let analysis = facts_of(source);
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
}
