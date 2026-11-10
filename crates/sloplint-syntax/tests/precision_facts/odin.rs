use sloplint_core::facts::{AssertShape, ImportKind, ImportScope, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn with_facts<R>(
    path: &str,
    source: &str,
    read: impl FnOnce(&sloplint_core::facts::FileFacts<'_>) -> R,
) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Odin,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "{path} parses");
    read(&analysis.facts)
}

#[test]
fn top_level_procedure_is_exported_private_attribute_is_not() {
    let shown = with_facts("show.odin", "package p\nshow :: proc() {\n}\n", |facts| {
        facts
            .functions
            .iter()
            .find(|f| f.name == "show")
            .map(|f| f.visibility)
            .expect("`show` is recorded")
    });
    assert_eq!(
        shown,
        Visibility::Exported,
        "top-level `show` is exported, got {shown:?}"
    );
    let hidden = with_facts(
        "hide.odin",
        "package p\n@(private)\nhide :: proc() {\n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "hide")
                .map(|f| f.visibility)
                .expect("`hide` is recorded")
        },
    );
    assert_eq!(
        hidden,
        Visibility::Private,
        "`@(private)` keeps `hide` private, got {hidden:?}"
    );
}

#[test]
fn default_parameter_keeps_bool_default_with_default_primitive() {
    let params = with_facts(
        "greet.odin",
        "package p\ngreet :: proc(name: string, loud := false) {\n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "greet")
                .map(|f| {
                    f.params
                        .iter()
                        .map(|p| {
                            (
                                p.name.to_owned(),
                                p.ty.map(str::to_owned),
                                p.primitive_kind,
                                p.has_default,
                                p.bool_default,
                                p.keyword_only,
                                p.is_variadic,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .expect("`greet` is recorded")
        },
    );
    assert_eq!(
        params.len(),
        2,
        "`greet` keeps both parameters, got {params:?}"
    );
    assert_eq!(
        params[0].1.as_deref(),
        Some("string"),
        "the typed parameter keeps its type, got {params:?}"
    );
    assert!(
        !params[0].3 && !params[0].4,
        "the plain parameter has no default, got {params:?}"
    );
    assert!(
        !params[1].3 && params[1].4,
        "the `:= false` parameter keeps a false bool default, got {params:?}"
    );
    for param in &params {
        assert_eq!(
            (param.2, param.5, param.6),
            (sloplint_core::facts::PrimitiveKind::Other, false, false),
            "new parameter facts stay at default, got {params:?}"
        );
    }
}

#[test]
fn import_keeps_module_scope_with_no_external_root() {
    let imports = with_facts(
        "use.odin",
        "package p\nimport \"core:fmt\"\nimport f \"core:mem\"\n",
        |facts| {
            facts
                .imports
                .iter()
                .map(|i| {
                    (
                        i.module.to_string(),
                        i.name.to_owned(),
                        i.alias.map(str::to_owned),
                        i.kind,
                        i.scope,
                        i.in_test,
                        i.is_reexport,
                        i.external_root.map(str::to_owned),
                    )
                })
                .collect::<Vec<_>>()
        },
    );
    assert_eq!(
        imports.len(),
        2,
        "both imports are recorded, got {imports:?}"
    );
    assert_eq!(
        (
            imports[0].3,
            imports[0].4,
            imports[0].5,
            imports[0].6,
            imports[0].7.as_deref()
        ),
        (
            ImportKind::Namespace,
            ImportScope::Module,
            false,
            false,
            None
        ),
        "plain import stays module-scoped with no external root, got {imports:?}"
    );
    assert_eq!(
        imports[1].2.as_deref(),
        Some("f"),
        "aliased import keeps its alias, got {imports:?}"
    );
}

#[test]
fn assert_comparison_keeps_default_shape() {
    let compared = with_facts(
        "check.odin",
        "package p\ncheck :: proc(x: int, y: int) {\n\tassert(x == y)\n}\n",
        |facts| {
            facts
                .assertions
                .first()
                .map(|a| (a.comparison, a.shape, a.has_message))
                .expect("the assert is recorded")
        },
    );
    assert_eq!(
        compared,
        (true, AssertShape::Other, false),
        "relational assert keeps comparison with default shape, got {compared:?}"
    );
    let bare = with_facts(
        "plain.odin",
        "package p\nplain :: proc(ok: bool) {\n\tensure(ok)\n}\n",
        |facts| {
            facts
                .assertions
                .first()
                .map(|a| a.comparison)
                .expect("the ensure is recorded")
        },
    );
    assert!(!bare, "bare ensure reports no comparison");
}

#[test]
fn bare_call_reports_simple_path_and_binding_defaults() {
    let call = with_facts(
        "run.odin",
        "package p\nrun :: proc() {\n\ttarget()\n}\n",
        |facts| {
            facts
                .calls
                .first()
                .map(|c| {
                    (
                        c.callee.to_owned(),
                        c.callee_root.map(str::to_owned),
                        c.is_simple_path,
                        c.initializes_binding,
                    )
                })
                .expect("the call is recorded")
        },
    );
    assert_eq!(
        call,
        (
            String::from("target"),
            Some(String::from("target")),
            true,
            false
        ),
        "bare call reports a simple path with binding defaults, got {call:?}"
    );
}

#[test]
fn test_attribute_marks_function_without_forcing_signature() {
    let found = with_facts(
        "calc.odin",
        "package probe\n@(test)\ncheck_it :: proc() {\n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "check_it")
                .map(|f| (f.in_test, f.entry_point, f.signature_forced))
                .expect("`check_it` is recorded")
        },
    );
    assert_eq!(
        found,
        (true, false, false),
        "`@(test)` marks in-test without entry or forced signature, got {found:?}"
    );
}
