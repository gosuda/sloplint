//! `import name = require("module")` is an import of that module, bound whole to `name`,
//! not a side-effect import of the empty module.

use sloplint_core::facts::ImportKind;
use sloplint_core::lang::Lang;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

type Row = (String, String, ImportKind);

fn imports(source: &str) -> Vec<Row> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::TypeScript,
        path: "mod.ts",
        source,
        is_test_file: false,
        has_interface: false,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(
        analysis.parse_error.is_none(),
        "import source parses, first syntax error at {:?}",
        analysis.parse_error
    );
    analysis
        .facts
        .imports
        .iter()
        .map(|i| (i.module.to_string(), i.name.to_string(), i.kind))
        .collect()
}

fn row(module: &str, name: &str, kind: ImportKind) -> Row {
    (module.to_string(), name.to_string(), kind)
}

#[test]
fn require_imports_name_their_module_and_binding() {
    let source = "import fs = require('fs');\nimport type Config = require(\"./config\");\n";
    assert_eq!(
        imports(source),
        vec![
            row("fs", "fs", ImportKind::Namespace),
            row("./config", "Config", ImportKind::Namespace),
        ]
    );
}

#[test]
fn other_import_forms_are_unchanged() {
    let source = "import path from 'path';\nimport './polyfill';\n";
    assert_eq!(
        imports(source),
        vec![
            row("path", "path", ImportKind::Named),
            row("./polyfill", "", ImportKind::SideEffect),
        ]
    );
}
