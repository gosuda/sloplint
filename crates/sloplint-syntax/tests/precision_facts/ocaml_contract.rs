use sloplint_core::facts::{ArgShape, AssertShape, ScrutineeKind, Stmt};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn facts_in<R>(
    lang: Lang,
    path: &str,
    source: &str,
    read: impl FnOnce(&sloplint_core::facts::FileFacts<'_>) -> R,
) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "{path} parses");
    read(&analysis.facts)
}

fn ml_facts<R>(source: &str, read: impl FnOnce(&sloplint_core::facts::FileFacts<'_>) -> R) -> R {
    facts_in(Lang::Ocaml, "lib.ml", source, read)
}

#[test]
fn module_qualified_callee_is_simple_path() {
    let calls = ml_facts(
        "let greet n = Greeting.render n\nlet deref g x = !g x\n",
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
    assert_eq!(
        calls,
        vec![
            (
                "Greeting.render".to_owned(),
                Some("Greeting".to_owned()),
                true,
            ),
            ("g".to_owned(), None, false),
        ],
        "scoped callee is a simple path, parenthesized callee is not, got {calls:?}"
    );
    let bare = ml_facts("let plain f x = f x\n", |facts| {
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
    });
    assert_eq!(
        bare,
        vec![("f".to_owned(), Some("f".to_owned()), true)],
        "bare callee stays a simple path, got {bare:?}"
    );
}

#[test]
fn call_initializes_let_binding() {
    let inits = ml_facts("let out = show (render x)\n", |facts| {
        facts
            .calls
            .iter()
            .map(|c| (c.callee.to_owned(), c.initializes_binding))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        inits,
        vec![("show".to_owned(), true), ("render".to_owned(), false)],
        "only the whole initializer marks the binding, got {inits:?}"
    );
}

#[test]
fn collection_literals_are_collection_args() {
    ml_facts(
        "let a = f1 [1; 2]\nlet b = f2 [|1|]\nlet c = f3 {x = 1}\nlet d = f4 (1, 2)\nlet e = f5 \"s\"\nlet f = f6 [fun x -> x]\n",
        |facts| {
            let shapes: Vec<_> = facts
                .calls
                .iter()
                .map(|c| c.args.first().map(|a| a.shape.clone()))
                .collect();
            assert_eq!(
                shapes,
                vec![
                    Some(ArgShape::Collection),
                    Some(ArgShape::Collection),
                    Some(ArgShape::Other),
                    Some(ArgShape::Collection),
                    Some(ArgShape::Str { longest_static: 1 }),
                    Some(ArgShape::CodeBody),
                ],
                "list, array and tuple stay collections, record is other, closure is code, got {shapes:?}"
            );
        },
    );
}

#[test]
fn closure_argument_is_code_body() {
    ml_facts(
        "let _ = List.iter (fun x -> print_int x) xs\nlet _ = List.iter render xs\n",
        |facts| {
            let shapes: Vec<_> = facts
                .calls
                .iter()
                .filter(|c| c.callee == "List.iter")
                .map(|c| c.args.first().map(|a| a.shape.clone()))
                .collect();
            assert!(
                matches!(shapes[0], Some(ArgShape::CodeBody)),
                "parenthesized closure is a code body, got {shapes:?}"
            );
            assert!(
                matches!(shapes[1], Some(ArgShape::Ident("render"))),
                "bare function value stays an ident, got {shapes:?}"
            );
        },
    );
}

#[test]
fn nested_concat_carries_static_length() {
    ml_facts(
        "let _ = prerr_endline (msg ^ \" connection to the host failed\")\nlet _ = prerr_endline (x ^ \"!\")\nlet _ = prerr_endline (a ^ b)\n",
        |facts| {
            let shapes: Vec<_> = facts
                .calls
                .iter()
                .map(|c| c.args.first().map(|a| a.shape.clone()))
                .collect();
            assert_eq!(
                shapes,
                vec![
                    Some(ArgShape::Concat { longest_static: 30 }),
                    Some(ArgShape::Concat { longest_static: 1 }),
                    Some(ArgShape::Concat { longest_static: 0 }),
                ],
                "long, short and dynamic concats carry 30, 1 and 0, got {shapes:?}"
            );
        },
    );
}

#[test]
fn unconstrained_module_functions_are_free() {
    let methods = ml_facts(
        "module Util = struct\n  let parse_flag x = x = \"1\"\nend\nlet top x = x\n",
        |facts| {
            facts
                .functions
                .iter()
                .map(|f| (f.name.to_owned(), f.is_method))
                .collect::<Vec<_>>()
        },
    );
    assert!(
        methods.contains(&("parse_flag".to_owned(), false)),
        "module function is not a method, got {methods:?}"
    );
    assert!(
        methods.contains(&("top".to_owned(), false)),
        "top-level function stays free, got {methods:?}"
    );
}

#[test]
fn attributes_mark_deprecated_and_decorated() {
    let flags = ml_facts(
        "let parse_v1 x = parse x [@@deprecated \"use parse\"]\nlet greet n = render n [@@inline]\n",
        |facts| {
            facts
                .functions
                .iter()
                .map(|f| (f.name.to_owned(), f.is_deprecated, f.is_decorated))
                .collect::<Vec<_>>()
        },
    );
    assert!(
        flags.contains(&("parse_v1".to_owned(), true, true)),
        "deprecated function is marked, got {flags:?}"
    );
    assert!(
        flags.contains(&("greet".to_owned(), false, true)),
        "decorated function is not deprecated, got {flags:?}"
    );
}

#[test]
fn module_type_vals_are_trait_items() {
    let items = ml_facts(
        "module type Store = sig\n  val get : string -> string\nend\nlet free x = x\n",
        |facts| {
            facts
                .functions
                .iter()
                .map(|f| (f.name.to_owned(), f.is_trait_item))
                .collect::<Vec<_>>()
        },
    );
    assert!(
        items.contains(&("get".to_owned(), true)),
        "val in a module type is a trait item, got {items:?}"
    );
    assert!(
        items.contains(&("free".to_owned(), false)),
        "plain function is not a trait item, got {items:?}"
    );
}

#[test]
fn spec_signature_normalizes_labeled_domains() {
    let params = facts_in(
        Lang::OcamlInterface,
        "a.mli",
        "val set_mode : bool -> string -> unit\nval set_verbose : verbose:bool -> unit\nval make : foo -> unit\n",
        |facts| {
            facts
                .functions
                .iter()
                .map(|f| {
                    (
                        f.name.to_owned(),
                        f.params
                            .iter()
                            .map(|p| (p.name.to_owned(), p.ty.map(str::to_owned), p.primitive_kind))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        },
    );
    let mode = params
        .iter()
        .find(|(name, _)| *name == "set_mode")
        .expect("set_mode is recorded");
    assert_eq!(
        mode.1,
        vec![
            (
                "".to_owned(),
                Some("bool".to_owned()),
                sloplint_core::facts::PrimitiveKind::Bool
            ),
            (
                "".to_owned(),
                Some("string".to_owned()),
                sloplint_core::facts::PrimitiveKind::Str
            ),
        ],
        "bool and string domains normalize, got {params:?}"
    );
    let verbose = params
        .iter()
        .find(|(name, _)| *name == "set_verbose")
        .expect("set_verbose is recorded");
    assert_eq!(
        verbose.1,
        vec![(
            "verbose".to_owned(),
            Some("bool".to_owned()),
            sloplint_core::facts::PrimitiveKind::Bool
        ),],
        "label leaves the type text and kind, got {params:?}"
    );
    let make = params
        .iter()
        .find(|(name, _)| *name == "make")
        .expect("make is recorded");
    assert_eq!(
        make.1,
        vec![(
            "".to_owned(),
            Some("foo".to_owned()),
            sloplint_core::facts::PrimitiveKind::Other
        )],
        "unknown domain stays other, got {params:?}"
    );
}

#[test]
fn or_pattern_siblings_count_as_enum() {
    let enums = ml_facts(
        "let pick k = match k with A | B -> 1 | _ -> 0\nlet wrap f x = try f x with Not_found -> 0 | _ -> 1\nlet tag k = match k with `A | `B -> 1 | _ -> 0\n",
        |facts| {
            facts
                .wildcard_arms
                .iter()
                .map(|a| a.enum_like)
                .collect::<Vec<_>>()
        },
    );
    assert_eq!(
        enums,
        vec![true, false, true],
        "or-pattern pairs are enum-like, single exception is not, got {enums:?}"
    );
}

#[test]
fn nullable_sibling_marks_match_arm() {
    let arm = ml_facts("let get o = match o with Some v -> v | _ -> 0\n", |facts| {
        facts
            .wildcard_arms
            .first()
            .map(|a| {
                (
                    a.enum_like,
                    a.scrutinee,
                    a.variant_count,
                    a.has_nullable_ctor,
                )
            })
            .expect("the wildcard arm is recorded")
    });
    assert_eq!(
        arm,
        (false, ScrutineeKind::Ident, 1, true),
        "option wildcard keeps ident scrutinee with nullable ctor, got {arm:?}"
    );
}

#[test]
fn unit_handler_body_is_empty() {
    ml_facts(
        "let remove p = try Sys.remove p with _ -> ()\nlet loud k = try read_key k with E -> prerr_endline \"x\"\n",
        |facts| {
            let handlers: Vec<_> = facts.handlers.iter().map(|h| h.stmts.clone()).collect();
            assert_eq!(
                handlers.len(),
                2,
                "both handlers are recorded, got {handlers:?}"
            );
            assert!(
                handlers[0].is_empty(),
                "unit body swallows to no statements, got {handlers:?}"
            );
            assert!(
                matches!(handlers[1].as_slice(), [Stmt::Call { .. }]),
                "single log call stays one call, got {handlers:?}"
            );
        },
    );
}

#[test]
fn incr_decr_record_writes() {
    let bindings = ml_facts(
        "let bump xs =\n  let total = ref 0 in\n  List.iter (fun x -> incr total) xs;\n  List.iter (fun x -> decr total) xs;\n  !total\n",
        |facts| {
            facts
                .bindings
                .iter()
                .map(|b| {
                    (
                        b.name.to_owned(),
                        b.first_write_line,
                        b.last_write_line,
                        b.write_lines,
                    )
                })
                .collect::<Vec<_>>()
        },
    );
    assert_eq!(
        bindings,
        vec![("total".to_owned(), 3, 4, 2)],
        "incr and decr extend the local span, got {bindings:?}"
    );
}

#[test]
fn sequence_handler_body_flattens() {
    ml_facts(
        "let go () = try run () with e -> Printf.eprintf \"a\"; Printf.eprintf \"b\"; raise e\n",
        |facts| {
            let handlers: Vec<_> = facts.handlers.iter().map(|h| h.stmts.clone()).collect();
            assert_eq!(
                handlers.len(),
                1,
                "the handler is recorded, got {handlers:?}"
            );
            assert!(
                matches!(
                    handlers[0].as_slice(),
                    [Stmt::Call { .. }, Stmt::Call { .. }, Stmt::Rethrow]
                ),
                "three-part sequence flattens to two calls plus rethrow, got {handlers:?}"
            );
        },
    );
}

#[test]
fn field_selection_callee_is_simple_path() {
    let calls = ml_facts("let wrap n = store.render n\n", |facts| {
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
    });
    assert_eq!(
        calls,
        vec![("store.render".to_owned(), Some("store".to_owned()), true)],
        "field selection is a simple path from its record, got {calls:?}"
    );
}

#[test]
fn function_body_does_not_initialize_binding() {
    let inits = ml_facts(
        "let rows = build_rows [1; 2]\nlet build cfg = make_table [1; 2]\n",
        |facts| {
            facts
                .calls
                .iter()
                .map(|c| (c.callee.to_owned(), c.initializes_binding))
                .collect::<Vec<_>>()
        },
    );
    assert_eq!(
        inits,
        vec![
            ("build_rows".to_owned(), true),
            ("make_table".to_owned(), false)
        ],
        "only a val initializer marks the binding, got {inits:?}"
    );
}

#[test]
fn assert_shape_distinguishes_equality() {
    let shape = ml_facts("let check x y = assert (x = y)\n", |facts| {
        facts
            .assertions
            .first()
            .map(|a| a.shape)
            .expect("the assert is recorded")
    });
    assert_eq!(
        shape,
        AssertShape::TopEqNe,
        "equality assert is top-level equality, got {shape:?}"
    );
}

#[test]
fn punned_labeled_argument_is_ident() {
    ml_facts(
        "let wrap ~x = render ~x\nlet echo v = render v\n",
        |facts| {
            let shapes: Vec<_> = facts
                .calls
                .iter()
                .map(|c| c.args.first().map(|a| a.shape.clone()))
                .collect();
            assert_eq!(
                shapes,
                vec![Some(ArgShape::Ident("x")), Some(ArgShape::Ident("v"))],
                "punned label forwards the name, got {shapes:?}"
            );
        },
    );
}
