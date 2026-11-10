//! Odin cross-adapter contract facts: qualified callees, defaults, entry,
//! visibility, imports, assertions, initializers, arg shapes, switch arms.
use sloplint_core::facts::{ArgShape, AssertShape, PrimitiveKind, Visibility};
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
fn qualified_callee_is_simple_path_receiver_call_is_not() {
    let qualified = with_facts(
        "greet.odin",
        "package p\ngreet :: proc(n: string) -> string {\n\treturn render.greet(n)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee.contains("greet"))
                .map(|c| {
                    (
                        c.callee.to_owned(),
                        c.callee_root.map(str::to_owned),
                        c.is_simple_path,
                    )
                })
                .expect("the qualified call is recorded")
        },
    );
    assert_eq!(
        qualified,
        (
            String::from("render.greet"),
            Some(String::from("render")),
            true
        ),
        "pkg.f keeps a simple path with its root, got {qualified:?}"
    );
    let receiver = with_facts(
        "label.odin",
        "package p\nlabel :: proc(ctx: ^Ctx) {\n\tget_logger(ctx).info(\"start\")\n}\n",
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
    let implicit = with_facts(
        "build.odin",
        "package p\nbuild :: proc(n: int) -> Thing {\n\treturn .Build(n)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "Build")
                .map(|c| {
                    (
                        c.callee.to_owned(),
                        c.callee_root.map(str::to_owned),
                        c.is_simple_path,
                    )
                })
                .expect("the implicit call is recorded")
        },
    );
    assert_eq!(
        implicit,
        (String::from("Build"), None, false),
        "an operand-less selector has no root and no simple path, got {implicit:?}"
    );
    let deep = with_facts(
        "deep.odin",
        "package p\nrun :: proc() {\n\ta.b.c.d(n)\n}\n",
        |facts| {
            facts
                .calls
                .first()
                .map(|c| {
                    (
                        c.callee.to_owned(),
                        c.callee_root.map(str::to_owned),
                        c.is_simple_path,
                    )
                })
                .expect("the deep call is recorded")
        },
    );
    assert_eq!(
        deep,
        (String::from("a.b.c.d"), Some(String::from("a")), true),
        "a four-link chain stays a simple path from its root, got {deep:?}"
    );
}

#[test]
fn index_and_call_roots_stay_none() {
    let indexed = with_facts(
        "run.odin",
        "package p\nrun :: proc(i: int) {\n\thandlers[i](cb)\n}\n",
        |facts| {
            facts
                .calls
                .first()
                .map(|c| (c.callee.to_owned(), c.callee_root.map(str::to_owned)))
                .expect("the indexed call is recorded")
        },
    );
    assert_eq!(
        indexed.1, None,
        "an index at the chain bottom gives no root, got {indexed:?}"
    );
    assert!(
        !indexed.0.contains('(') && !indexed.0.contains('['),
        "an index callee carries no argument text, got {indexed:?}"
    );
    let nested = with_facts(
        "go.odin",
        "package p\ngo :: proc() {\n\tget_handler()(x)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "get_handler" && !c.is_simple_path)
                .map(|c| (c.callee.to_owned(), c.callee_root.map(str::to_owned)))
                .expect("the nested call is recorded")
        },
    );
    assert_eq!(
        nested.1, None,
        "a call at the chain bottom gives no root, got {nested:?}"
    );
}

#[test]
fn typed_default_param_carries_its_default() {
    let params = with_facts(
        "fill.odin",
        "package p\nfill :: proc(a: string, b: string, c: int = 3) {\n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "fill")
                .map(|f| {
                    f.params
                        .iter()
                        .map(|p| (p.name.to_owned(), p.has_default, p.bool_default))
                        .collect::<Vec<_>>()
                })
                .expect("`fill` is recorded")
        },
    );
    assert_eq!(
        params,
        vec![
            (String::from("a"), false, false),
            (String::from("b"), false, false),
            (String::from("c"), true, false),
        ],
        "`c: int = 3` carries a non-bool default, got {params:?}"
    );
}

#[test]
fn string_and_cstring_map_to_str() {
    let kinds = with_facts(
        "pair.odin",
        "package p\npair :: proc(a: string, b: cstring, c: u64, d: bool) {\n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "pair")
                .map(|f| {
                    f.params
                        .iter()
                        .map(|p| p.primitive_kind)
                        .collect::<Vec<_>>()
                })
                .expect("`pair` is recorded")
        },
    );
    assert_eq!(
        kinds,
        vec![
            PrimitiveKind::Str,
            PrimitiveKind::Str,
            PrimitiveKind::Other,
            PrimitiveKind::Bool,
        ],
        "string and cstring are Str, u64 stays Other, bool stays Bool, got {kinds:?}"
    );
}

