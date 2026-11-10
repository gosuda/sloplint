//! Rust item visibility and head flags, observed through `analyze` only.
//!
//! Visibility text decides the fact: exactly `pub` exports, `pub(..)` restricts, and a
//! trait method inherits the trait's own visibility. `impl Trait for T` forces the
//! signature, `main` and `#[tokio::main]` mark entry points, and test attributes, test
//! modules, and plain `mod tests` mark in-test functions.

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

#[derive(Default)]
struct Flags {
    forced: bool,
    entry: bool,
    in_test: bool,
    deprecated: bool,
    decorated: bool,
    trait_item: bool,
}

type Row = (String, Visibility, bool, bool, bool, bool, bool, bool, bool);

fn functions(source: &str) -> Vec<Row> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "visibility source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .functions
        .iter()
        .map(|f| {
            (
                f.name.to_string(),
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
        .collect()
}

fn row(name: &str, visibility: Visibility, flags: Flags) -> Row {
    (
        name.to_string(),
        visibility,
        flags.forced,
        flags.entry,
        flags.in_test,
        false,
        flags.deprecated,
        flags.decorated,
        flags.trait_item,
    )
}

const PRIVATE: Visibility = Visibility::Private;
const RESTRICTED: Visibility = Visibility::Restricted;
const EXPORTED: Visibility = Visibility::Exported;

#[test]
fn visibility_text_decides_the_fact() {
    let source = "pub fn a() {}\npub(crate) fn b() {}\npub(super) fn c() {}\nfn d() {}\n";
    assert_eq!(
        functions(source),
        vec![
            row("a", EXPORTED, Flags::default()),
            row("b", RESTRICTED, Flags::default()),
            row("c", RESTRICTED, Flags::default()),
            row("d", PRIVATE, Flags::default()),
        ]
    );
}

#[test]
fn trait_methods_inherit_the_trait_visibility() {
    let source = "pub trait Draw {\nfn paint(&self);\nfn sketch(&self) {}\n}\npub(crate) trait Hidden {\nfn quiet(&self) {}\n}\n";
    assert_eq!(
        functions(source),
        vec![
            row(
                "sketch",
                EXPORTED,
                Flags {
                    trait_item: true,
                    ..Default::default()
                },
            ),
            row(
                "quiet",
                RESTRICTED,
                Flags {
                    trait_item: true,
                    ..Default::default()
                },
            ),
        ],
        "the bodiless declaration yields no function; default methods inherit"
    );
}

#[test]
fn trait_impl_methods_are_signature_forced() {
    let source = "pub trait Draw {\nfn paint(&self);\n}\nstruct Circle;\nimpl Draw for Circle {\nfn paint(&self) {}\n}\nimpl Circle {\npub fn area(&self) {}\n}\n";
    assert_eq!(
        functions(source),
        vec![
            row(
                "paint",
                PRIVATE,
                Flags {
                    forced: true,
                    ..Default::default()
                },
            ),
            row("area", EXPORTED, Flags::default()),
        ]
    );
}

#[test]
fn main_and_tokio_main_are_entry_points() {
    let source = "fn main() {}\n#[tokio::main]\nasync fn serve() {}\nfn worker() {}\n";
    assert_eq!(
        functions(source),
        vec![
            row(
                "main",
                PRIVATE,
                Flags {
                    entry: true,
                    ..Default::default()
                },
            ),
            row(
                "serve",
                PRIVATE,
                Flags {
                    entry: true,
                    decorated: true,
                    ..Default::default()
                },
            ),
            row("worker", PRIVATE, Flags::default()),
        ]
    );
}

#[test]
fn deprecated_and_decorated_attributes_are_recorded() {
    let source = "#[deprecated]\nfn old() {}\n#[inline]\nfn fast() {}\nfn plain() {}\n";
    assert_eq!(
        functions(source),
        vec![
            row(
                "old",
                PRIVATE,
                Flags {
                    deprecated: true,
                    decorated: true,
                    ..Default::default()
                },
            ),
            row(
                "fast",
                PRIVATE,
                Flags {
                    decorated: true,
                    ..Default::default()
                },
            ),
            row("plain", PRIVATE, Flags::default()),
        ]
    );
}

#[test]
fn test_marks_and_plain_test_modules_are_in_test() {
    let source = "#[test]\nfn checked() {}\nmod tests {\nfn helper() {}\n}\n#[cfg(test)]\nmod gated {\nfn other() {}\n}\nfn shipped() {}\n";
    assert_eq!(
        functions(source),
        vec![
            row(
                "checked",
                PRIVATE,
                Flags {
                    in_test: true,
                    decorated: true,
                    ..Default::default()
                },
            ),
            row(
                "helper",
                PRIVATE,
                Flags {
                    in_test: true,
                    ..Default::default()
                },
            ),
            row(
                "other",
                PRIVATE,
                Flags {
                    in_test: true,
                    ..Default::default()
                },
            ),
            row("shipped", PRIVATE, Flags::default()),
        ]
    );
}

#[test]
fn methods_named_main_are_not_entry_points() {
    let source = "impl Runner {\n    pub fn main() {}\n}\nfn main() {}\n";
    assert_eq!(
        functions(source),
        vec![
            row("main", EXPORTED, Flags::default()),
            row(
                "main",
                PRIVATE,
                Flags {
                    entry: true,
                    ..Default::default()
                },
            ),
        ],
        "only the free function at the file root is the program entry"
    );
}

#[test]
fn pub_fn_inside_a_private_module_is_not_exported() {
    let source = "mod inner {\n    pub fn load() {}\n}\npub mod open {\n    pub fn serve() {}\n}\npub(crate) mod half {\n    pub fn mid() {}\n}\npub fn top() {}\n";
    assert_eq!(
        functions(source),
        vec![
            row("load", RESTRICTED, Flags::default()),
            row("serve", EXPORTED, Flags::default()),
            row("mid", RESTRICTED, Flags::default()),
            row("top", EXPORTED, Flags::default()),
        ],
        "a pub fn under a private or restricted mod is not externally visible"
    );
}

#[test]
fn nested_fn_in_a_trait_default_body_is_not_a_trait_item() {
    let source = "pub trait Render {\n    fn go(&self) {\n        fn helper() {}\n    }\n}\nmod m {\n    pub trait T {\n        fn take(&self) {}\n    }\n}\n";
    assert_eq!(
        functions(source),
        vec![
            row(
                "go",
                EXPORTED,
                Flags {
                    trait_item: true,
                    ..Default::default()
                },
            ),
            row("helper", PRIVATE, Flags::default()),
            row(
                "take",
                RESTRICTED,
                Flags {
                    trait_item: true,
                    ..Default::default()
                },
            ),
        ],
        "trait methods keep the trait mark while nested fns stay private free fns"
    );
}

#[test]
fn extern_and_no_mangle_signatures_are_forced() {
    let source = "#[no_mangle]\npub extern \"C\" fn init() {}\npub fn plain() {}\n";
    assert_eq!(
        functions(source),
        vec![
            row(
                "init",
                EXPORTED,
                Flags {
                    forced: true,
                    decorated: true,
                    ..Default::default()
                },
            ),
            row("plain", EXPORTED, Flags::default()),
        ],
        "a C-ABI symbol keeps its forced signature while a plain fn stays actionable"
    );
}
