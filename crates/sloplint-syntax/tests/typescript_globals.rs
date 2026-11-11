use sloplint_core::facts::{AmbientKind, ImportScope, ScrutineeKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

fn analyze_source<'s>(
    parsers: &mut Parsers<'_>,
    custom: &'s CustomRules,
    lang: Lang,
    path: &'s str,
    source: &'s str,
) -> Analysis<'s> {
    let input = FileInput {
        lang,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(parsers, &input, custom);
    assert!(
        analysis.parse_error.is_none(),
        "wave-self-lint: {lang:?} source must parse cleanly"
    );
    analysis
}

fn with_harness(lang: Lang, path: &str, source: &str, check: impl FnOnce(Analysis<'_>)) {
    let mut parsers = Parsers::new();
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_source(&mut parsers, &custom, lang, path, source);
    check(analysis);
}

fn globals(lang: Lang, source: &str) -> (Vec<String>, bool, bool) {
    let mut parsers = Parsers::new();
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let analysis = analyze_source(&mut parsers, &custom, lang, "a.ts", source);
    let names = analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalMutable)
        .map(|a| a.name.to_string())
        .collect();
    let env = analysis
        .facts
        .ambient
        .iter()
        .any(|a| a.kind == AmbientKind::EnvRead);
    let const_global = analysis
        .facts
        .ambient
        .iter()
        .any(|a| a.kind == AmbientKind::GlobalMutable && a.name == "app");
    (names, env, const_global)
}

const SOURCE: &str = "let top_let = 1;\nvar top_var = 2;\nexport let exported_let = 3;\nconst app = express();\napp.get('/', (req, res) => {\n  let hits = 0;\n  res.send(String(++hits));\n});\nfor (let i = 0; i < 3; i++) {\n  console.log(i);\n}\n{\n  let block_let = 0;\n}\nconst mode = process.env.MODE;\n";

#[test]
fn typescript_locals_are_not_globals() {
    let (names, env, const_global) = globals(Lang::TypeScript, SOURCE);
    for keep in ["top_let", "top_var", "exported_let"] {
        assert!(
            names.contains(&keep.to_string()),
            "missing {keep}: {names:?}"
        );
    }
    for local in ["hits", "i", "block_let"] {
        assert!(
            !names.contains(&local.to_string()),
            "leaked {local}: {names:?}"
        );
    }
    assert!(!const_global, "const leaked: {names:?}");
    assert!(env, "process.env read lost: {names:?}");
}

#[test]
fn javascript_matches_typescript_scoping() {
    let (ts, _, _) = globals(Lang::TypeScript, SOURCE);
    let (js, _, _) = globals(Lang::JavaScript, SOURCE);
    assert_eq!(
        ts, js,
        "wave-self-lint: JavaScript scoping diverged from TypeScript: ts={ts:?} js={js:?}"
    );
    assert!(
        !js.contains(&"hits".to_string()),
        "wave-self-lint: block-local `hits` leaked to globals: {js:?}"
    );
}

#[test]
fn it_inside_describe_if_gets_test_region() {
    let source = "describe('s', () => {\n  if (cond) {\n    it('x', () => {})\n  }\n})\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", source, |analysis| {
            assert_eq!(
                analysis.facts.role_at(1),
                CodeRole::Test,
                "{lang:?}: describe line must be a Test region"
            );
            assert_eq!(
                analysis.facts.role_at(3),
                CodeRole::Test,
                "{lang:?}: it line inside if must stay a Test region"
            );
        });
    }
}

#[test]
fn exported_functions_are_exported() {
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(
            lang,
            "a.ts",
            "export function f() {}\nfunction g() {}\n",
            |analysis| {
                let has_exported = analysis
                    .facts
                    .functions
                    .iter()
                    .any(|f| f.visibility == Visibility::Exported);
                let has_private = analysis
                    .facts
                    .functions
                    .iter()
                    .any(|f| f.visibility == Visibility::Private);
                assert!(has_exported, "{lang:?}: exported fn must be Exported");
                assert!(has_private, "{lang:?}: local fn must be Private");
            },
        );
    }
}

#[test]
fn function_assignment_to_top_let_is_global_write() {
    let source = "let top_let = 1;\nfunction f() {\n  top_let = 2;\n}\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", source, |analysis| {
            assert!(
                analysis.facts.ambient.iter().any(|a| {
                    a.kind == AmbientKind::GlobalWrite
                        && a.name == "top_let"
                        && a.in_function.is_some()
                }),
                "{lang:?}: assignment to file-scope let inside a fn must be a GlobalWrite"
            );
        });
    }
}

