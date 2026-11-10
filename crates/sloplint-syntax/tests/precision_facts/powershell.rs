use sloplint_core::facts::{PrimitiveKind, ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

struct ParamFacts {
    name: String,
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

fn functions_of(source: &str) -> Vec<FnFacts> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "script.ps1",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "powershell parses");
    analysis
        .facts
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
                    has_default: p.has_default,
                    bool_default: p.bool_default,
                    primitive_kind: p.primitive_kind,
                    keyword_only: p.keyword_only,
                    is_variadic: p.is_variadic,
                })
                .collect(),
        })
        .collect()
}

fn calls_of(source: &str) -> Vec<(String, bool, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "script.ps1",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "powershell parses");
    analysis
        .facts
        .calls
        .iter()
        .map(|c| (c.callee.to_owned(), c.is_simple_path, c.initializes_binding))
        .collect()
}

fn arms_of(source: &str) -> Vec<(ScrutineeKind, u32, bool, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "script.ps1",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "powershell parses");
    analysis
        .facts
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
}

fn handlers_of(source: &str) -> Vec<(bool, bool, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::PowerShell,
        path: "script.ps1",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "powershell parses");
    analysis
        .facts
        .handlers
        .iter()
        .map(|h| (h.has_else, h.try_imports_only, h.in_cleanup))
        .collect()
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
        assert!(!is_simple_path, "member invocation is not a simple path");
    }
    let bound = calls_of("$items = Get-ChildItem -Path $dir\n");
    assert!(!bound.is_empty(), "initializer call recorded");
    for (_, _, initializes_binding) in &bound {
        assert!(
            !initializes_binding,
            "assignment target still leaves the binding fact at default"
        );
    }
}

#[test]
fn default_arm_and_catch_carry_no_extra_facts() {
    let source = "switch ($x) {\n    1 { \"one\" }\n    default { \"other\" }\n}\ntry { Do-Thing } catch { Write-Host \"bad\" }\n";
    let arms = arms_of(source);
    assert_eq!(arms.len(), 1, "default arm recorded once");
    assert_eq!(
        arms[0].0,
        ScrutineeKind::Other,
        "switch scrutinee stays Other"
    );
    assert_eq!(arms[0].1, 0, "powershell counts no variants");
    assert!(!arms[0].2, "powershell has no nullable constructors");
    let handlers = handlers_of(source);
    assert_eq!(handlers.len(), 1, "catch handler recorded");
    assert!(!handlers[0].0, "catch has no else branch");
    assert!(!handlers[0].1, "catch is not imports-only");
    assert!(!handlers[0].2, "catch is not a cleanup frame");
    let bare = arms_of("switch ($x) {\n    1 { \"one\" }\n}\n");
    assert!(
        bare.is_empty(),
        "switch without default has no wildcard arm"
    );
    let no_try = handlers_of("$x = 1\n");
    assert!(no_try.is_empty(), "file without try has no handlers");
}
