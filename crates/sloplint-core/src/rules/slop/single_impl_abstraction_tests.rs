use super::*;
use crate::facts::{FileFacts, ImplRef, Pos, TypeDef, Visibility, WorldFacts};
use crate::finding::line_starts;

fn named(name: &str) -> NamedSite {
    NamedSite {
        name: name.into(),
        line: 1,
        column: 1,
    }
}

fn facts(package: &str, abstractions: Vec<NamedSite>, impls: Vec<NamedSite>) -> WorldFacts {
    WorldFacts {
        lang: Lang::Rust,
        role: crate::role::CodeRole::Library,
        regions: Vec::new(),
        package: package.to_string(),
        package_from_manifest: false,
        has_inline_tests: false,
        public_functions: 0,
        definitions: Vec::new(),
        abstractions,
        impls,
        barrel: None,
        imports: Vec::new(),
        reexports: Vec::new(),
    }
}

fn same_package(abstractions: Vec<NamedSite>, impls: Vec<NamedSite>) -> WorldFacts {
    facts("pkg", abstractions, impls)
}

fn test_double(abstractions: Vec<NamedSite>, impls: Vec<NamedSite>) -> WorldFacts {
    let mut double = facts("pkg", abstractions, impls);
    double.role = crate::role::CodeRole::Test;
    double
}

