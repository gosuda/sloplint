//! Bindings, writes, and ambient state for TypeScript.

use sloplint_core::facts::{AmbientKind, Pos};
use tree_sitter::Node;

use super::calls::ts_declared;
use super::funcs::top_level;
use crate::hooks::{Cx, child_by_field, code_children, find_named_child, pos_of, text};

fn is_write_target(parent: Option<Node<'_>>, n: Node<'_>) -> bool {
    let Some(parent) = parent else {
        return false;
    };
    match parent.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            child_by_field(parent, "left").is_some_and(|left| left.id() == n.id())
        }
        "update_expression" => {
            child_by_field(parent, "argument").is_some_and(|argument| argument.id() == n.id())
        }
        _ => false,
    }
}

fn is_env_base(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    if n.kind() != "member_expression" {
        return false;
    }
    let (Some(object), Some(property)) =
        (child_by_field(n, "object"), child_by_field(n, "property"))
    else {
        return false;
    };
    if text(cx, property) != "env" {
        return false;
    }
    match object.kind() {
        "identifier" => text(cx, object) == "process",
        "meta_property" => text(cx, object) == "import.meta",
        _ => false,
    }
}

fn env_rooted(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        if is_env_base(cx, current) {
            return true;
        }
        match current.kind() {
            "member_expression" | "subscript_expression" => {
                let Some(object) = child_by_field(current, "object") else {
                    return false;
                };
                current = object;
            }
            _ => return false,
        }
    }
}

fn under_env_store(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let ancestors: Vec<Node<'_>> = cx.ancestors().collect();
    let mut child = n;
    for (index, node) in ancestors.iter().enumerate() {
        match node.kind() {
            "member_expression" | "subscript_expression" => {
                if !child_by_field(*node, "object").is_some_and(|object| object.id() == child.id())
                {
                    return false;
                }
                if is_write_target(ancestors.get(index + 1).copied(), *node) {
                    return true;
                }
                child = *node;
            }
            _ => return false,
        }
    }
    false
}

fn file_mutables<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for stmt in code_children(root) {
        collect_top_mutable(cx, stmt, &mut out);
    }
    out
}

fn collect_top_mutable<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, out: &mut Vec<&'s str>) {
    let decl = if stmt.kind() == "export_statement" {
        find_named_child(stmt, |child| {
            child.kind() == "lexical_declaration" || child.kind() == "variable_declaration"
        })
    } else if stmt.kind() == "lexical_declaration" || stmt.kind() == "variable_declaration" {
        Some(stmt)
    } else {
        None
    };
    let Some(decl) = decl else {
        return;
    };
    if decl.kind() == "lexical_declaration"
        && child_by_field(decl, "kind").map(|kind| text(cx, kind)) != Some("let")
    {
        return;
    }
    out.extend(ts_declared(cx, decl).into_iter().map(|(name, _)| name));
}

pub(crate) fn decl_names_for<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "lexical_declaration"
        && !child_by_field(n, "kind").is_some_and(|k| text(cx, k) == "let")
    {
        return;
    }
    if n.kind() == "using_declaration" {
        return;
    }
    out.extend(ts_declared(cx, n));
}

pub(crate) fn write_roots_for<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    let target = if n.kind() == "update_expression" {
        child_by_field(n, "argument")
    } else {
        child_by_field(n, "left")
    };
    if let Some(t) = target
        && let Some(root) = super::calls::ts_root(cx, t)
    {
        out.push(root);
    }
}

pub(crate) fn ambient_sites<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    match n.kind() {
        "lexical_declaration" | "variable_declaration" => {
            push_declaration_global(cx, n, in_function, out);
        }
        "member_expression" => push_env_or_store(cx, n, out),
        "subscript_expression" => {
            if is_write_target(cx.parent(), n) && env_rooted(cx, n) {
                out.push((AmbientKind::GlobalWrite, text(cx, n), pos_of(n)));
            }
        }
        "identifier"
            if in_function
                && is_write_target(cx.parent(), n)
                && file_mutables(cx).contains(&text(cx, n)) =>
        {
            out.push((AmbientKind::GlobalWrite, text(cx, n), pos_of(n)));
        }
        _ => {}
    }
}

fn push_declaration_global<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if in_function || !top_level(cx) {
        return;
    }
    if n.kind() == "lexical_declaration" || n.kind() == "variable_declaration" {
        let mutable = n.kind() == "variable_declaration"
            || child_by_field(n, "kind").is_some_and(|k| text(cx, k) == "let");
        if mutable {
            for (name, pos) in ts_declared(cx, n) {
                out.push((AmbientKind::GlobalMutable, name, pos));
            }
        }
    }
}

fn push_env_or_store<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    if is_env_base(cx, n) && !is_write_target(cx.parent(), n) && !under_env_store(cx, n) {
        out.push((AmbientKind::EnvRead, text(cx, n), pos_of(n)));
        return;
    }
    if is_write_target(cx.parent(), n) && env_rooted(cx, n) {
        out.push((AmbientKind::GlobalWrite, text(cx, n), pos_of(n)));
    }
}
