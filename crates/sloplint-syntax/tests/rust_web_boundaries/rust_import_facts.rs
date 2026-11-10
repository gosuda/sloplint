//! Rust import facts: scope, test marking, re-exports, and external roots.
//!
//! The adapter fills every `Import` field the fan-out and wildcard rules read. External
//! roots are first path segments minus the standard and local prefixes, wildcard kind
//! survives only at item scope outside preludes and local modules, and test-context
//! imports mark `in_test` unless a wildcard exemption clears it.

use sloplint_core::facts::{ImportKind, ImportScope};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

struct ImportFlags {
    scope: ImportScope,
    in_test: bool,
    reexport: bool,
    root: Option<String>,
}

type Row = (
    String,
    String,
    ImportKind,
    ImportScope,
    bool,
    bool,
    Option<String>,
);

fn imports(source: &str) -> Vec<Row> {
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
        "use source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .imports
        .iter()
        .map(|import| {
            (
                import.module.to_string(),
                import.name.to_string(),
                import.kind,
                import.scope,
                import.in_test,
                import.is_reexport,
                import.external_root.map(str::to_string),
            )
        })
        .collect()
}

fn row(module: &str, name: &str, kind: ImportKind, flags: ImportFlags) -> Row {
    (
        module.to_string(),
        name.to_string(),
        kind,
        flags.scope,
        flags.in_test,
        flags.reexport,
        flags.root,
    )
}

fn plain(root: Option<&str>) -> ImportFlags {
    ImportFlags {
        scope: ImportScope::Module,
        in_test: false,
        reexport: false,
        root: root.map(str::to_string),
    }
}

fn reexported(root: Option<&str>) -> ImportFlags {
    ImportFlags {
        reexport: true,
        ..plain(root)
    }
}

const NAMED: ImportKind = ImportKind::Named;
const WILD: ImportKind = ImportKind::Wildcard;

#[test]
fn external_roots_skip_std_and_local_prefixes() {
    let source = "use serde::Deserialize;\nuse std::fmt::Display;\nuse crate::foo::Bar;\nuse super::baz::Qux;\n";
    assert_eq!(
        imports(source),
        vec![
            row("serde", "Deserialize", NAMED, plain(Some("serde"))),
            row("std::fmt", "Display", NAMED, plain(None)),
            row("crate::foo", "Bar", NAMED, plain(None)),
            row("super::baz", "Qux", NAMED, plain(None)),
        ]
    );
}

#[test]
fn reexports_and_function_local_scope_are_marked() {
    let source = "pub use foo::Bar;\nfn f() {\nuse foo::Baz;\n}\n";
    assert_eq!(
        imports(source),
        vec![
            row("foo", "Bar", NAMED, reexported(Some("foo"))),
            row(
                "foo",
                "Baz",
                NAMED,
                ImportFlags {
                    scope: ImportScope::FunctionLocal,
                    ..plain(Some("foo"))
                },
            ),
        ]
    );
}

#[test]
fn test_module_imports_mark_in_test() {
    let source = "#[cfg(test)]\nmod tests {\nuse foo::Bar;\n}\n";
    assert_eq!(
        imports(source),
        vec![row(
            "foo",
            "Bar",
            NAMED,
            ImportFlags {
                in_test: true,
                ..plain(Some("foo"))
            },
        )]
    );
}

#[test]
fn prelude_globs_are_not_wildcards() {
    let source = "use foo::prelude::*;\nuse crate::alloc_prelude::*;\nuse std::io::*;\n";
    assert_eq!(
        imports(source),
        vec![
            row("foo::prelude", "*", NAMED, plain(Some("foo"))),
            row("crate::alloc_prelude", "*", NAMED, plain(None)),
            row("std::io", "*", WILD, plain(None)),
        ]
    );
}

#[test]
fn private_super_globs_are_not_wildcards() {
    let source = "mod m {\nuse super::*;\n}\npub mod n {\nuse super::*;\n}\n";
    assert_eq!(
        imports(source),
        vec![
            row("super", "*", NAMED, plain(None)),
            row("super", "*", WILD, plain(None)),
        ]
    );
}

#[test]
fn same_file_child_globs_are_not_wildcards() {
    let source = "mod handlers;\npub use handlers::*;\npub use crate::handlers::parse;\n";
    assert_eq!(
        imports(source),
        vec![
            row("handlers", "*", NAMED, reexported(None)),
            row("crate::handlers", "parse", NAMED, reexported(None)),
        ]
    );
}

#[test]
fn cfg_test_super_glob_clears_in_test() {
    let source = "#[cfg(test)]\nmod tests {\nuse super::*;\n}\n";
    assert_eq!(imports(source), vec![row("super", "*", NAMED, plain(None))]);
}