#[test]
fn main_in_package_main_is_entry_elsewhere_it_is_not() {
    let entry = with_facts(
        "main.odin",
        "package main\nmain :: proc() {\n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "main")
                .map(|f| (f.entry_point, f.visibility))
                .expect("`main` is recorded")
        },
    );
    assert_eq!(
        entry,
        (true, Visibility::Exported),
        "package-main `main` is the entry point, got {entry:?}"
    );
    let other = with_facts("tool.odin", "package p\nmain :: proc() {\n}\n", |facts| {
        facts
            .functions
            .iter()
            .find(|f| f.name == "main")
            .map(|f| f.entry_point)
            .expect("`main` is recorded")
    });
    assert!(!other, "a `main` outside package main is no entry point");
}

#[test]
fn when_and_foreign_blocks_keep_top_level_visibility() {
    let helper = with_facts(
        "when.odin",
        "package p\nwhen ODIN_OS == .Windows {\n\thelper :: proc() {\n\t}\n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "helper")
                .map(|f| (f.visibility, f.signature_forced))
                .expect("`helper` is recorded")
        },
    );
    assert_eq!(
        helper,
        (Visibility::Exported, false),
        "a when-block proc stays exported without a forced signature, got {helper:?}"
    );
    let foreign = with_facts(
        "sqrt.odin",
        "package p\nforeign libc {\n\tsqrt_c :: proc(x: f64) -> f64 --- \n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "sqrt_c")
                .map(|f| (f.visibility, f.signature_forced))
                .expect("`sqrt_c` is recorded")
        },
    );
    assert_eq!(
        foreign,
        (Visibility::Exported, true),
        "a foreign stub is exported with an ABI-fixed signature, got {foreign:?}"
    );
}

#[test]
fn export_attribute_forces_signature_plain_proc_does_not() {
    let forced = with_facts(
        "plugin.odin",
        "package p\n@(export)\nplugin_init :: proc(a: cstring) {\n}\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "plugin_init")
                .map(|f| (f.signature_forced, f.visibility))
                .expect("`plugin_init` is recorded")
        },
    );
    assert_eq!(
        forced,
        (true, Visibility::Exported),
        "`@(export)` fixes the signature, got {forced:?}"
    );
    let plain = with_facts("show.odin", "package p\nshow :: proc() {\n}\n", |facts| {
        facts
            .functions
            .iter()
            .find(|f| f.name == "show")
            .map(|f| f.signature_forced)
            .expect("`show` is recorded")
    });
    assert!(!plain, "a plain proc keeps an open signature");
}

#[test]
fn vendor_collection_has_root_core_and_relative_do_not() {
    let roots = with_facts(
        "use.odin",
        "package p\nimport rl \"vendor:raylib\"\nimport rlgl \"vendor:raylib/rlgl\"\nimport \"core:fmt\"\nimport \"./local\"\n",
        |facts| {
            facts
                .imports
                .iter()
                .map(|i| (i.module.to_string(), i.external_root.map(str::to_owned)))
                .collect::<Vec<_>>()
        },
    );
    assert_eq!(
        roots,
        vec![
            (
                String::from("vendor:raylib"),
                Some(String::from("vendor:raylib"))
            ),
            (
                String::from("vendor:raylib/rlgl"),
                Some(String::from("vendor:raylib"))
            ),
            (String::from("core:fmt"), None),
            (String::from("./local"), None),
        ],
        "one vendor package keeps one root, core and relative keep none, got {roots:?}"
    );
    let linker = with_facts(
        "foreign.odin",
        "package p\nforeign import libc \"system:c\"\n",
        |facts| {
            facts
                .imports
                .iter()
                .map(|i| (i.kind, i.external_root.map(str::to_owned)))
                .collect::<Vec<_>>()
        },
    );
    assert_eq!(
        linker,
        vec![(sloplint_core::facts::ImportKind::SideEffect, None)],
        "a foreign linker import names no package, got {linker:?}"
    );
}

#[test]
fn formatted_assert_shares_the_plain_condition_shape() {
    let shaped = with_facts(
        "check.odin",
        "package p\ncheck :: proc(x: int, y: int) {\n\tfmt.assertf(x == y, \"bad\")\n}\n",
        |facts| {
            facts
                .assertions
                .first()
                .map(|a| (a.comparison, a.shape, a.has_message))
                .expect("the formatted assert is recorded")
        },
    );
    assert_eq!(
        shaped,
        (true, AssertShape::TopEqNe, true),
        "formatted assert over == reports TopEqNe with a message, got {shaped:?}"
    );
    let bare = with_facts(
        "ok.odin",
        "package p\nok :: proc(ready: bool) {\n\tfmt.assertf(ready, \"bad\")\n}\n",
        |facts| {
            facts
                .assertions
                .first()
                .map(|a| (a.comparison, a.shape, a.has_message))
                .expect("the bare formatted assert is recorded")
        },
    );
    assert_eq!(
        bare,
        (false, AssertShape::BareName, true),
        "formatted assert over a name stays BareName, got {bare:?}"
    );
}

