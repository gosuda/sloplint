//! Bindings, writes, and ambient state for TypeScript.

use sloplint_core::facts::{AmbientKind, Pos};
use tree_sitter::Node;

use super::calls::{ts_declared, ts_pat, ts_root};
use super::funcs::{closure_shadow_params, top_level};

use crate::hooks::{
    Cx, FileSetKind, child_by_field, code_children, find_named_child, named_children, text,
};
use crate::lang::javascript::ambient::push_env_or_store;

fn file_mutables<'a, 's>(cx: &'a Cx<'_, 's>) -> &'a [&'s str] {
    cx.file_names(FileSetKind::Scope, || {
        let mut out = Vec::new();
        if let Some(root) = cx.root() {
            for stmt in code_children(root) {
                collect_top_mutable(cx, stmt, &mut out);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    })
}

fn is_file_mutable(cx: &Cx<'_, '_>, name: &str) -> bool {
    file_mutables(cx).binary_search(&name).is_ok()
}

fn is_fn_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression"
            | "generator_function"
            | "class_static_block"
    )
}

pub(crate) fn hoisted_shadows_for<'s>(cx: &Cx<'_, 's>, body: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        for child in named_children(node) {
            hoist_child(cx, child, &mut stack, out);
        }
    }
}

fn hoist_child<'s, 't>(
    cx: &Cx<'_, 's>,
    child: Node<'t>,
    stack: &mut Vec<Node<'t>>,
    out: &mut Vec<&'s str>,
) {
    if is_fn_like(child.kind()) {
        return;
    }
    if child.kind() == "variable_declaration" {
        out.extend(ts_declared(cx, child).into_iter().map(|(name, _)| name));
        return;
    }
    if child.kind() == "for_in_statement" {
        hoist_for_in(cx, child, out);
    }
    if child.child_count() > 0 {
        stack.push(child);
    }
}

fn hoist_for_in<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if !child_by_field(n, "kind").is_some_and(|kind| text(cx, kind) == "var") {
        return;
    }
    if let Some(left) = child_by_field(n, "left") {
        let mut found = Vec::new();
        ts_pat(cx, left, &mut found);
        out.extend(found.into_iter().map(|(name, _)| name));
    }
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
    if n.kind() == "for_in_statement" {
        for_in_var_names(cx, n, out);
        return;
    }
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

fn for_in_var_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if !child_by_field(n, "kind").is_some_and(|kind| text(cx, kind) == "var") {
        return;
    }
    if let Some(left) = child_by_field(n, "left") {
        ts_pat(cx, left, out);
    }
}

pub(crate) fn shadow_kind_for(kind: &str) -> bool {
    kind == "lexical_declaration" || kind == "for_in_statement" || kind == "catch_clause"
}

pub(crate) fn function_body_depth(depth: usize, cx: &Cx<'_, '_>) -> usize {
    let mut parent = depth;
    for ancestor in cx.ancestors() {
        parent = parent.saturating_sub(1);
        if is_fn_like(ancestor.kind()) {
            return parent.saturating_add(1);
        }
    }
    depth.saturating_sub(1)
}

pub(crate) fn shadow_names_for<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if n.kind() == "catch_clause" {
        if let Some(param) = child_by_field(n, "parameter") {
            let mut found = Vec::new();
            ts_pat(cx, param, &mut found);
            out.extend(found.into_iter().map(|(name, _)| name));
        }
        return;
    }
    if n.kind() == "for_in_statement" {
        shadow_for_in_names(cx, n, out);
        return;
    }
    if is_fn_like(n.kind()) {
        out.extend(closure_shadow_params(cx, n));
        if let Some(body) = child_by_field(n, "body") {
            hoisted_shadows_for(cx, body, out);
        }
        return;
    }
    if !child_by_field(n, "kind").is_some_and(|kind| text(cx, kind) == "const") {
        return;
    }
    out.extend(ts_declared(cx, n).into_iter().map(|(name, _)| name));
}

fn shadow_for_in_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if !child_by_field(n, "kind").is_some_and(|kind| {
        let keyword = text(cx, kind);
        keyword == "let" || keyword == "const"
    }) {
        return;
    }
    if let Some(left) = child_by_field(n, "left") {
        let mut found = Vec::new();
        ts_pat(cx, left, &mut found);
        out.extend(found.into_iter().map(|(name, _)| name));
    }
}

pub(crate) fn write_roots_for<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    let target = if n.kind() == "update_expression" {
        child_by_field(n, "argument")
    } else {
        child_by_field(n, "left")
    };
    let Some(mut current) = target else {
        return;
    };
    while current.kind() == "parenthesized_expression" {
        let Some(inner) = find_named_child(current, |child| child.kind() != "comment") else {
            return;
        };
        current = inner;
    }
    let mut names = Vec::new();
    ts_pat(cx, current, &mut names);
    if names.is_empty() {
        names.extend(ts_root(cx, current));
    }
    out.extend(names);
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
        "member_expression" | "subscript_expression" => push_env_or_store(cx, n, out),
        "assignment_expression" | "augmented_assignment_expression" | "update_expression"
            if in_function =>
        {
            push_global_write(cx, n, out);
        }
        _ => {}
    }
}

fn push_global_write<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(AmbientKind, &'s str, Pos)>) {
    let target = match n.kind() {
        "assignment_expression" | "augmented_assignment_expression" => child_by_field(n, "left"),
        "update_expression" => child_by_field(n, "argument"),
        _ => None,
    };
    let Some(mut current) = target else {
        return;
    };
    while current.kind() == "parenthesized_expression" {
        let Some(inner) = find_named_child(current, |child| child.kind() != "comment") else {
            return;
        };
        current = inner;
    }
    let mut names = Vec::new();
    ts_pat(cx, current, &mut names);
    if names.is_empty() {
        names.extend(ts_root(cx, current));
    }
    for (name, pos) in names {
        if is_file_mutable(cx, name) {
            out.push((AmbientKind::GlobalWrite, name, pos));
        }
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
