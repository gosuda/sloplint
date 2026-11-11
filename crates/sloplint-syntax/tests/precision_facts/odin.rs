use sloplint_core::facts::{AmbientKind, AssertShape, ImportKind, ImportScope, Visibility};
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
    assert_eq!(
        params[0].2,
        sloplint_core::facts::PrimitiveKind::Str,
        "`string` maps to Str like the Rust shape, got {params:?}"
    );
    assert!(
        params[1].3 && params[1].4,
        "the `:= false` parameter carries its default, got {params:?}"
    );
    for param in &params {
        assert_eq!(
            (param.5, param.6),
            (false, false),
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
        (true, AssertShape::TopEqNe, false),
        "relational assert reports TopEqNe like Rust, got {compared:?}"
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

#[test]
fn chained_call_keeps_last_segment_callee() {
    let calls = with_facts(
        "chain.odin",
        "package p\nrun :: proc(t: rawptr) {\n\twrap(reflect.struct_field_by_name(t, \"id\")).get(0, 1)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .map(|c| {
                    (
                        c.callee.to_owned(),
                        c.callee_root.map(str::to_owned),
                        c.is_simple_path,
                    )
                })
                .collect::<Vec<_>>()
        },
    );
    let outer = calls
        .iter()
        .find(|(callee, _, _)| *callee == "get" || callee.contains("get"))
        .expect("outer chained call is recorded");
    assert_eq!(
        outer,
        &(String::from("get"), None, false),
        "call on a call result keeps the last segment only, got {outer:?}"
    );
    assert!(
        calls.iter().all(|(callee, _, _)| {
            callee == "reflect.struct_field_by_name" || !callee.contains("struct_field_by_name")
        }),
        "reflect text inside arguments must not leak into a callee, got {calls:?}"
    );
}

#[test]
fn procedure_parameter_shadow_stays_quiet() {
    let ambient = with_facts(
        "shadow.odin",
        "package p\ncounter : int = 0\nbump :: proc(counter: int) {\n\tcounter = 1\n}\n",
        |facts| {
            facts
                .ambient
                .iter()
                .map(|a| (a.kind, a.name.to_owned(), a.in_function.is_some()))
                .collect::<Vec<_>>()
        },
    );
    assert!(
        ambient.contains(&(AmbientKind::GlobalMutable, String::from("counter"), false)),
        "file-scope var is a global mutable, got {ambient:?}"
    );
    assert!(
        !ambient
            .iter()
            .any(|(kind, name, _)| { *kind == AmbientKind::GlobalWrite && name == "counter" }),
        "parameter shadow absorbs the write, got {ambient:?}"
    );
}

#[test]
fn walrus_local_shadow_stays_quiet() {
    let ambient = with_facts(
        "walrus.odin",
        "package p\ncounter : int = 0\nrun :: proc() {\n\tcounter := 5\n\tcounter = 6\n}\n",
        |facts| {
            facts
                .ambient
                .iter()
                .map(|a| (a.kind, a.name.to_owned(), a.in_function.is_some()))
                .collect::<Vec<_>>()
        },
    );
    assert!(
        ambient.contains(&(AmbientKind::GlobalMutable, String::from("counter"), false)),
        "file-scope var is a global mutable, got {ambient:?}"
    );
    assert!(
        !ambient
            .iter()
            .any(|(kind, name, _)| { *kind == AmbientKind::GlobalWrite && name == "counter" }),
        "walrus local absorbs the later write, got {ambient:?}"
    );
}

#[test]
fn multi_name_global_reports_each_name() {
    let ambient = with_facts(
        "multi.odin",
        "package p\ncounter_a, counter_b: int\nrun :: proc() {\n\tcounter_b = 1\n}\n",
        |facts| {
            facts
                .ambient
                .iter()
                .map(|a| (a.kind, a.name.to_owned(), a.in_function.is_some()))
                .collect::<Vec<_>>()
        },
    );
    assert!(
        ambient.contains(&(AmbientKind::GlobalMutable, String::from("counter_a"), false)),
        "first name in multi-name global is mutable, got {ambient:?}"
    );
    assert!(
        ambient.contains(&(AmbientKind::GlobalMutable, String::from("counter_b"), false)),
        "second name in multi-name global is mutable, got {ambient:?}"
    );
    assert!(
        ambient
            .iter()
            .any(|(kind, name, _)| { *kind == AmbientKind::GlobalWrite && name == "counter_b" }),
        "write to second name resolves against file scope, got {ambient:?}"
    );
}

