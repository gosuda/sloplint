use std::sync::LazyLock;

use sloplint_core::facts::{ArgShape, AssertShape, CallKind, ImportKind, Visibility};
use sloplint_core::lang::Lang;
use sloplint_core::role::CodeRole;
use sloplint_syntax::{Analysis, CustomRules, FileInput, Parsers, analyze};

static NO_CUSTOM: LazyLock<CustomRules> =
    LazyLock::new(|| CustomRules::compile(&[]).expect("empty custom rules compile"));

fn facts_of(source: &str) -> Analysis<'_> {
    let input = FileInput {
        lang: Lang::JavaScript,
        path: "src/app.js",
        source,
        role: CodeRole::Library,
    };
    analyze(&mut Parsers::new(), &input, &NO_CUSTOM)
}

#[test]
fn left_nested_string_concat_carries_longest_run() {
    let analysis = facts_of(
        "log.info('Connecting to database ' + host + ':' + port);\nlog.info(host + port);\nlog.error('oops: ' + err);\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.calls.len(),
        3,
        "three calls, got {:?}",
        analysis.facts.calls.len()
    );
    assert_eq!(
        analysis.facts.calls[0].args[0].shape,
        ArgShape::Concat { longest_static: 23 },
        "left-nested chain carries the 23-char run"
    );
    assert_eq!(
        analysis.facts.calls[1].args[0].shape,
        ArgShape::Other,
        "string-free addition stays Other"
    );
    assert_eq!(
        analysis.facts.calls[2].args[0].shape,
        ArgShape::Concat { longest_static: 6 },
        "short static run stays measurable"
    );
}

#[test]
fn interpolated_template_is_concat_with_measured_run() {
    let analysis = facts_of(
        "log.info(`Failed to load ${name}`);\nlog.info(`${a}${b}`);\nlog.info(`plain`);\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.calls.len(),
        3,
        "three calls, got {:?}",
        analysis.facts.calls.len()
    );
    assert_eq!(
        analysis.facts.calls[0].args[0].shape,
        ArgShape::Concat { longest_static: 15 },
        "interpolated template carries its static run"
    );
    assert_eq!(
        analysis.facts.calls[1].args[0].shape,
        ArgShape::Concat { longest_static: 0 },
        "all-dynamic interpolation carries zero"
    );
    assert_eq!(
        analysis.facts.calls[2].args[0].shape,
        ArgShape::Str { longest_static: 5 },
        "plain template stays Str"
    );
}

#[test]
fn keyed_and_closure_arguments_reclassified() {
    let analysis = facts_of("runTask({ a: 1 });\nrunTask([() => 1]);\nrunTask([1, 2]);\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.calls.len(),
        3,
        "three calls, got {:?}",
        analysis.facts.calls.len()
    );
    assert_eq!(
        analysis.facts.calls[0].args[0].shape,
        ArgShape::Other,
        "keyed literal is not a Collection"
    );
    assert_eq!(
        analysis.facts.calls[1].args[0].shape,
        ArgShape::CodeBody,
        "collection holding a closure is a CodeBody"
    );
    assert_eq!(
        analysis.facts.calls[2].args[0].shape,
        ArgShape::Collection,
        "plain list stays a Collection"
    );
}

