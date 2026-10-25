//! Grouped Rust `use` trees spell each leaf's module and name exactly as the plain `use` would.
//!
//! The import-fan-out rule counts distinct module strings and the no-rename-import rule
//! quotes the imported name, so `{HashMap}` and `HashMap` must agree, and a `self` leaf names
//! the module it re-imports rather than the keyword.

use sloplint_core::facts::ImportKind;
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

type Row = (String, String, Option<String>, ImportKind);

fn imports(source: &str) -> Vec<Row> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Rust,
        path: "src/lib.rs",
        source,
        is_test_file: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "use-tree source parses, first syntax error at {:?}",
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
                import.alias.map(str::to_string),
                import.kind,
            )
        })
        .collect()
}

fn row(module: &str, name: &str, alias: Option<&str>, kind: ImportKind) -> Row {
    (
        module.to_string(),
        name.to_string(),
        alias.map(str::to_string),
        kind,
    )
}

fn named(module: &str, name: &str, alias: Option<&str>) -> Row {
    row(module, name, alias, ImportKind::Named)
}

#[test]
fn grouped_leaves_share_the_module_of_the_plain_form() {
    let source = "use std::collections::{HashMap, HashSet};\nuse std::collections::BTreeMap;\n";
    assert_eq!(
        imports(source),
        vec![
            named("std::collections", "HashMap", None),
            named("std::collections", "HashSet", None),
            named("std::collections", "BTreeMap", None),
        ],
        "one module string for every spelling of the same import"
    );
}

#[test]
fn self_leaves_name_the_module_they_reimport() {
    let source = "use std::io::{self, Write};\nuse std::fmt::{self as fmt_mod};\nuse a::{b::{self, C}, D};\n";
    assert_eq!(
        imports(source),
        vec![
            named("std", "io", None),
            named("std::io", "Write", None),
            named("std", "fmt", Some("fmt_mod")),
            named("a", "b", None),
            named("a::b", "C", None),
            named("a", "D", None),
        ],
        "`self` resolves to the last segment of its enclosing path"
    );
}

#[test]
fn wildcards_and_plain_paths_keep_their_modules() {
    let source = "use std::{io::*, fmt::*};\nuse std::io::Write;\n";
    assert_eq!(
        imports(source),
        vec![
            row("std::io", "*", None, ImportKind::Wildcard),
            row("std::fmt", "*", None, ImportKind::Wildcard),
            named("std::io", "Write", None),
        ],
        "wildcard and ungrouped imports are unchanged"
    );
}
