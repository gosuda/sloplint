use sloplint_core::facts::{ArgShape, Stmt};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

fn ml_facts<R>(source: &str, read: impl FnOnce(&sloplint_core::facts::FileFacts<'_>) -> R) -> R {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Ocaml,
        path: "lib.ml",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "lib.ml parses");
    read(&analysis.facts)
}

#[test]
fn closure_keeps_outer_params() {
    let params = ml_facts(
        "let make (verbose : bool) = fun x -> ignore (verbose, x)\nlet id (a : int) = a\n",
        |facts| {
            facts
                .functions
                .iter()
                .map(|f| {
                    (
                        f.name.to_owned(),
                        f.params
                            .iter()
                            .map(|p| (p.name.to_owned(), p.primitive_kind))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        },
    );
    let make = params
        .iter()
        .find(|(name, _)| name == "make")
        .expect("`make` is recorded");
    assert_eq!(
        make.1,
        vec![
            (
                "verbose".to_owned(),
                sloplint_core::facts::PrimitiveKind::Bool
            ),
            ("x".to_owned(), sloplint_core::facts::PrimitiveKind::Other),
        ],
        "closure keeps the outer parameter, got {params:?}"
    );
    let id = params
        .iter()
        .find(|(name, _)| name == "id")
        .expect("`id` is recorded");
    assert_eq!(
        id.1,
        vec![("a".to_owned(), sloplint_core::facts::PrimitiveKind::Int)],
        "plain typed parameter is unchanged, got {params:?}"
    );
}

#[test]
fn function_expression_keeps_outer_params() {
    let found = ml_facts(
        "let set_mode (flag : bool) = function true -> 1 | false -> 0\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "set_mode")
                .map(|f| {
                    (
                        f.params
                            .iter()
                            .map(|p| (p.name.to_owned(), p.primitive_kind))
                            .collect::<Vec<_>>(),
                        f.sole_call.is_none(),
                    )
                })
                .expect("`set_mode` is recorded")
        },
    );
    assert_eq!(
        found.0,
        vec![("flag".to_owned(), sloplint_core::facts::PrimitiveKind::Bool)],
        "function cases keep the outer bool parameter, got {found:?}"
    );
    assert!(found.1, "multi-arm function has no sole call");
}

#[test]
fn incr_records_global_write() {
    let writes = ml_facts(
        "let counter = ref 0\nlet bump () = incr counter\nlet touch () = ignore counter\n",
        |facts| {
            facts
                .ambient
                .iter()
                .filter(|a| {
                    a.kind == sloplint_core::facts::AmbientKind::GlobalWrite && a.name == "counter"
                })
                .map(|a| a.pos.line)
                .collect::<Vec<_>>()
        },
    );
    assert_eq!(
        writes,
        vec![2],
        "incr on a file-scope ref writes at its line, got {writes:?}"
    );
}

#[test]
fn begin_end_handler_body_flattens() {
    ml_facts(
        "let go () = try run () with e -> begin Printf.eprintf \"a\"; raise e end\nlet go2 () = try run () with E -> raise E\n",
        |facts| {
            let handlers: Vec<_> = facts.handlers.iter().map(|h| h.stmts.clone()).collect();
            assert_eq!(
                handlers.len(),
                2,
                "both handlers are recorded, got {handlers:?}"
            );
            assert!(
                matches!(handlers[0].as_slice(), [Stmt::Call { .. }, Stmt::Rethrow]),
                "begin block flattens to call plus rethrow, got {handlers:?}"
            );
            assert!(
                matches!(handlers[1].as_slice(), [Stmt::Rethrow]),
                "bare rethrow stays one statement, got {handlers:?}"
            );
        },
    );
}

#[test]
fn cleanup_helper_handler_is_marked() {
    let flags = ml_facts(
        "let close_fd fd = try Unix.close fd with _ -> ()\nlet load k = try read_key k with Not_found -> prerr_endline \"x\"\n",
        |facts| {
            facts
                .handlers
                .iter()
                .map(|h| (h.in_cleanup, h.stmts.is_empty()))
                .collect::<Vec<_>>()
        },
    );
    assert_eq!(
        flags,
        vec![(true, true), (false, false)],
        "cleanup helper handler is marked, loader is not, got {flags:?}"
    );
}

#[test]
fn record_copies_report_their_source() {
    let copies = ml_facts(
        "let convert (src : point) : point =\n  { a = src.a; b = src.b; c = src.c; d = src.d }\nlet lit = { a = 1; b = 2 }\n",
        |facts| {
            facts
                .field_copies
                .iter()
                .map(|c| (c.fields, c.source.map(str::to_owned)))
                .collect::<Vec<_>>()
        },
    );
    assert_eq!(
        copies,
        vec![(4, Some("src".to_owned())), (2, None)],
        "field copies keep their source, literals do not, got {copies:?}"
    );
}

#[test]
fn parameter_shadows_global_write() {
    let writes = ml_facts(
        "let total = ref 0\nlet set total v = total := v\nlet bump () = total := 1\n",
        |facts| {
            facts
                .ambient
                .iter()
                .filter(|a| {
                    a.kind == sloplint_core::facts::AmbientKind::GlobalWrite && a.name == "total"
                })
                .map(|a| a.pos.line)
                .collect::<Vec<_>>()
        },
    );
    assert_eq!(
        writes,
        vec![3],
        "parameter shadows the global name, plain write still reports, got {writes:?}"
    );
}

#[test]
fn match_argument_is_code_body() {
    ml_facts(
        "let _ = compute (match x with A -> 1 | _ -> 0)\nlet _ = compute [1; 2]\n",
        |facts| {
            let shapes: Vec<_> = facts
                .calls
                .iter()
                .map(|c| c.args.first().map(|a| a.shape.clone()))
                .collect();
            assert_eq!(
                shapes,
                vec![Some(ArgShape::CodeBody), Some(ArgShape::Collection)],
                "match argument is code, list stays a collection, got {shapes:?}"
            );
        },
    );
}