#[test]
fn return_object_literal_emits_source() {
    let analysis = facts_of(
        "function toDto(u) { return { id: u.id, name: u.name, email: u.email, role: u.role, active: u.active, created: u.created }; }\nfunction toReport(u) { return { id: u.id, total: u.count + u.extra }; }\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let sourced: Vec<_> = analysis
        .facts
        .field_copies
        .iter()
        .filter(|copy| copy.source == Some("u"))
        .collect();
    assert_eq!(
        sourced.len(),
        1,
        "one-to-one return literal emits its source, got {:?}",
        analysis.facts.field_copies
    );
    assert_eq!(sourced[0].fields, 6, "the literal keeps its six fields");
    assert!(
        analysis
            .facts
            .field_copies
            .iter()
            .all(|copy| copy.source != Some("u") || copy.fields == 6),
        "computed field breaks the source, got {:?}",
        analysis.facts.field_copies
    );
}

#[test]
fn ordering_comparison_is_not_top_eq_ne() {
    let analysis = facts_of("assert(retries > 0);\nassert(a === b);\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.assertions.len(),
        2,
        "two assertions, got {:?}",
        analysis.facts.assertions.len()
    );
    assert_eq!(
        analysis.facts.assertions[0].shape,
        AssertShape::Other,
        "numeric comparison stays silent"
    );
    assert!(
        !analysis.facts.assertions[0].comparison,
        "numeric comparison clears the comparison flag"
    );
    assert_eq!(
        analysis.facts.assertions[1].shape,
        AssertShape::TopEqNe,
        "strict equality stays TopEqNe"
    );
    assert!(
        analysis.facts.assertions[1].comparison,
        "equality marks comparison"
    );
}

#[test]
fn export_list_class_is_exported() {
    let analysis =
        facts_of("class Store { save() {} }\nexport { Store };\nclass Hidden { run() {} }\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let visibility = |want: &str| {
        analysis
            .facts
            .types
            .iter()
            .find(|typedef| typedef.name == want)
            .unwrap_or_else(|| panic!("has type {want}"))
            .visibility
    };
    assert_eq!(
        visibility("Store"),
        Visibility::Exported,
        "export list surfaces the class"
    );
    assert_eq!(
        visibility("Hidden"),
        Visibility::Private,
        "unexported class stays private"
    );
}

#[test]
fn side_effect_and_require_imports_carry_external_roots() {
    let analysis =
        facts_of("import 'core-js/stable';\nimport fs from 'fs';\nconst _ = require('lodash');\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let by_module = |module: &str| {
        analysis
            .facts
            .imports
            .iter()
            .find(|import| import.module == module)
            .unwrap_or_else(|| panic!("has import from {module}"))
    };
    let side = by_module("core-js/stable");
    assert_eq!(
        side.kind,
        ImportKind::SideEffect,
        "bare import is a side effect"
    );
    assert_eq!(
        side.external_root,
        Some("core-js"),
        "bare package names its external root"
    );
    let bound = by_module("lodash");
    assert_eq!(
        bound.kind,
        ImportKind::Namespace,
        "bound require is a namespace import"
    );
    assert_eq!(
        bound.external_root,
        Some("lodash"),
        "require names its external root"
    );
    assert_eq!(
        by_module("fs").external_root,
        Some("fs"),
        "bare node builtin keeps its root per ruling 10"
    );
}

#[test]
fn same_name_alias_is_none() {
    let analysis =
        facts_of("import { join as join } from 'path';\nimport { readFile as read } from 'fs';\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let by_name = |name: &str| {
        analysis
            .facts
            .imports
            .iter()
            .find(|import| import.name == name)
            .unwrap_or_else(|| panic!("has import {name}"))
    };
    assert_eq!(
        by_name("join").alias,
        None,
        "identity rename records no alias"
    );
    assert_eq!(
        by_name("readFile").alias,
        Some("read"),
        "true rename keeps its alias"
    );
}

#[test]
fn new_expression_reports_bare_root() {
    let analysis = facts_of("const p = new Parser(opts);\nconst q = a.b();\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let new = analysis
        .facts
        .calls
        .iter()
        .find(|call| call.kind == CallKind::New)
        .expect("has the constructor call");
    assert_eq!(
        new.callee_root,
        Some("Parser"),
        "bare constructor names its root"
    );
    let plain = analysis
        .facts
        .calls
        .iter()
        .find(|call| call.kind == CallKind::Plain)
        .expect("has the member call");
    assert_eq!(
        plain.callee_root,
        Some("a"),
        "member call keeps its leftmost root"
    );
    assert!(
        plain.is_simple_path,
        "dotted member call stays a simple path"
    );
}

#[test]
fn commonjs_exports_surface_as_exported() {
    let analysis = facts_of(
        "function parse(src) { return src.trim(); }\nmodule.exports = { parse };\nfunction hidden() {}\nexports.help = hidden;\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let visibility = |want: &str| {
        analysis
            .facts
            .functions
            .iter()
            .find(|function| function.name == want)
            .unwrap_or_else(|| panic!("has function {want}"))
            .visibility
    };
    assert_eq!(
        visibility("parse"),
        Visibility::Exported,
        "module.exports shorthand surfaces the function"
    );
    assert_eq!(
        visibility("hidden"),
        Visibility::Private,
        "unexported binding name stays private"
    );
}

#[test]
fn class_heritage_records_base_not_self() {
    let analysis = facts_of("class Circle extends Shape {}\nclass Plain {}\n");
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    assert_eq!(
        analysis.facts.impls.len(),
        1,
        "one heritage impl, got {:?}",
        analysis.facts.impls
    );
    assert_eq!(
        analysis.facts.impls[0].name, "Shape",
        "the base records the impl, not the class itself"
    );
}

#[test]
fn require_destructuring_binds_named_imports() {
    let analysis = facts_of(
        "const { readFile } = require('fs');\nconst { join: joinPath, sep: separator = '/' } = require('path');\nconst tool = require('tool');\nrequire('setup');\n",
    );
    assert!(
        analysis.parse_error.is_none(),
        "parses, got {:?}",
        analysis.parse_error
    );
    let by_module = |module: &str| {
        analysis
            .facts
            .imports
            .iter()
            .find(|import| import.module == module)
            .unwrap_or_else(|| panic!("has import from {module}"))
    };
    let destructured = by_module("fs");
    assert_eq!(
        destructured.kind,
        ImportKind::Named,
        "destructured require binds names"
    );
    assert_eq!(
        destructured.name, "readFile",
        "destructured require keeps the binding name"
    );
    let renamed = analysis
        .facts
        .imports
        .iter()
        .find(|import| import.module == "path" && import.name == "join")
        .unwrap_or_else(|| panic!("has renamed import from path"));
    assert_eq!(
        renamed.kind,
        ImportKind::Named,
        "renamed destructuring binds a name"
    );
    assert_eq!(
        renamed.alias,
        Some("joinPath"),
        "renamed destructuring keeps its alias"
    );
    let defaulted = analysis
        .facts
        .imports
        .iter()
        .find(|import| import.module == "path" && import.name == "sep")
        .unwrap_or_else(|| panic!("has defaulted import from path"));
    assert_eq!(
        defaulted.kind,
        ImportKind::Named,
        "defaulted destructuring still binds"
    );
    assert_eq!(
        defaulted.alias,
        Some("separator"),
        "defaulted destructuring keeps its local name"
    );
    let bound = by_module("tool");
    assert_eq!(
        bound.kind,
        ImportKind::Namespace,
        "bound require stays a namespace import"
    );
    let bare = by_module("setup");
    assert_eq!(
        bare.kind,
        ImportKind::SideEffect,
        "unbound require stays a side effect"
    );
}