fn reported(files: &[WorldFile<'_>]) -> Vec<Finding> {
    let rule = SingleImplAbstraction { allow: Vec::new() };
    let mut out = Vec::new();
    rule.check(files, &mut out);
    out
}

#[test]
fn singletons_emit_in_name_order_and_duplicates_stay_silent() {
    let rule = SingleImplAbstraction { allow: Vec::new() };
    let zeta = same_package(vec![named("Zeta")], vec![named("Zeta")]);
    let alpha = same_package(vec![named("Alpha")], vec![named("Alpha")]);
    let dup_first = same_package(vec![named("Dup")], vec![named("Dup")]);
    let dup_second = same_package(vec![named("Dup")], Vec::new());
    let pair = same_package(vec![named("Pair")], Vec::new());
    let pair_impl_one = same_package(Vec::new(), vec![named("Pair")]);
    let pair_impl_two = same_package(Vec::new(), vec![named("Pair")]);
    let files = [
        WorldFile {
            parsed: true,
            path: "z.rs",
            facts: &zeta,
        },
        WorldFile {
            parsed: true,
            path: "a.rs",
            facts: &alpha,
        },
        WorldFile {
            parsed: true,
            path: "d1.rs",
            facts: &dup_first,
        },
        WorldFile {
            parsed: true,
            path: "d2.rs",
            facts: &dup_second,
        },
        WorldFile {
            parsed: true,
            path: "p.rs",
            facts: &pair,
        },
        WorldFile {
            parsed: true,
            path: "p1.rs",
            facts: &pair_impl_one,
        },
        WorldFile {
            parsed: true,
            path: "p2.rs",
            facts: &pair_impl_two,
        },
    ];
    let mut out = Vec::new();
    rule.check(&files, &mut out);
    let messages: Vec<String> = out.iter().map(|finding| finding.message.clone()).collect();
    assert_eq!(messages.len(), 2);
    assert!(
        messages[0].contains("`Alpha`"),
        "the first finding must be `Alpha`: {}",
        messages[0]
    );
    assert!(
        messages[1].contains("`Zeta`"),
        "the second finding must be `Zeta`: {}",
        messages[1]
    );
}

#[test]
fn same_name_in_two_packages_flags_twice() {
    let rule = SingleImplAbstraction { allow: Vec::new() };
    let left = facts("left", vec![named("Draw")], vec![named("Draw")]);
    let right = facts("right", vec![named("Draw")], vec![named("Draw")]);
    let files = [
        WorldFile {
            parsed: true,
            path: "left.rs",
            facts: &left,
        },
        WorldFile {
            parsed: true,
            path: "right.rs",
            facts: &right,
        },
    ];
    let mut out = Vec::new();
    rule.check(&files, &mut out);
    assert_eq!(
        out.len(),
        2,
        "each package has its own sole implementation: {out:?}"
    );
}

fn in_lang(lang: Lang, abstractions: Vec<NamedSite>, impls: Vec<NamedSite>) -> WorldFacts {
    let mut file = same_package(abstractions, impls);
    file.lang = lang;
    file
}

#[test]
fn incomplete_c_header_blocks_only_its_cpp_scope() {
    let cpp = in_lang(Lang::Cpp, vec![named("Shape")], vec![named("Shape")]);
    let header = in_lang(Lang::C, Vec::new(), Vec::new());
    let mut other_package = in_lang(Lang::Cpp, vec![named("Shape")], vec![named("Shape")]);
    other_package.package = "other".to_string();
    let script = in_lang(Lang::TypeScript, vec![named("Shape")], vec![named("Shape")]);
    let files = [
        WorldFile {
            path: "pkg/shape.cpp",
            facts: &cpp,
            parsed: true,
        },
        WorldFile {
            path: "pkg/damaged.h",
            facts: &header,
            parsed: false,
        },
        WorldFile {
            path: "other/control.cpp",
            facts: &other_package,
            parsed: true,
        },
        WorldFile {
            path: "pkg/control.ts",
            facts: &script,
            parsed: true,
        },
    ];
    let out = reported(&files);
    let mut paths: Vec<&str> = out.iter().map(|finding| &*finding.path).collect();
    paths.sort_unstable();
    assert_eq!(paths, ["other/control.cpp", "pkg/control.ts"]);
}

#[test]
fn a_second_implementation_in_tsx_counts_for_a_typescript_interface() {
    let script = in_lang(Lang::TypeScript, vec![named("Shape")], vec![named("Shape")]);
    let view = in_lang(Lang::Tsx, Vec::new(), vec![named("Shape")]);
    let files = [
        WorldFile {
            parsed: true,
            path: "shape.ts",
            facts: &script,
        },
        WorldFile {
            parsed: true,
            path: "square.tsx",
            facts: &view,
        },
    ];
    let out = reported(&files);
    assert!(
        out.is_empty(),
        "TypeScript and TSX share one namespace: {out:?}"
    );
}

#[test]
fn an_implementation_in_an_unrelated_language_is_not_a_second_one() {
    let script = in_lang(Lang::TypeScript, vec![named("Shape")], vec![named("Shape")]);
    let other = in_lang(Lang::Python, Vec::new(), vec![named("Shape")]);
    let files = [
        WorldFile {
            parsed: true,
            path: "shape.ts",
            facts: &script,
        },
        WorldFile {
            parsed: true,
            path: "shape.py",
            facts: &other,
        },
    ];
    let out = reported(&files);
    assert_eq!(
        out.len(),
        1,
        "a Python class cannot implement a TypeScript interface: {out:?}"
    );
}

#[test]
fn impl_in_another_package_stays_silent() {
    let rule = SingleImplAbstraction { allow: Vec::new() };
    let declaration = facts("left", vec![named("Draw")], Vec::new());
    let elsewhere = facts("right", Vec::new(), vec![named("Draw")]);
    let files = [
        WorldFile {
            parsed: true,
            path: "left.rs",
            facts: &declaration,
        },
        WorldFile {
            parsed: true,
            path: "right.rs",
            facts: &elsewhere,
        },
    ];
    let mut out = Vec::new();
    rule.check(&files, &mut out);
    assert!(
        out.is_empty(),
        "an impl from another package is not a local implementation: {out:?}"
    );
}

#[test]
fn ext_suffixed_name_with_one_impl_flags() {
    let rule = SingleImplAbstraction { allow: Vec::new() };
    let ext = same_package(vec![named("ParseExt")], vec![named("ParseExt")]);
    let files = [WorldFile {
        parsed: true,
        path: "input.rs",
        facts: &ext,
    }];
    let mut out = Vec::new();
    rule.check(&files, &mut out);
    assert_eq!(
        out.len(),
        1,
        "a name suffix must not exempt a sole implementation: {out:?}"
    );
}

fn abstraction<'s>(name: &'s str, visibility: Visibility) -> TypeDef<'s> {
    typedef(name, visibility, true)
}

fn typedef<'s>(name: &'s str, visibility: Visibility, abstraction: bool) -> TypeDef<'s> {
    TypeDef {
        name,
        pos: Pos { line: 1, byte: 0 },
        visibility,
        abstraction,
    }
}

fn concrete_impl<'s>(name: &'s str) -> ImplRef<'s> {
    implref(name, false)
}

