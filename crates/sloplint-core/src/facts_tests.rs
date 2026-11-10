use crate::facts::*;
use crate::finding::line_starts;
use crate::lang::Lang;
use crate::role::CodeRole;

fn plain_function(name: &str, pos: Pos) -> Function<'_> {
    Function {
        name,
        pos,
        doc: vec![],
        doc_pos: None,
        visibility: Visibility::Exported,
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        is_method: false,
        params: vec![],
        return_type: None,
        untyped_throws: false,
        missing_annotations: vec![],
        cognitive: 0,
        max_depth: 0,
        deepest_line: 0,
        end_line: pos.line,
        has_value_return: false,
        sole_call: None,
    }
}

#[test]
fn projects_public_surface_and_barrel() {
    let source = "pub fn keep() {}\nfn hidden() {}\n";
    let mut sealed = plain_function("sealed", Pos { line: 1, byte: 8 });
    sealed.signature_forced = true;
    let mut hidden = plain_function("hidden", Pos { line: 2, byte: 4 });
    hidden.visibility = Visibility::Private;
    let facts = FileFacts {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        line_starts: line_starts(source),
        role: CodeRole::Entry,
        regions: Vec::new(),
        line_count: 2,
        package: "ledger".to_string(),
        is_generated: false,
        functions: vec![
            plain_function("keep", Pos { line: 1, byte: 8 }),
            sealed,
            hidden,
        ],
        calls: vec![],
        bindings: vec![],
        imports: vec![],
        types: vec![TypeDef {
            name: "Shape",
            pos: Pos { line: 2, byte: 100 },
            visibility: Visibility::Restricted,
            abstraction: true,
        }],
        impls: vec![ImplRef {
            name: "Draw",
            pos: Pos { line: 1, byte: 0 },
            is_generic: false,
            child_is_abstract: false,
        }],
        wildcard_arms: vec![],
        handlers: vec![],
        assertions: vec![],
        field_copies: vec![],
        dynamic_symbols: vec![],
        ambient: vec![],
        barrel: Some(Barrel {
            targets: vec![vec!["a.rs".to_string(), "a/mod.rs".to_string()]],
        }),
        has_inline_tests: false,
    };
    let world = WorldFacts::from_facts(&facts);
    assert_eq!(world.package, "ledger");
    assert_eq!(world.public_functions, 2);
    assert_eq!(
        world
            .definitions
            .iter()
            .map(|d| d.name.as_ref())
            .collect::<Vec<_>>(),
        ["keep"]
    );
    assert_eq!(world.definitions[0].line, 1);
    assert_eq!(world.definitions[0].column, 9);
    assert_eq!(
        world
            .abstractions
            .iter()
            .map(|a| a.name.as_ref())
            .collect::<Vec<_>>(),
        ["Shape"]
    );
    assert_eq!(world.impls.len(), 1);
    assert_eq!(
        world.barrel,
        Some(vec![vec!["a.rs".to_string(), "a/mod.rs".to_string()]])
    );
}

#[test]
fn serialized_world_facts_carry_no_line_text() {
    let source = "pub fn listed() { let withheld_token = 1; }\n";
    let facts = FileFacts {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        line_starts: line_starts(source),
        role: CodeRole::Library,
        regions: Vec::new(),
        line_count: 1,
        package: "ledger".to_string(),
        is_generated: false,
        functions: vec![plain_function("listed", Pos { line: 1, byte: 8 })],
        calls: vec![],
        bindings: vec![],
        imports: vec![],
        types: vec![],
        impls: vec![],
        wildcard_arms: vec![],
        handlers: vec![],
        assertions: vec![],
        field_copies: vec![],
        dynamic_symbols: vec![],
        ambient: vec![],
        barrel: None,
        has_inline_tests: false,
    };
    let world = WorldFacts::from_facts(&facts);
    let serialized = String::from_utf8(
        sonic_rs::to_vec(&world).unwrap_or_else(|e| panic!("serialization failed: {e}")),
    )
    .unwrap_or_else(|e| panic!("serialized bytes are not UTF-8: {e}"));
    assert!(
        !serialized.contains("withheld_token"),
        "world facts serialization must not carry source line text: {serialized}"
    );
    assert!(
        serialized.contains("\"line\":1") && serialized.contains("\"column\":9"),
        "positions stay in the serialization: {serialized}"
    );
    assert!(
        serialized.contains("\"package\":\"ledger\""),
        "the package key travels with the world facts: {serialized}"
    );
}

#[test]
fn actionable_api_needs_exported_unforced_outside_tests() {
    let mut forced = plain_function("forced", Pos { line: 1, byte: 0 });
    forced.signature_forced = true;
    let mut internal = plain_function("internal", Pos { line: 2, byte: 0 });
    internal.visibility = Visibility::Restricted;
    let mut private = plain_function("private_fn", Pos { line: 3, byte: 0 });
    private.visibility = Visibility::Private;
    let mut tested = plain_function("tested", Pos { line: 4, byte: 0 });
    tested.in_test = true;
    let plain = plain_function("plain", Pos { line: 5, byte: 0 });
    assert!(plain.exported());
    assert!(plain.actionable_api());
    assert!(forced.exported());
    assert!(!forced.actionable_api());
    assert!(!internal.exported());
    assert!(!internal.actionable_api());
    assert!(!private.exported());
    assert!(!private.actionable_api());
    assert!(tested.exported());
    assert!(!tested.actionable_api());
}

