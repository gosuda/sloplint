use sloplint_core::facts::{AmbientKind, ImportKind, ImportScope, ScrutineeKind, Visibility};
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
        lang: Lang::Ocaml,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "{path} parses");
    read(&analysis.facts)
}

#[test]
fn top_level_function_is_exported_with_default_head_facts() {
    let found = with_facts("lib.ml", "let top x = x\n", |facts| {
        facts
            .functions
            .iter()
            .find(|f| f.name == "top")
            .map(|f| {
                (
                    f.visibility,
                    f.signature_forced,
                    f.entry_point,
                    f.in_test,
                    f.is_overload,
                    f.is_deprecated,
                    f.is_decorated,
                    f.is_trait_item,
                )
            })
            .expect("`top` is recorded")
    });
    assert_eq!(
        found,
        (
            Visibility::Exported,
            false,
            false,
            false,
            false,
            false,
            false,
            false
        ),
        "top-level `top` is exported with every new head fact at default, got {found:?}"
    );
}

#[test]
fn nested_function_is_private() {
    let visibility = with_facts(
        "lib.ml",
        "let outer x = let inner y = y in inner x\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "inner")
                .map(|f| f.visibility)
                .expect("`inner` is recorded")
        },
    );
    assert_eq!(
        visibility,
        Visibility::Private,
        "nested `inner` is private, got {visibility:?}"
    );
}

#[test]
fn signature_constrained_module_forces_signature() {
    let forced = with_facts(
        "lib.ml",
        "module M : sig val f : int -> int end = struct\n  let f x = x\nend\n",
        |facts| {
            facts
                .functions
                .iter()
                .rfind(|f| f.name == "f")
                .map(|f| f.signature_forced)
                .expect("`f` is recorded")
        },
    );
    assert!(forced, "constrained `f` keeps a forced signature");
    let free = with_facts(
        "lib.ml",
        "module N = struct\n  let g x = x\nend\n",
        |facts| {
            facts
                .functions
                .iter()
                .find(|f| f.name == "g")
                .map(|f| f.signature_forced)
                .expect("`g` is recorded")
        },
    );
    assert!(!free, "unconstrained `g` keeps a free signature");
}

#[test]
fn wildcard_beside_constructors_reports_enum_like_with_default_counts() {
    let arm = with_facts(
        "lib.ml",
        "type t = A of int | B of string\nlet pick x = match x with A a -> 1 | B b -> 2 | _ -> 0\n",
        |facts| {
            facts
                .wildcard_arms
                .first()
                .map(|arm| {
                    (
                        arm.enum_like,
                        arm.scrutinee,
                        arm.variant_count,
                        arm.has_nullable_ctor,
                    )
                })
                .expect("the `_` arm is recorded")
        },
    );
    assert_eq!(
        arm,
        (true, ScrutineeKind::Ident, 2, false),
        "the `_` arm beside constructors is enum-like with ident scrutinee, got {arm:?}"
    );
    let plain = with_facts(
        "lib.ml",
        "let mood b = match b with true -> 1 | _ -> 0\n",
        |facts| {
            facts
                .wildcard_arms
                .first()
                .map(|arm| arm.enum_like)
                .expect("the `_` arm is recorded")
        },
    );
    assert!(!plain, "the `_` arm beside a literal is not enum-like");
}

#[test]
fn assert_comparison_reports_top_eq_ne() {
    let compared = with_facts("lib.ml", "let check x y = assert (x = y)\n", |facts| {
        facts
            .assertions
            .first()
            .map(|a| (a.comparison, a.shape, a.has_message))
            .expect("the assert is recorded")
    });
    assert_eq!(
        compared,
        (true, sloplint_core::facts::AssertShape::TopEqNe, false),
        "relational assert keeps comparison with equality shape, got {compared:?}"
    );
    let bare = with_facts("lib.ml", "let check_plain x = assert x\n", |facts| {
        facts
            .assertions
            .first()
            .map(|a| a.comparison)
            .expect("the assert is recorded")
    });
    assert!(!bare, "bare assert reports no comparison");
}