fn generic_impl<'s>(name: &'s str) -> ImplRef<'s> {
    implref(name, true)
}

fn implref<'s>(name: &'s str, generic: bool) -> ImplRef<'s> {
    ImplRef {
        name,
        pos: Pos { line: 1, byte: 0 },
        is_generic: generic,
        child_is_abstract: false,
    }
}

fn file_facts<'s>(
    source: &'s str,
    types: Vec<TypeDef<'s>>,
    impls: Vec<ImplRef<'s>>,
) -> FileFacts<'s> {
    FileFacts {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        line_starts: line_starts(source),
        role: crate::role::CodeRole::Library,
        regions: Vec::new(),
        package: "pkg".to_string(),
        is_generated: false,
        line_count: 1,
        functions: Vec::new(),
        calls: Vec::new(),
        bindings: Vec::new(),
        imports: Vec::new(),
        types,
        impls,
        wildcard_arms: Vec::new(),
        handlers: Vec::new(),
        redundant_elses: Vec::new(),
        blocking_sleeps: Vec::new(),
        assertions: Vec::new(),
        field_copies: Vec::new(),
        dynamic_symbols: Vec::new(),
        ambient: Vec::new(),
        barrel: None,
        has_inline_tests: false,
    }
}

fn projected(source: &str, types: Vec<TypeDef<'_>>, impls: Vec<ImplRef<'_>>) -> usize {
    let rule = SingleImplAbstraction { allow: Vec::new() };
    let file = file_facts(source, types, impls);
    let world = WorldFacts::from_facts(&file);
    let files = [WorldFile {
        parsed: true,
        path: "src/lib.rs",
        facts: &world,
    }];
    let mut out = Vec::new();
    rule.check(&files, &mut out);
    out.len()
}

#[test]
fn projected_exported_trait_stays_silent() {
    let count = projected(
        "pub trait Draw {}\n",
        vec![abstraction("Draw", Visibility::Exported)],
        vec![concrete_impl("Draw")],
    );
    assert_eq!(
        count, 0,
        "an exported trait never reaches the rule: {count}"
    );
}

#[test]
fn projected_generic_impl_stays_silent() {
    let count = projected(
        "trait Captures {}\n",
        vec![abstraction("Captures", Visibility::Restricted)],
        vec![generic_impl("Captures")],
    );
    assert_eq!(count, 0, "a generic impl never reaches the rule: {count}");
}

#[test]
fn projected_private_trait_with_concrete_impl_flags() {
    let count = projected(
        "trait Draw {}\n",
        vec![abstraction("Draw", Visibility::Restricted)],
        vec![concrete_impl("Draw")],
    );
    assert_eq!(
        count, 1,
        "a crate-visible trait with one concrete impl flags: {count}"
    );
}

#[test]
fn test_double_impl_silences_the_library_singleton() {
    let library = same_package(vec![named("Repo")], vec![named("Repo")]);
    let double = test_double(Vec::new(), vec![named("Repo")]);
    let files = [
        WorldFile {
            parsed: true,
            path: "a.rs",
            facts: &library,
        },
        WorldFile {
            parsed: true,
            path: "t.rs",
            facts: &double,
        },
    ];
    assert!(
        reported(&files).is_empty(),
        "a test double is a second implementation and keeps the seam: {:?}",
        reported(&files)
    );
}

#[test]
fn abstraction_and_impl_only_in_a_test_file_stay_silent() {
    let double = test_double(vec![named("Repo")], vec![named("Repo")]);
    let files = [WorldFile {
        parsed: true,
        path: "t.rs",
        facts: &double,
    }];
    assert!(
        reported(&files).is_empty(),
        "a test-only impl must stay silent"
    );
}

#[test]
fn test_region_abstraction_does_not_ambiguate_the_library_singleton() {
    let library = same_package(vec![named("Repo")], vec![named("Repo")]);
    let double = test_double(vec![named("Repo")], Vec::new());
    let files = [
        WorldFile {
            parsed: true,
            path: "a.rs",
            facts: &library,
        },
        WorldFile {
            parsed: true,
            path: "t.rs",
            facts: &double,
        },
    ];
    let out = reported(&files);
    assert_eq!(
        out.len(),
        1,
        "the exempt abstraction declaration must not block the library report: {out:?}"
    );
    assert_eq!(&*out[0].path, "a.rs");
}
