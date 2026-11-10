use sloplint_core::facts::{ArgShape, FileFacts, PrimitiveKind, ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

struct ParamFacts {
    name: String,
    ty: Option<String>,
    has_default: bool,
    bool_default: bool,
    primitive_kind: PrimitiveKind,
    keyword_only: bool,
    is_variadic: bool,
}

struct FnFacts {
    name: String,
    visibility: Visibility,
    signature_forced: bool,
    entry_point: bool,
    in_test: bool,
    is_overload: bool,
    is_deprecated: bool,
    is_decorated: bool,
    is_trait_item: bool,
    params: Vec<ParamFacts>,
}

fn observe<R>(source: &str, read: impl FnOnce(&FileFacts<'_>) -> R) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "script.ps1",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "powershell parses");
    read(&analysis.facts)
}

fn functions_of(source: &str) -> Vec<FnFacts> {
    observe(source, |facts| {
        facts
            .functions
            .iter()
            .map(|f| FnFacts {
                name: f.name.to_owned(),
                visibility: f.visibility,
                signature_forced: f.signature_forced,
                entry_point: f.entry_point,
                in_test: f.in_test,
                is_overload: f.is_overload,
                is_deprecated: f.is_deprecated,
                is_decorated: f.is_decorated,
                is_trait_item: f.is_trait_item,
                params: f
                    .params
                    .iter()
                    .map(|p| ParamFacts {
                        name: p.name.to_owned(),
                        ty: p.ty.map(str::to_owned),
                        has_default: p.has_default,
                        bool_default: p.bool_default,
                        primitive_kind: p.primitive_kind,
                        keyword_only: p.keyword_only,
                        is_variadic: p.is_variadic,
                    })
                    .collect(),
            })
            .collect()
    })
}

fn calls_of(source: &str) -> Vec<(String, bool, bool)> {
    observe(source, |facts| {
        facts
            .calls
            .iter()
            .map(|c| (c.callee.to_owned(), c.is_simple_path, c.initializes_binding))
            .collect()
    })
}

type CallRow = (String, Option<String>, bool, bool, Vec<(String, u32, u32)>);
fn describe_shape(shape: &ArgShape<'_>) -> String {
    match shape {
        ArgShape::Str { longest_static } => format!("str:{longest_static}"),
        ArgShape::Concat { longest_static } => format!("concat:{longest_static}"),
        ArgShape::Ident(name) => format!("ident:{name}"),
        ArgShape::Collection => "collection".to_string(),
        ArgShape::CodeBody => "code".to_string(),
        ArgShape::Other => "other".to_string(),
    }
}

fn calls_full_of(source: &str) -> Vec<CallRow> {
    observe(source, |facts| {
        facts
            .calls
            .iter()
            .map(|c| {
                (
                    c.callee.to_owned(),
                    c.callee_root.map(str::to_owned),
                    c.is_simple_path,
                    c.initializes_binding,
                    c.args
                        .iter()
                        .map(|a| (describe_shape(&a.shape), a.start_line, a.end_line))
                        .collect(),
                )
            })
            .collect()
    })
}

fn arms_of(source: &str) -> Vec<(ScrutineeKind, u32, bool, bool)> {
    observe(source, |facts| {
        facts
            .wildcard_arms
            .iter()
            .map(|a| {
                (
                    a.scrutinee,
                    a.variant_count,
                    a.has_nullable_ctor,
                    a.enum_like,
                )
            })
            .collect()
    })
}

#[test]
fn top_level_function_is_exported() {
    let rows = functions_of("function Get-Greeting {\n    \"hi\"\n}\n");
    assert_eq!(rows.len(), 1, "one function recorded");
    assert_eq!(rows[0].name, "Get-Greeting", "function name kept");
    assert_eq!(
        rows[0].visibility,
        Visibility::Exported,
        "top-level function is exported"
    );
    let hidden =
        functions_of("class Widget {\n    hidden [string] GetSecret() { return \"x\" }\n}\n");
    assert_eq!(hidden.len(), 1, "hidden method recorded");
    assert_eq!(
        hidden[0].visibility,
        Visibility::Private,
        "hidden method is private"
    );
}

#[test]
fn function_head_carries_no_forced_signature() {
    let rows = functions_of("function Get-Greeting {\n    \"hi\"\n}\n");
    assert_eq!(rows.len(), 1, "one function recorded");
    assert!(
        !rows[0].signature_forced,
        "powershell never forces a signature"
    );
    assert!(
        !rows[0].entry_point,
        "powershell has no entry-point concept"
    );
    assert!(!rows[0].in_test, "library file is not a test region");
    assert!(!rows[0].is_overload, "powershell has no overloads");
    assert!(!rows[0].is_deprecated, "deprecation is not tracked");
    assert!(!rows[0].is_decorated, "decoration is not tracked");
    assert!(!rows[0].is_trait_item, "powershell has no traits");
    let methods = functions_of("class Widget {\n    [string] Name() { return \"x\" }\n}\n");
    assert_eq!(methods.len(), 1, "class method recorded");
    assert!(
        !methods[0].signature_forced,
        "class method also forces nothing"
    );
}

#[test]
fn powershell_typed_params_report_primitive_kinds() {
    let rows = functions_of(
        "function Send-Report([string]$Name = \"World\", [bool]$Flag = $true, [int]$Count) {\n    $Name\n}\n",
    );
    assert_eq!(rows.len(), 1, "one function recorded");
    assert_eq!(rows[0].params.len(), 3, "three params recorded");
    assert_eq!(rows[0].params[0].name, "Name", "first param name kept");
    assert!(rows[0].params[0].has_default, "string default recorded");
    assert!(
        !rows[0].params[0].bool_default,
        "string default is not a bool default"
    );
    assert!(rows[0].params[1].has_default, "bool default recorded");
    assert!(
        rows[0].params[1].bool_default,
        "$true default is a bool default"
    );
    assert!(
        !rows[0].params[2].has_default,
        "param without default stays default"
    );
    assert!(
        !rows[0].params[2].bool_default,
        "param without default has no bool default"
    );
    assert_eq!(
        rows[0].params[0].primitive_kind,
        PrimitiveKind::Str,
        "string annotation is a string kind"
    );
    assert_eq!(
        rows[0].params[1].primitive_kind,
        PrimitiveKind::Bool,
        "bool annotation is a bool kind"
    );
    assert_eq!(
        rows[0].params[2].primitive_kind,
        PrimitiveKind::Int,
        "int annotation is an int kind"
    );
    for param in &rows[0].params {
        assert!(!param.keyword_only, "powershell has no keyword-only params");
        assert!(!param.is_variadic, "powershell tracks no variadics");
    }
    let plain = functions_of("function Get-Thing([string]$Name) {\n    $Name\n}\n");
    assert_eq!(plain.len(), 1, "plain function recorded");
    assert_eq!(plain[0].params.len(), 1, "one param recorded");
    assert!(
        !plain[0].params[0].has_default,
        "near-miss param has no default"
    );
    let untyped = functions_of("function Get-Thing($Name) {\n    $Name\n}\n");
    assert_eq!(untyped.len(), 1, "untyped function recorded");
    assert_eq!(untyped[0].params.len(), 1, "one param recorded");
    assert_eq!(
        untyped[0].params[0].primitive_kind,
        PrimitiveKind::Other,
        "untyped param keeps the default kind"
    );
}

#[test]
fn powershell_command_calls_report_simple_paths() {
    let rows = calls_of("Get-ChildItem -Path $dir\n");
    assert!(!rows.is_empty(), "command call recorded");
    for (_, is_simple_path, initializes_binding) in &rows {
        assert!(is_simple_path, "command name is a simple path");
        assert!(!initializes_binding, "plain command initializes no binding");
    }
    let invoked = calls_of("$list.Add(1)\n");
    assert!(!invoked.is_empty(), "method invocation recorded");
    for (_, is_simple_path, _) in &invoked {
        assert!(is_simple_path, "variable receiver chain is a simple path");
    }
    let bound = calls_of("$items = Get-ChildItem -Path $dir\n");
    assert!(!bound.is_empty(), "initializer call recorded");
    for (_, _, initializes_binding) in &bound {
        assert!(initializes_binding, "whole-value call marks its binding");
    }
}

#[test]
fn body_param_block_collects_typed_params() {
    let rows = functions_of(
        "function Confirm-Deploy {\n    param(\n        [bool]$Force,\n        [bool]$DryRun\n    )\n}\n",
    );
    assert_eq!(rows.len(), 1, "one function recorded");
    assert_eq!(rows[0].params.len(), 2, "both body params recorded");
    assert_eq!(rows[0].params[0].name, "Force", "first param kept");
    assert_eq!(
        rows[0].params[0].ty.as_deref(),
        Some("[bool]"),
        "bool annotation kept"
    );
    assert_eq!(
        rows[0].params[0].primitive_kind,
        PrimitiveKind::Bool,
        "bool kind recorded"
    );
    assert_eq!(rows[0].params[1].name, "DryRun", "second param kept");
    let bare = functions_of("function Get-Thing {\n    \"hi\"\n}\n");
    assert_eq!(bare.len(), 1, "body without params recorded");
    assert!(bare[0].params.is_empty(), "param-less body stays empty");
}

#[test]
fn class_method_params_report_types() {
    let rows = functions_of("class Store {\n    [void] Save([bool]$Force, [string]$Name) {}\n}\n");
    assert_eq!(rows.len(), 1, "one method recorded");
    assert_eq!(rows[0].params.len(), 2, "both method params recorded");
    assert_eq!(rows[0].params[0].name, "Force", "first method param kept");
    assert_eq!(
        rows[0].params[0].primitive_kind,
        PrimitiveKind::Bool,
        "bool method param kind kept"
    );
    assert_eq!(
        rows[0].params[1].primitive_kind,
        PrimitiveKind::Str,
        "string method param kind kept"
    );
    let plain = functions_of("class Store {\n    [void] Save() {}\n}\n");
    assert_eq!(plain.len(), 1, "param-less method recorded");
    assert!(plain[0].params.is_empty(), "param-less method stays empty");
}

#[test]
fn interpolated_strings_report_static_runs() {
    let rows = calls_full_of(
        "Write-Host \"Done successfully: $x\"\nWrite-Host \"Done: $x\"\nWrite-Host \"$CompanyName\"\nWrite-Host \"plain\"\n",
    );
    assert_eq!(rows.len(), 4, "four log calls recorded");
    assert!(
        rows[0].4.len() == 1 && rows[0].4[0].0 == "concat:19",
        "long static run kept"
    );
    assert!(
        rows[1].4.len() == 1 && rows[1].4[0].0 == "concat:6",
        "short static run kept"
    );
    assert!(
        rows[2].4.len() == 1 && rows[2].4[0].0 == "concat:0",
        "fully dynamic string reports zero"
    );
    assert!(
        rows[3].4.len() == 1 && rows[3].4[0].0 == "str:5",
        "plain string keeps full length"
    );
}

#[test]
fn variable_command_callee_is_not_simple() {
    let rows = calls_full_of("function Invoke-Log($Message) { & $Logger $Message }\n");
    assert_eq!(rows.len(), 1, "one forwarded call recorded");
    assert_eq!(rows[0].0, "$Logger", "variable callee text kept");
    assert!(!rows[0].2, "variable callee is not a simple path");
    assert_eq!(rows[0].1, None, "variable callee has no identifier root");
    let plain = calls_full_of("Get-ChildItem -Path $dir\n");
    assert!(!plain.is_empty(), "plain command recorded");
    assert!(plain[0].2, "bare command name stays a simple path");
    assert_eq!(
        plain[0].1.as_deref(),
        Some("Get-ChildItem"),
        "bare name keeps its root"
    );
}

#[test]
fn chained_invocation_callee_drops_receiver_call() {
    let rows = calls_full_of("$a.Get().Trim()\n");
    assert_eq!(rows.len(), 2, "outer and inner calls recorded");
    let outer = rows
        .iter()
        .find(|(callee, _, _, _, _)| callee == "Trim")
        .expect("outer call recorded");
    assert!(!outer.2, "call-rooted chain is not a simple path");
    assert_eq!(outer.1, None, "call-rooted chain has no identifier root");
    let inner = rows
        .iter()
        .find(|(callee, _, _, _, _)| callee == "$a.Get")
        .expect("inner call recorded");
    assert!(inner.2, "variable receiver chain is a simple path");
    assert_eq!(
        inner.1.as_deref(),
        Some("a"),
        "variable receiver keeps its root"
    );
    let direct = calls_full_of("$list.Add(1)\n");
    assert_eq!(direct.len(), 1, "direct member call recorded");
    assert_eq!(
        direct[0].0, "$list.Add",
        "plain receiver path keeps its text"
    );
    assert!(direct[0].2, "variable receiver chain is a simple path");
    assert_eq!(
        direct[0].1.as_deref(),
        Some("list"),
        "variable receiver keeps its root"
    );
}

#[test]
fn default_arm_counts_enum_variants_through_member_access() {
    let rows = arms_of(
        "switch ($c) {\n    ([Color]::Red) { 1 }\n    ([Color]::Blue) { 2 }\n    default { 3 }\n}\n",
    );
    assert_eq!(rows.len(), 1, "default arm recorded once");
    assert!(rows[0].3, "two variants of one type are enum-like");
    assert_eq!(rows[0].1, 2, "both variants counted");
    let mixed = arms_of(
        "switch ($c) {\n    ([Color]::Red) { 1 }\n    ([Size]::Big) { 2 }\n    default { 3 }\n}\n",
    );
    assert_eq!(mixed.len(), 1, "mixed-type default recorded");
    assert!(
        !mixed[0].3,
        "single variants of different types are not enum-like"
    );
}