#[test]
fn plain_write_to_file_global_reports_global_write() {
    let ambient = with_facts(
        "global_write.odin",
        "package p\ncounter: int\nmain :: proc() {\n\tcounter = 1\n}\n",
        |facts| {
            facts
                .ambient
                .iter()
                .map(|a| (a.kind, a.name.to_owned(), a.in_function.is_some()))
                .collect::<Vec<_>>()
        },
    );
    assert!(
        ambient.contains(&(AmbientKind::GlobalMutable, String::from("counter"), false)),
        "file-scope counter is a global mutable, got {ambient:?}"
    );
    let writes = ambient
        .iter()
        .filter(|(kind, name, _)| *kind == AmbientKind::GlobalWrite && name == "counter")
        .collect::<Vec<_>>();
    assert_eq!(
        writes.len(),
        1,
        "plain in-proc write to the file global is one GlobalWrite, got {ambient:?}"
    );
    assert!(
        writes[0].2,
        "the GlobalWrite sits inside `main`, got {ambient:?}"
    );
    let local = with_facts(
        "local.odin",
        "package p\nrun :: proc() {\n\tx := 1\n\tx = 2\n}\n",
        |facts| {
            facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalWrite)
                .count()
        },
    );
    assert_eq!(
        local, 0,
        "writes to a proc-local stay out of ambient, got {local:?}"
    );
}

#[test]
fn for_in_loop_var_shadow_stays_quiet() {
    let ambient = with_facts(
        "for_in.odin",
        "package p\ncounter: int\nxs: []int\nrun :: proc() {\n\tfor counter in xs {\n\t\tcounter = 1\n\t}\n}\n",
        |facts| {
            facts
                .ambient
                .iter()
                .map(|a| (a.kind, a.name.to_owned(), a.in_function.is_some()))
                .collect::<Vec<_>>()
        },
    );
    assert!(
        ambient.contains(&(AmbientKind::GlobalMutable, String::from("counter"), false)),
        "file-scope counter is a global mutable, got {ambient:?}"
    );
    assert!(
        !ambient
            .iter()
            .any(|(kind, name, _)| { *kind == AmbientKind::GlobalWrite && name == "counter" }),
        "for-in loop variable absorbs the inner write, got {ambient:?}"
    );
    let plain = with_facts(
        "for_in_plain.odin",
        "package p\ncounter: int\nrun :: proc() {\n\tcounter = 1\n}\n",
        |facts| {
            facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalWrite && a.name == "counter")
                .count()
        },
    );
    assert_eq!(
        plain, 1,
        "same write without the loop variable is one GlobalWrite, got {plain:?}"
    );
}

#[test]
fn second_target_write_reports_global_write() {
    let shadowed = with_facts(
        "multi_shadow.odin",
        "package p\ncounter: int\nrun :: proc() {\n\tcounter := 5\n\tx := 0\n\tx, counter = 1, 2\n}\n",
        |facts| {
            facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalWrite && a.name == "counter")
                .count()
        },
    );
    assert_eq!(
        shadowed, 0,
        "local shadows the second target, got {shadowed:?}"
    );
    let plain = with_facts(
        "multi_second.odin",
        "package p\ncounter: int\nrun :: proc() {\n\tx := 0\n\tx, counter = 1, 2\n}\n",
        |facts| {
            facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalWrite && a.name == "counter")
                .count()
        },
    );
    assert_eq!(
        plain, 1,
        "second target naming the file global is one GlobalWrite, got {plain:?}"
    );
}

#[test]
fn package_call_keeps_qualifier_call_receiver_collapses() {
    let qualified = with_facts(
        "job.odin",
        "package p\nrun_job :: proc(x: int) {\n\tlog.info(\"x\")\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee.contains("info"))
                .map(|c| {
                    (
                        c.callee.to_owned(),
                        c.callee_root.map(str::to_owned),
                        c.is_simple_path,
                    )
                })
                .expect("the log call is recorded")
        },
    );
    assert_eq!(
        qualified,
        (String::from("log.info"), Some(String::from("log")), true),
        "a package call keeps its qualifier and root, got {qualified:?}"
    );
    let receiver = with_facts(
        "view.odin",
        "package p\nshow :: proc(x: int) {\n\tget(x).info(\"y\")\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "info")
                .map(|c| {
                    (
                        c.callee.to_owned(),
                        c.callee_root.map(str::to_owned),
                        c.is_simple_path,
                    )
                })
                .expect("the receiver call is recorded")
        },
    );
    assert_eq!(
        receiver,
        (String::from("info"), None, false),
        "a call on a call result keeps the last segment only, got {receiver:?}"
    );
}