#[test]
fn plain_read_of_top_let_is_not_a_global_write() {
    let source = "let counter = 0;\nfunction bump() {\n  return counter;\n}\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", source, |analysis| {
            assert!(
                !analysis
                    .facts
                    .ambient
                    .iter()
                    .any(|a| a.kind == AmbientKind::GlobalWrite && a.name == "counter"),
                "{lang:?}: plain read of file-scope let inside a fn must not be a GlobalWrite"
            );
        });
    }
}

#[test]
fn in_function_writes_to_top_let_are_global_writes() {
    for (label, stmt) in [
        ("assign", "counter = 1;"),
        ("augmented", "counter += 1;"),
        ("update", "counter++;"),
    ] {
        let source = format!("let counter = 0;\nfunction bump() {{\n  {stmt}\n}}\n");
        for lang in [Lang::JavaScript, Lang::TypeScript] {
            with_harness(lang, "a.ts", &source, |analysis| {
                assert!(
                    analysis.facts.ambient.iter().any(|a| {
                        a.kind == AmbientKind::GlobalWrite
                            && a.name == "counter"
                            && a.in_function.is_some()
                    }),
                    "{lang:?}: {label} to file-scope let inside a fn must be a GlobalWrite"
                );
            });
        }
    }
}

#[test]
fn in_function_destructuring_writes_to_top_let_are_global_writes() {
    for (label, stmt) in [
        ("array", "[counter] = [1];"),
        ("object", "({ counter } = obj);"),
        ("parenthesized", "(counter) = 1;"),
    ] {
        let source = format!("let counter = 0;\nfunction reset() {{\n  {stmt}\n}}\n");
        for lang in [Lang::JavaScript, Lang::TypeScript] {
            with_harness(lang, "a.ts", &source, |analysis| {
                assert!(
                    analysis.facts.ambient.iter().any(|a| {
                        a.kind == AmbientKind::GlobalWrite
                            && a.name == "counter"
                            && a.in_function.is_some()
                    }),
                    "{lang:?}: {label} to file-scope let inside a fn must be a GlobalWrite"
                );
            });
        }
    }
}

#[test]
fn env_store_is_global_write_while_read_stays_read() {
    let source =
        "const mode = process.env.MODE;\nfunction f() {\n  process.env.TOKEN = \"x\";\n}\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", source, |analysis| {
            assert!(
                analysis
                    .facts
                    .ambient
                    .iter()
                    .any(|a| a.kind == AmbientKind::GlobalWrite && a.in_function.is_some()),
                "{lang:?}: process.env store inside a fn must be a GlobalWrite"
            );
            assert!(
                analysis
                    .facts
                    .ambient
                    .iter()
                    .any(|a| a.kind == AmbientKind::EnvRead && a.in_function.is_none()),
                "{lang:?}: top-level process.env read must stay an EnvRead"
            );
        });
    }
}

#[test]
fn bare_imports_carry_external_roots() {
    let source = "import React from \"react\";\nimport { x } from \"@scope/pkg/sub\";\nimport y from \"./local\";\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", source, |analysis| {
            let roots: Vec<Option<&str>> = analysis
                .facts
                .imports
                .iter()
                .map(|i| i.external_root)
                .collect();
            assert!(
                roots.contains(&Some("react")),
                "{lang:?}: bare root lost: {roots:?}"
            );
            assert!(
                roots.contains(&Some("@scope/pkg")),
                "{lang:?}: scoped root lost: {roots:?}"
            );
            assert!(
                roots.contains(&None),
                "{lang:?}: relative import must have no root: {roots:?}"
            );
            assert!(
                analysis
                    .facts
                    .imports
                    .iter()
                    .all(|i| { i.scope == ImportScope::Module && !i.in_test && !i.is_reexport }),
                "{lang:?}: top-level imports must be module scope outside tests"
            );
        });
    }
}

#[test]
fn ts_enum_switch_default_tracks_scrutinee_and_variants() {
    let source = "enum Color { Red, Green }\nfunction f(c: Color) {\n  switch (c) {\n    case Color.Red:\n      break;\n    case Color.Green:\n      break;\n    default:\n      break;\n  }\n}\n";
    with_harness(Lang::TypeScript, "a.ts", source, |analysis| {
        assert_eq!(analysis.facts.wildcard_arms.len(), 1);
        let arm = &analysis.facts.wildcard_arms[0];
        assert!(arm.enum_like, "file-local enum switch must be enum-like");
        assert_eq!(arm.variant_count, 2, "two enum cases expected");
        assert_eq!(arm.scrutinee, ScrutineeKind::Ident);
    });
}

#[test]
fn non_enum_member_switch_stays_silent() {
    let source = "function f(o: any) {\n  switch (o.kind) {\n    case Math.PI:\n      break;\n    default:\n      break;\n  }\n}\n";
    with_harness(Lang::TypeScript, "a.ts", source, |analysis| {
        assert_eq!(analysis.facts.wildcard_arms.len(), 1);
        assert!(
            !analysis.facts.wildcard_arms[0].enum_like,
            "foreign member switch must not be enum-like"
        );
    });
}

