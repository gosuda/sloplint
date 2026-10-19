//! A Dart import without a `show` list names the library, not its file: the final path
//! segment loses its `.dart` extension. `show` imports name the shown identifiers.

use crate::support::with_facts;
use sloplint_core::facts::ImportKind;
use sloplint_core::lang::Lang;

const SOURCE: &str = "import 'dart:async';
import 'package:http/http.dart';
import 'package:http/client.dart' as client;
import 'src/model.g.dart';
import 'package:meta/meta.dart' show immutable;
export 'src/shared.dart';

void main() {}
";

/// `(module, name, alias, kind)` of every import and export in `SOURCE`.
fn imports() -> Vec<(String, String, Option<String>, ImportKind)> {
    with_facts(Lang::Dart, "case.dart", SOURCE, |facts| {
        facts
            .imports
            .iter()
            .map(|i| {
                (
                    i.module.to_string(),
                    i.name.to_string(),
                    i.alias.map(str::to_string),
                    i.kind,
                )
            })
            .collect()
    })
}

fn name_of(module: &str) -> String {
    imports()
        .into_iter()
        .find(|i| i.0 == module)
        .unwrap_or_else(|| panic!("no import of {module}"))
        .1
}

#[test]
fn package_and_relative_libraries_are_named_without_the_dart_extension() {
    let package = name_of("package:http/http.dart");
    assert_eq!(
        package, "http",
        "package import names the library, not its file; observed {package:?}"
    );
    let relative = name_of("src/shared.dart");
    assert_eq!(
        relative, "shared",
        "relative export names the library, not its file; observed {relative:?}"
    );
    assert_eq!(
        name_of("src/model.g.dart"),
        "model.g",
        "only the final `.dart` is the extension"
    );
}

#[test]
fn aliased_imports_keep_their_alias_and_a_stem_name() {
    let aliased = imports()
        .into_iter()
        .find(|i| i.0 == "package:http/client.dart")
        .expect("aliased import is present");
    let (name, alias, kind) = (aliased.1.as_str(), aliased.2.as_deref(), aliased.3);
    assert_eq!(
        name, "client",
        "aliased import keeps the stem name; observed {name:?}"
    );
    assert_eq!(
        alias,
        Some("client"),
        "the `as` alias is preserved; observed {alias:?}"
    );
    assert_eq!(
        kind,
        ImportKind::Namespace,
        "an import without `show` is a namespace import; observed {kind:?}"
    );
}

#[test]
fn schemes_without_a_file_extension_and_show_lists_are_unchanged() {
    let scheme = name_of("dart:async");
    assert_eq!(
        scheme, "async",
        "a scheme library has no file extension to strip; observed {scheme:?}"
    );
    let shown = imports()
        .into_iter()
        .find(|i| i.0 == "package:meta/meta.dart")
        .expect("show import is present");
    let observed = (shown.1.as_str(), shown.3);
    assert_eq!(
        observed,
        ("immutable", ImportKind::Named),
        "a `show` list names the shown identifier; observed {observed:?}"
    );
}
