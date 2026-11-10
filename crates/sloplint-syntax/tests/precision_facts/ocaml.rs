use sloplint_core::facts::{ImportKind, ImportScope, ScrutineeKind, Visibility};
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
                .find(|f| f.name == "f")
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
        (true, ScrutineeKind::Other, 0, false),
        "the `_` arm beside constructors is enum-like with default counts, got {arm:?}"
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
fn assert_comparison_keeps_default_shape() {
    let compared = with_facts("lib.ml", "let check x y = assert (x = y)\n", |facts| {
        facts
            .assertions
            .first()
            .map(|a| (a.comparison, a.shape, a.has_message))
            .expect("the assert is recorded")
    });
    assert_eq!(
        compared,
        (true, sloplint_core::facts::AssertShape::Other, false),
        "relational assert keeps comparison with default shape, got {compared:?}"
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
        "open Printf\ninclude Set\nlet id x = x\n",
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
        2,
        "both module directives are recorded, got {imports:?}"
    );
    for (kind, scope, in_test, is_reexport, external_root) in &imports {
        assert_eq!(
            (*kind, *scope, *in_test, *is_reexport, external_root.clone()),
            (
                ImportKind::Wildcard,
                ImportScope::Module,
                false,
                false,
                None
            ),
            "open directives stay module-scoped with no external root, got {imports:?}"
        );
    }
}