#[test]
fn open_import_keeps_module_scope_and_no_external_root() {
    let imports = with_facts(
        "lib.ml",
        "open Printf\ninclude Set\nopen Yojson.Safe\nmodule J = Yojson.Basic\nlet id x = x\n",
        |facts| {
            facts
                .imports
                .iter()
                .map(|i| {
                    (
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
        4,
        "all four module directives are recorded, got {imports:?}"
    );
    assert_eq!(
        imports[0],
        (
            ImportKind::Wildcard,
            ImportScope::Module,
            false,
            false,
            None
        ),
        "open stays module-scoped with no external root, got {imports:?}"
    );
    assert_eq!(
        imports[1],
        (ImportKind::Wildcard, ImportScope::Module, false, true, None),
        "include reexports with no external root, got {imports:?}"
    );
    assert_eq!(
        imports[2],
        (
            ImportKind::Wildcard,
            ImportScope::Module,
            false,
            false,
            Some("Yojson".to_owned())
        ),
        "third-party open keeps its root, got {imports:?}"
    );
    assert_eq!(
        imports[3],
        (
            ImportKind::Namespace,
            ImportScope::Module,
            false,
            false,
            Some("Yojson".to_owned())
        ),
        "third-party alias keeps its root, got {imports:?}"
    );
}

#[test]
fn and_chain_declares_every_ref_binding() {
    let mutables = with_facts(
        "lib.ml",
        "let a = ref 0 and b = ref 0\nlet f x = a := x\n",
        |facts| {
            facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalMutable)
                .map(|a| a.name.to_owned())
                .collect::<Vec<_>>()
        },
    );
    assert!(
        mutables.contains(&String::from("a")),
        "first and-branch stays declared, got {mutables:?}"
    );
    assert!(
        mutables.contains(&String::from("b")),
        "later and-branch joins the declaration, got {mutables:?}"
    );
}

#[test]
fn and_chain_skips_immutable_branches() {
    let mutables = with_facts(
        "lib.ml",
        "let a = 0 and b = ref 0\nlet f x = b := x\n",
        |facts| {
            facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalMutable)
                .map(|a| a.name.to_owned())
                .collect::<Vec<_>>()
        },
    );
    assert!(
        !mutables.contains(&String::from("a")),
        "immutable branch is not global state, got {mutables:?}"
    );
    assert!(
        mutables.contains(&String::from("b")),
        "ref branch stays declared, got {mutables:?}"
    );
}

#[test]
fn local_ref_shadow_stays_quiet() {
    let ambient = with_facts(
        "lib.ml",
        "let r = ref 0\nlet f () = let r = ref 1 in r := 2; !r\nlet g () = r := 3\n",
        |facts| {
            facts
                .ambient
                .iter()
                .map(|a| (a.kind, a.name.to_owned(), a.pos.line))
                .collect::<Vec<_>>()
        },
    );
    assert!(
        ambient.contains(&(AmbientKind::GlobalMutable, String::from("r"), 1)),
        "file-scope ref is a global mutable, got {ambient:?}"
    );
    assert!(
        !ambient
            .iter()
            .any(|(kind, name, line)| *kind == AmbientKind::GlobalWrite
                && name == "r"
                && *line == 2),
        "local ref absorbs the shadowing write, got {ambient:?}"
    );
    assert!(
        ambient
            .iter()
            .any(|(kind, name, line)| *kind == AmbientKind::GlobalWrite
                && name == "r"
                && *line == 3),
        "plain write to the file-scope ref still reports, got {ambient:?}"
    );
}

#[test]
fn match_arm_pattern_shadow_stays_quiet() {
    let writes = with_facts(
        "lib.ml",
        "let counter = ref 0\nlet f x = match x with Some counter -> counter := 1 | None -> ()\nlet g () = counter := 2\n",
        |facts| {
            facts
                .ambient
                .iter()
                .filter(|a| a.kind == AmbientKind::GlobalWrite && a.name == "counter")
                .map(|a| a.pos.line)
                .collect::<Vec<_>>()
        },
    );
    assert!(
        !writes.contains(&2),
        "arm-bound ref absorbs the shadowing write, got {writes:?}"
    );
    assert!(
        writes.contains(&3),
        "plain write to the file-scope ref still reports, got {writes:?}"
    );
}

#[test]
fn shadowed_file_hides_toplevel_binding() {
    let shadowed: std::collections::HashSet<String> = ["a.ml".to_owned()].into_iter().collect();
    let mut parsers = Parsers::with_ocaml_interfaces(&shadowed);
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let source = "let f flag = ignore flag\nlet outer x = let inner y = y in inner x\n";
    let input = FileInput {
        lang: Lang::Ocaml,
        path: "a.ml",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut parsers, &input, &custom);
    assert!(analysis.parse_error.is_none(), "a.ml parses");
    let vis: Vec<(&str, Visibility)> = analysis
        .facts
        .functions
        .iter()
        .map(|f| (f.name, f.visibility))
        .collect();
    assert!(
        vis.contains(&("f", Visibility::Private)),
        "shadowed top-level is hidden, got {vis:?}"
    );
    assert!(
        vis.contains(&("inner", Visibility::Private)),
        "shadowed nesting stays private, got {vis:?}"
    );
}

#[test]
fn test_module_functions_are_in_test() {
    let flags = with_facts(
        "lib.ml",
        "module Tests = struct\n  let check_roundtrip flag = ignore flag\nend\nlet plain x = x\n",
        |facts| {
            facts
                .functions
                .iter()
                .map(|f| (f.name.to_owned(), f.in_test))
                .collect::<Vec<_>>()
        },
    );
    assert!(
        flags.contains(&("check_roundtrip".to_owned(), true)),
        "test module function is in test, got {flags:?}"
    );
    assert!(
        flags.contains(&("plain".to_owned(), false)),
        "plain function stays out of test, got {flags:?}"
    );
}

#[test]
fn constrained_module_function_is_free_but_forced() {
    let found = with_facts(
        "lib.ml",
        "module M : sig val f : int -> int end = struct\n  let f x = x\nend\n",
        |facts| {
            facts
                .functions
                .iter()
                .rfind(|f| f.name == "f")
                .map(|f| (f.signature_forced, f.is_method))
                .expect("`f` is recorded")
        },
    );
    assert_eq!(
        found,
        (true, false),
        "constrained function is forced but not a method, got {found:?}"
    );
}
