use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope};
use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, first_named_child, first_named_child_matching, pos_of, text,
};

use super::calls::{decl_value, is_import_builtin};

fn import_module<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    let found = first_named_child_matching(n, &["string_content"]);
    found.map(|c| text(cx, c)).unwrap_or("")
}

fn import_names<'s>(
    cx: &Cx<'_, 's>,
    builtin: Node<'_>,
) -> Option<(&'s str, Option<&'s str>, ImportKind)> {
    let holder = cx.parent_of(builtin)?;
    let value = decl_value(holder);
    if holder.kind() == "variable_declaration" && value.is_some_and(|e| e.id() == builtin.id()) {
        let id = first_named_child_matching(holder, &["identifier"])?;
        return Some((text(cx, id), None, ImportKind::Namespace));
    }
    let object = child_by_field(holder, "object");
    if holder.kind() != "field_expression" || !object.is_some_and(|o| o.id() == builtin.id()) {
        return None;
    }
    let member = child_by_field(holder, "member")
        .map(|m| text(cx, m))
        .unwrap_or("");
    let decl = cx.parent_of(holder)?;
    if decl.kind() != "variable_declaration" {
        return None;
    }
    let binding = first_named_child_matching(decl, &["identifier"]).map(|c| text(cx, c));
    let alias = match binding {
        Some(found) if found != member => Some(found),
        _ => None,
    };
    Some((member, alias, ImportKind::Named))
}

fn push_import_string<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(args) = cx.parent() else { return };
    if args.kind() != "arguments" {
        return;
    }
    let Some(builtin) = cx.parent_of(args) else {
        return;
    };
    if builtin.kind() != "builtin_function" || !is_import_builtin(cx, builtin) {
        return;
    }
    let parent = cx.parent_of(builtin);
    if parent.is_some_and(|p| p.kind() == "using_namespace_declaration") {
        return;
    }
    let module = import_module(cx, n);
    let shape = import_names(cx, builtin);
    let (name, alias, kind) = shape.unwrap_or(("", None, ImportKind::SideEffect));
    let pos = pos_of(n);
    out.push(Import {
        pos,
        module: Cow::Borrowed(module),
        name,
        alias,
        kind,
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
        external_root: None,
    });
}

fn push_using<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Import<'s>>) {
    let Some(expr) = first_named_child(n) else {
        return;
    };
    if expr.kind() != "builtin_function" || !is_import_builtin(cx, expr) {
        return;
    }
    let Some(args) = first_named_child_matching(expr, &["arguments"]) else {
        return;
    };
    let found = first_named_child_matching(args, &["string"]);
    let Some(literal) = found else { return };
    if import_module(cx, literal).is_empty() {
        return;
    }
    let module = import_module(cx, literal);
    let pos = pos_of(n);
    out.push(Import {
        pos,
        module: Cow::Borrowed(module),
        name: "",
        alias: None,
        kind: ImportKind::Wildcard,
        scope: ImportScope::Module,
        in_test: false,
        is_reexport: false,
        external_root: None,
    });
}

pub(crate) fn zig_imports<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
    if n.kind() == "using_namespace_declaration" {
        push_using(cx, n, out);
    } else if n.kind() == "string" {
        push_import_string(cx, n, out);
    }
}
