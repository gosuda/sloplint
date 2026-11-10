//! Cpp import facts: includes, using declarations, and namespace aliases.

use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope, Pos};
use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, first_named_child_matching, pos_of, rfind_named_child, strip_quotes, text,
};
use crate::lang::c_family::{angles, flat, in_function};

fn import_fact<'s>(
    pos: Pos,
    module: Cow<'s, str>,
    name: &'s str,
    alias: Option<&'s str>,
    kind: ImportKind,
    scope: ImportScope,
) -> Import<'s> {
    Import {
        pos,
        module,
        name,
        alias,
        kind,
        scope,
        in_test: false,
        is_reexport: false,
        external_root: None,
    }
}

fn fn_scope(cx: &Cx<'_, '_>) -> ImportScope {
    if in_function(cx) {
        ImportScope::FunctionLocal
    } else {
        ImportScope::Module
    }
}

pub(crate) fn push_include<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(path) = child_by_field(n, "path") else {
        return;
    };
    let module = match path.kind() {
        "string_literal" => strip_quotes(text(cx, path)),
        "system_lib_string" => angles(text(cx, path)),
        _ => text(cx, path),
    };
    out.push(import_fact(
        pos_of(n),
        Cow::Borrowed(module),
        "",
        None,
        ImportKind::SideEffect,
        ImportScope::Module,
    ));
}

pub(crate) fn scope_of(path: &str) -> (&str, &str) {
    match path.rfind("::") {
        Some(at) => (&path[..at], &path[at + 2..]),
        None => ("", path),
    }
}

pub(crate) fn push_using<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(target) = first_named_child_matching(n, &["identifier", "qualified_identifier"])
    else {
        return;
    };
    if flat(text(cx, n)).starts_with("usingnamespace") {
        out.push(import_fact(
            pos_of(n),
            Cow::Borrowed(text(cx, target)),
            "",
            None,
            ImportKind::Wildcard,
            fn_scope(cx),
        ));
        return;
    }
    if (0..n.child_count()).any(|i| n.child(i).is_some_and(|c| c.kind() == "enum")) {
        out.push(import_fact(
            pos_of(n),
            Cow::Borrowed(""),
            text(cx, target),
            None,
            ImportKind::Named,
            fn_scope(cx),
        ));
        return;
    }
    let (module, name) = scope_of(text(cx, target));
    out.push(import_fact(
        pos_of(n),
        Cow::Borrowed(module),
        name,
        None,
        ImportKind::Named,
        fn_scope(cx),
    ));
}

pub(crate) fn push_alias<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    out: &mut Vec<Import<'s>>,
) -> Option<()> {
    let alias_node = child_by_field(n, "name")?;
    let target = rfind_named_child(n, |c| {
        c.kind() == "namespace_identifier" || c.kind() == "nested_namespace_specifier"
    })?;
    let (module, name) = scope_of(text(cx, target));
    out.push(import_fact(
        pos_of(n),
        Cow::Borrowed(module),
        name,
        Some(text(cx, alias_node)),
        ImportKind::Named,
        fn_scope(cx),
    ));
    Some(())
}