#[test]
fn call_shapes_and_initializers() {
    let source = "const x = foo([1], () => {});\nfoo(1);\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", source, |analysis| {
            assert_eq!(analysis.facts.calls.len(), 2);
            let init = &analysis.facts.calls[0];
            assert!(init.initializes_binding, "{lang:?}: const initializer lost");
            assert!(
                init.is_simple_path,
                "{lang:?}: bare callee must be a simple path"
            );
            assert!(
                init.args
                    .iter()
                    .any(|a| matches!(a.shape, sloplint_core::facts::ArgShape::Collection)),
                "{lang:?}: array argument must be a Collection"
            );
            assert!(
                init.args
                    .iter()
                    .any(|a| matches!(a.shape, sloplint_core::facts::ArgShape::CodeBody)),
                "{lang:?}: closure argument must be a CodeBody"
            );
            assert!(
                !analysis.facts.calls[1].initializes_binding,
                "{lang:?}: bare call must not initialize a binding"
            );
        });
    }
}

#[test]
fn member_call_roots_and_paths() {
    let source = "a.b();\na[0]();\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", source, |analysis| {
            assert_eq!(analysis.facts.calls.len(), 2);
            assert!(analysis.facts.calls[0].is_simple_path);
            assert_eq!(analysis.facts.calls[0].callee_root, Some("a"));
            assert!(!analysis.facts.calls[1].is_simple_path);
        });
    }
}

#[test]
fn ts_params_and_overloads() {
    let source = "function f(flag: boolean, ...rest: string[]): void;\nfunction f(flag: boolean, ...rest: string[]) {}\n";
    with_harness(Lang::TypeScript, "a.ts", source, |analysis| {
        assert_eq!(analysis.facts.functions.len(), 2);
        let overload = analysis
            .facts
            .functions
            .iter()
            .find(|f| f.is_overload)
            .expect("overload signature must be marked");
        assert!(
            overload
                .params
                .iter()
                .any(|p| p.primitive_kind == sloplint_core::facts::PrimitiveKind::Bool),
            "boolean param kind lost: {:?}",
            overload.params
        );
        assert!(
            overload.params.iter().any(|p| p.is_variadic),
            "rest param must be variadic: {:?}",
            overload.params
        );
        assert!(
            analysis.facts.functions.iter().any(|f| !f.is_overload),
            "implementation must not be marked overload"
        );
    });
}

#[test]
fn const_local_shadows_top_let_write() {
    let plain = "let g = 0;\nfunction f() {\n  g = 2;\n}\n";
    let shadowed = "let g = 0;\nfunction f() {\n  const g = 1;\n  g = 2;\n}\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", plain, |analysis| {
            assert!(
                analysis.facts.ambient.iter().any(|a| {
                    a.kind == AmbientKind::GlobalWrite && a.name == "g" && a.in_function.is_some()
                }),
                "{lang:?}: assignment to file-scope let stays a GlobalWrite"
            );
        });
        with_harness(lang, "a.ts", shadowed, |analysis| {
            assert!(
                !analysis.facts.ambient.iter().any(|a| {
                    a.kind == AmbientKind::GlobalWrite && a.name == "g" && a.in_function.is_some()
                }),
                "{lang:?}: a write to a shadowing const local is no GlobalWrite"
            );
        });
    }
}

#[test]
fn destructured_param_shadows_top_let_write() {
    let plain = "let a = 0;\nfunction f() {\n  a = 1;\n}\n";
    let shadowed = "let a = 0;\nfunction f({a}) {\n  a = 1;\n}\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", plain, |analysis| {
            assert!(
                analysis.facts.ambient.iter().any(|a| {
                    a.kind == AmbientKind::GlobalWrite && a.name == "a" && a.in_function.is_some()
                }),
                "{lang:?}: assignment to file-scope let stays a GlobalWrite"
            );
        });
        with_harness(lang, "a.ts", shadowed, |analysis| {
            assert!(
                !analysis.facts.ambient.iter().any(|a| {
                    a.kind == AmbientKind::GlobalWrite && a.name == "a" && a.in_function.is_some()
                }),
                "{lang:?}: a write to a destructured shadowing param is no GlobalWrite"
            );
        });
    }
}

fn in_function_writes_to(analysis: &Analysis<'_>, name: &str) -> usize {
    analysis
        .facts
        .ambient
        .iter()
        .filter(|a| a.kind == AmbientKind::GlobalWrite && a.name == name && a.in_function.is_some())
        .count()
}