#[test]
fn short_declaration_initializer_sets_binding_plain_assign_does_not() {
    let init = with_facts(
        "table.odin",
        "package p\nrun :: proc() {\n\ttable := make_table(1)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "make_table")
                .map(|c| c.initializes_binding)
                .expect("the initializer call is recorded")
        },
    );
    assert!(init, "`table := make_table(1)` initializes the binding");
    let blank = with_facts(
        "blank.odin",
        "package p\nrun :: proc() {\n\t_ := make_table(1)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "make_table")
                .map(|c| c.initializes_binding)
                .expect("the blank call is recorded")
        },
    );
    assert!(!blank, "`_` binds nothing, so no initializer is reported");
}

#[test]
fn composite_literal_is_collection_proc_literal_is_codebody() {
    let collection = with_facts(
        "compute.odin",
        "package p\nrun :: proc() {\n\tcompute([]int{1, 2, 3})\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "compute")
                .map(|c| c.args.iter().any(|a| a.shape == ArgShape::Collection))
                .expect("`compute` is recorded")
        },
    );
    assert!(
        collection,
        "the composite literal is a Collection, got {collection:?}"
    );
    let body = with_facts(
        "defer.odin",
        "package p\nrun :: proc() {\n\trun2(proc() {\n\t})\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "run2")
                .map(|c| c.args.iter().any(|a| a.shape == ArgShape::CodeBody))
                .expect("`run2` is recorded")
        },
    );
    assert!(body, "the proc literal is a CodeBody, got {body:?}");
}

#[test]
fn multi_name_arm_counts_each_variant_single_name_arm_does_not() {
    let two = with_facts(
        "two.odin",
        "package p\nrun :: proc(s: int) {\n\tswitch s {\n\tcase .Red, .Green:\n\t\tf()\n\tcase:\n\t\tg()\n\t}\n}\n",
        |facts| {
            facts
                .wildcard_arms
                .first()
                .map(|w| (w.variant_count, w.enum_like))
                .expect("the wildcard arm is recorded")
        },
    );
    assert_eq!(
        two,
        (2, true),
        "one arm naming two variants counts two, got {two:?}"
    );
    let one = with_facts(
        "one.odin",
        "package p\nrun :: proc(s: int) {\n\tswitch s {\n\tcase .Red:\n\t\tf()\n\tcase:\n\t\tg()\n\t}\n}\n",
        |facts| {
            facts
                .wildcard_arms
                .first()
                .map(|w| (w.variant_count, w.enum_like))
                .expect("the wildcard arm is recorded")
        },
    );
    assert_eq!(
        one,
        (1, false),
        "a single named variant leaves enum_like false, got {one:?}"
    );
}

#[test]
fn string_concat_carries_longest_static_run() {
    let long = with_facts(
        "long.odin",
        "package p\nrun :: proc(name: string) {\n\tlog(\"hello world\" + name)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "log")
                .map(|c| {
                    c.args.iter().find_map(|a| match a.shape {
                        ArgShape::Concat { longest_static } => Some(longest_static),
                        _ => None,
                    })
                })
                .expect("`log` is recorded")
        },
    );
    assert_eq!(
        long,
        Some(11),
        "eleven static chars outrun the strict floor, got {long:?}"
    );
    let short = with_facts(
        "short.odin",
        "package p\nrun :: proc(name: string) {\n\tlog(\"hi \" + name)\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "log")
                .map(|c| {
                    c.args.iter().find_map(|a| match a.shape {
                        ArgShape::Concat { longest_static } => Some(longest_static),
                        _ => None,
                    })
                })
                .expect("`log` is recorded")
        },
    );
    assert_eq!(
        short,
        Some(3),
        "three static chars stay under the strict floor, got {short:?}"
    );
    let dynamic = with_facts(
        "dynamic.odin",
        "package p\nrun :: proc(name: string) {\n\tlog(name + \"\")\n}\n",
        |facts| {
            facts
                .calls
                .iter()
                .find(|c| c.callee == "log")
                .map(|c| {
                    c.args.iter().find_map(|a| match a.shape {
                        ArgShape::Concat { longest_static } => Some(longest_static),
                        _ => None,
                    })
                })
                .expect("`log` is recorded")
        },
    );
    assert_eq!(
        dynamic,
        Some(0),
        "no static run measures zero, got {dynamic:?}"
    );
}
