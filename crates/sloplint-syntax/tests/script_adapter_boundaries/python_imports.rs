//! Python import facts: external roots skip stdlib and relative paths,
//! function-local imports carry scope, and `__init__` from-imports reexport.

use sloplint_core::facts::{ImportKind, ImportScope};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{CustomRules, FileInput, Parsers, analyze};

const SOURCE: &str = r#"import os
import requests
import requests.models as models
from urllib.parse import urlparse
from requests.models import Response
from . import local
from .models import helper


def fetch(url):
    import json
    return json.loads(url)
"#;

const INIT: &str = r#"from .core import Engine
from .utils import helper
"#;

fn imports_of(source: &str, path: &str) -> Vec<(String, Option<String>, String, bool, bool)> {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path,
        source,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    assert!(analysis.parse_error.is_none(), "parses");
    analysis
        .facts
        .imports
        .iter()
        .map(|i| {
            (
                i.module.to_string(),
                i.external_root.map(str::to_owned),
                i.name.to_owned(),
                i.scope == ImportScope::FunctionLocal,
                i.is_reexport,
            )
        })
        .collect()
}

#[test]
fn stdlib_imports_have_no_external_root() {
    let imports = imports_of(SOURCE, "pkg/case.py");
    let os = imports.iter().find(|i| i.0 == "os").expect("has os");
    assert_eq!(os.1, None);
    assert_eq!(os.2, "os");
}

#[test]
fn third_party_imports_record_top_package() {
    let imports = imports_of(SOURCE, "pkg/case.py");
    let plain = imports
        .iter()
        .find(|i| i.0 == "requests" && i.2 == "requests")
        .expect("has requests");
    assert_eq!(plain.1, Some("requests".to_owned()));
    let aliased = imports
        .iter()
        .find(|i| i.0 == "requests.models")
        .expect("has requests.models");
    assert_eq!(aliased.1, Some("requests".to_owned()));
    assert_eq!(aliased.2, "models");
    let from = imports
        .iter()
        .find(|i| i.0 == "urllib.parse")
        .expect("has urllib.parse");
    assert_eq!(from.1, None);
    assert_eq!(from.2, "urlparse");
    let third_from = imports
        .iter()
        .find(|i| i.0 == "requests.models" && i.2 == "Response")
        .expect("has requests.models Response");
    assert_eq!(third_from.1, Some("requests".to_owned()));
}

#[test]
fn relative_imports_have_no_external_root() {
    let imports = imports_of(SOURCE, "pkg/case.py");
    assert!(imports.iter().any(|i| i.0 == "." && i.1.is_none()));
    assert!(
        imports
            .iter()
            .any(|i| i.0 == ".models" && i.1.is_none() && i.2 == "helper")
    );
}

#[test]
fn imports_inside_functions_are_function_local() {
    let custom = CustomRules::compile(&[]).expect("empty custom rules compile");
    let input = FileInput {
        lang: Lang::Python,
        path: "pkg/case.py",
        source: SOURCE,
        role: CodeRole::Library,
    };
    let analysis = analyze(&mut Parsers::new(), &input, &custom);
    let json = analysis
        .facts
        .imports
        .iter()
        .find(|i| i.module == "json")
        .expect("has json");
    assert_eq!(json.scope, ImportScope::FunctionLocal);
    assert_eq!(json.kind, ImportKind::Named);
    let top = analysis
        .facts
        .imports
        .iter()
        .find(|i| i.module == "os")
        .expect("has os");
    assert_eq!(top.scope, ImportScope::Module);
}

#[test]
fn init_from_imports_are_reexports() {
    let imports = imports_of(INIT, "pkg/__init__.py");
    assert_eq!(imports.len(), 2);
    assert!(imports.iter().all(|i| i.4));
    assert!(
        imports
            .iter()
            .all(|i| i.1.is_none() || i.1 == Some("pkg".to_owned()))
    );
    let plain = imports_of(SOURCE, "pkg/case.py");
    assert!(plain.iter().all(|i| !i.4));
}