#[test]
fn for_in_let_shadows_top_let_write() {
    let bare =
        "let counter = 0;\nfunction f(xs) {\n  for (counter of xs) {\n    counter = 1;\n  }\n}\n";
    let shadowed = "let counter = 0;\nfunction f(xs) {\n  for (let counter of xs) {\n    counter = 1;\n  }\n}\n";
    let hoisted =
        "let counter = 0;\nfunction f(xs) {\n  counter = 1;\n  for (var counter of xs) {\n  }\n}\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", bare, |analysis| {
            assert_eq!(
                in_function_writes_to(&analysis, "counter"),
                1,
                "{lang:?}: a bare for-in write to file-scope let stays a GlobalWrite"
            );
        });
        with_harness(lang, "a.ts", shadowed, |analysis| {
            assert_eq!(
                in_function_writes_to(&analysis, "counter"),
                0,
                "{lang:?}: a write to a for-in let binding is no GlobalWrite"
            );
        });
        with_harness(lang, "a.ts", hoisted, |analysis| {
            assert_eq!(
                in_function_writes_to(&analysis, "counter"),
                0,
                "{lang:?}: a write before a hoisted for-in var is no GlobalWrite"
            );
        });
    }
}

#[test]
fn rest_param_shadows_top_let_write() {
    let plain = "let counter = 0;\nfunction f() {\n  counter = 1;\n}\n";
    let array_rest = "let counter = 0;\nfunction f(...[counter]) {\n  counter = 1;\n}\n";
    let object_plain = "let counter = 0;\nfunction f({a}) {\n  counter = 1;\n}\n";
    let object_rest = "let counter = 0;\nfunction f({a, ...counter}) {\n  counter = 1;\n}\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", plain, |analysis| {
            assert_eq!(
                in_function_writes_to(&analysis, "counter"),
                1,
                "{lang:?}: assignment to file-scope let stays a GlobalWrite"
            );
        });
        with_harness(lang, "a.ts", array_rest, |analysis| {
            assert_eq!(
                in_function_writes_to(&analysis, "counter"),
                0,
                "{lang:?}: a write to a rest array param is no GlobalWrite"
            );
        });
        with_harness(lang, "a.ts", object_plain, |analysis| {
            assert_eq!(
                in_function_writes_to(&analysis, "counter"),
                1,
                "{lang:?}: assignment to file-scope let stays a GlobalWrite"
            );
        });
        with_harness(lang, "a.ts", object_rest, |analysis| {
            assert_eq!(
                in_function_writes_to(&analysis, "counter"),
                0,
                "{lang:?}: a write to an object rest param is no GlobalWrite"
            );
        });
    }
}

#[test]
fn member_write_to_top_let_object_is_global_write() {
    let source = "let state = { n: 0 };\nfunction bump() {\n  state.n = 1;\n}\nfunction local() {\n  let state = { n: 0 };\n  state.n = 2;\n}\nfunction param(state) {\n  state.n = 3;\n}\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", source, |analysis| {
            let writes: Vec<_> = analysis
                .facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalWrite)
                .collect();
            assert_eq!(
                writes.len(),
                1,
                "{lang:?}: only the file-scope object write reports, got {:?}",
                writes
                    .iter()
                    .map(|w| (w.name, w.pos.line))
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                writes[0].name, "state",
                "{lang:?}: the write keeps the object name"
            );
            assert!(
                writes[0].in_function.is_some(),
                "{lang:?}: the object write sits in a function"
            );
        });
    }
}

#[test]
fn every_top_let_name_stays_writable() {
    let source = "let aaa = 0;\nlet bbb = 0;\nlet ccc = 0;\nlet ddd = 0;\nfunction f() {\n  aaa = 1;\n  bbb += 2;\n  ccc++;\n  ddd = 4;\n}\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", source, |analysis| {
            for name in ["aaa", "bbb", "ccc", "ddd"] {
                assert_eq!(
                    in_function_writes_to(&analysis, name),
                    1,
                    "{lang:?}: in-function write to {name} stays a GlobalWrite"
                );
            }
        });
    }
}

#[test]
fn write_before_var_decl_in_same_fn_is_local() {
    let source = "let counter = 0;\nfunction f() {\n  counter = 1;\n  var counter = 2;\n}\nfunction g() {\n  counter = 3;\n}\n";
    for lang in [Lang::JavaScript, Lang::TypeScript] {
        with_harness(lang, "a.ts", source, |analysis| {
            let writes: Vec<_> = analysis
                .facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalWrite)
                .collect();
            assert_eq!(
                writes.len(),
                1,
                "{lang:?}: only the unshadowed write reports, got {:?}",
                writes
                    .iter()
                    .map(|w| (w.name, w.pos.line))
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                writes[0].name, "counter",
                "{lang:?}: the write keeps the name"
            );
            assert_eq!(
                writes[0].pos.line, 7,
                "{lang:?}: the surviving write sits in g"
            );
            assert!(
                writes[0].in_function.is_some(),
                "{lang:?}: the surviving write sits in a function"
            );
        });
    }
}