#[test]
fn world_facts_carry_the_package_key() {
    let source = "pub fn keep() {}\n";
    let mut facts = FileFacts {
        lang: Lang::Rust,
        path: "ledger/src/lib.rs",
        source,
        line_starts: line_starts(source),
        role: CodeRole::Library,
        regions: Vec::new(),
        line_count: 1,
        package: "ledger".to_string(),
        is_generated: false,
        functions: vec![plain_function("keep", Pos { line: 1, byte: 8 })],
        calls: vec![],
        bindings: vec![],
        imports: vec![],
        types: vec![],
        impls: vec![],
        wildcard_arms: vec![],
        handlers: vec![],
        assertions: vec![],
        field_copies: vec![],
        dynamic_symbols: vec![],
        ambient: vec![],
        barrel: None,
        has_inline_tests: false,
    };
    assert_eq!(WorldFacts::from_facts(&facts).package, "ledger");
    facts.package = String::new();
    assert_eq!(WorldFacts::from_facts(&facts).package, "");
}
fn facts_with(
    source: &'static str,
    functions: Vec<Function<'static>>,
    types: Vec<TypeDef<'static>>,
    impls: Vec<ImplRef<'static>>,
) -> FileFacts<'static> {
    FileFacts {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        line_starts: line_starts(source),
        role: CodeRole::Library,
        regions: Vec::new(),
        line_count: 2,
        package: "ledger".to_string(),
        is_generated: false,
        functions,
        types,
        impls,
        calls: vec![],
        bindings: vec![],
        imports: vec![],
        wildcard_arms: vec![],
        handlers: vec![],
        assertions: vec![],
        field_copies: vec![],
        dynamic_symbols: vec![],
        ambient: vec![],
        barrel: None,
        has_inline_tests: false,
    }
}

#[test]
fn trait_items_stay_out_of_definitions() {
    let source = "pub trait Store { fn get(&self); }\npub fn get() {}\n";
    let mut item = plain_function("get", Pos { line: 1, byte: 16 });
    item.is_trait_item = true;
    let world = WorldFacts::from_facts(&facts_with(
        source,
        vec![item, plain_function("get", Pos { line: 2, byte: 8 })],
        vec![],
        vec![],
    ));
    assert_eq!(
        world
            .definitions
            .iter()
            .map(|d| (d.name.as_ref(), d.line))
            .collect::<Vec<_>>(),
        [("get", 2)],
    );
}

#[test]
fn methods_stay_out_of_definitions() {
    let source = "impl Store { pub fn get(&self) {} }\npub fn get() {}\n";
    let mut method = plain_function("get", Pos { line: 1, byte: 22 });
    method.is_method = true;
    let world = WorldFacts::from_facts(&facts_with(
        source,
        vec![method, plain_function("get", Pos { line: 2, byte: 8 })],
        vec![],
        vec![],
    ));
    assert_eq!(
        world
            .definitions
            .iter()
            .map(|d| (d.name.as_ref(), d.line))
            .collect::<Vec<_>>(),
        [("get", 2)],
    );
}

#[test]
fn entry_points_stay_out_of_definitions() {
    let source = "pub fn main() {}\n";
    for path in ["src/bin/a.rs", "src/bin/b.rs"] {
        let mut main = plain_function("main", Pos { line: 1, byte: 8 });
        main.entry_point = true;
        let mut facts = facts_with(source, vec![main], vec![], vec![]);
        facts.path = path;
        let world = WorldFacts::from_facts(&facts);
        assert!(
            world.definitions.is_empty(),
            "entry point main in {path} must not count as a homonym definition"
        );
    }
}

#[test]
fn exported_abstractions_stay_out_of_world() {
    let source = "pub trait ValueEnum {}\ntrait Draw {}\n";
    let world = WorldFacts::from_facts(&facts_with(
        source,
        vec![],
        vec![
            TypeDef {
                name: "ValueEnum",
                pos: Pos { line: 1, byte: 11 },
                visibility: Visibility::Exported,
                abstraction: true,
            },
            TypeDef {
                name: "Draw",
                pos: Pos { line: 2, byte: 7 },
                visibility: Visibility::Private,
                abstraction: true,
            },
        ],
        vec![],
    ));
    assert_eq!(
        world
            .abstractions
            .iter()
            .map(|a| a.name.as_ref())
            .collect::<Vec<_>>(),
        ["Draw"],
    );
}

#[test]
fn generic_and_abstract_impls_stay_out_of_world() {
    let source = "impl Draw for Circle {}\n";
    let world = WorldFacts::from_facts(&facts_with(
        source,
        vec![],
        vec![],
        vec![
            ImplRef {
                name: "Captures",
                pos: Pos { line: 1, byte: 0 },
                is_generic: true,
                child_is_abstract: false,
            },
            ImplRef {
                name: "Base",
                pos: Pos { line: 1, byte: 0 },
                is_generic: false,
                child_is_abstract: true,
            },
            ImplRef {
                name: "Draw",
                pos: Pos { line: 1, byte: 0 },
                is_generic: false,
                child_is_abstract: false,
            },
        ],
    ));
    assert_eq!(
        world
            .impls
            .iter()
            .map(|i| i.name.as_ref())
            .collect::<Vec<_>>(),
        ["Draw"],
    );
}
