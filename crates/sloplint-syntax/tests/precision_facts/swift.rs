use std::sync::LazyLock;

use sloplint_core::facts::{PrimitiveKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn run<'s>(path: &'s str, source: &'s str) -> Analysis<'s> {
    let input = FileInput {
        lang: Lang::Swift,
        path,
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

fn fact_of<'a>(analysis: &'a Analysis<'a>, name: &str) -> &'a sloplint_core::facts::Function<'a> {
    analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("has function {name}"))
}

#[test]
fn visibility_maps_modifiers_to_three_states() {
    let analysis = run(
        "case.swift",
        "public func a() {}\nfunc b() {}\nprivate func c() {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        fact_of(&analysis, "a").visibility,
        Visibility::Exported,
        "public is callable from another module"
    );
    assert_eq!(
        fact_of(&analysis, "b").visibility,
        Visibility::Restricted,
        "the default internal access is module-restricted"
    );
    assert_eq!(
        fact_of(&analysis, "c").visibility,
        Visibility::Private,
        "private stays file-private"
    );
}

#[test]
fn override_marks_the_signature_forced() {
    let analysis = run(
        "case.swift",
        "class A {\n    func f() {}\n}\nclass B: A {\n    override func f() {}\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let mut fs = analysis.facts.functions.iter().filter(|f| f.name == "f");
    let plain = fs.next().expect("has the base method");
    let forced = fs.next().expect("has the override");
    assert!(
        !plain.signature_forced,
        "a plain method keeps an editable signature"
    );
    assert!(
        forced.signature_forced,
        "an override restates a forced signature"
    );
}

#[test]
fn primitive_param_kinds_follow_the_swift_type_table() {
    let analysis = run(
        "case.swift",
        "func f(a: String, b: Bool, c: Int, d: Double, e: [String]) {}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    let kinds: Vec<PrimitiveKind> = fact_of(&analysis, "f")
        .params
        .iter()
        .map(|p| p.primitive_kind)
        .collect();
    assert_eq!(
        kinds,
        [
            PrimitiveKind::Str,
            PrimitiveKind::Bool,
            PrimitiveKind::Int,
            PrimitiveKind::Float,
            PrimitiveKind::Other
        ],
        "String Bool Int Double map by name and an array type stays Other"
    );
}

#[test]
fn swift_assert_shapes_split_comparison_from_bare_name() {
    let analysis = run(
        "case.swift",
        "func check(x: Int, y: Int, ready: Bool) {\n    assert(x == y)\n    assert(ready)\n    assert(load())\n}\n",
    );
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(
        analysis.facts.assertions.len(),
        3,
        "all asserts are recorded"
    );
    let first = &analysis.facts.assertions[0];
    let second = &analysis.facts.assertions[1];
    let third = &analysis.facts.assertions[2];
    assert!(first.comparison, "an equality condition is a comparison");
    assert_eq!(
        first.shape,
        sloplint_core::facts::AssertShape::TopEqNe,
        "an equality condition is top eq-ne"
    );
    assert!(!second.comparison, "a bare name is not a comparison");
    assert_eq!(
        second.shape,
        sloplint_core::facts::AssertShape::BareName,
        "a bare name is a bare name"
    );
    assert!(!third.comparison, "a call result is not a comparison");
    assert_eq!(
        third.shape,
        sloplint_core::facts::AssertShape::Other,
        "a call result keeps the default shape"
    );
    for assertion in &analysis.facts.assertions {
        assert!(!assertion.in_typecheck, "swift has no TYPE_CHECKING frame");
        assert!(
            !assertion.value_is_bool_literal,
            "bool-literal values are a go-only fact"
        );
    }
}

#[test]
fn swift_module_imports_carry_the_module_as_external_root() {
    let analysis = run("case.swift", "import Foundation\nfunc f() {}\n");
    assert!(analysis.parse_error.is_none(), "parses");
    assert_eq!(analysis.facts.imports.len(), 1, "the import is recorded");
    let import = &analysis.facts.imports[0];
    assert_eq!(
        import.external_root,
        Some("Foundation"),
        "a module import names its module as the external root"
    );
    assert!(!import.is_reexport, "an import is not a re-export");
    assert!(!import.in_test, "a library import is not test-scoped");
    let bare = run("case.swift", "func f() {}\n");
    assert!(bare.parse_error.is_none(), "parses");
    assert!(
        bare.facts.imports.is_empty(),
        "a file without imports records none"
    );
}
