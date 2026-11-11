use sloplint_core::facts::{AmbientKind, Pos};
use tree_sitter::Node;

use super::calls::{incr_write_root, infix_write_root};
use super::funcs::pattern_names;
use super::params::{is_let_binding, is_parameter};
use crate::hooks::{
    Cx, FileSetKind, any_named_child, child_by_field, has_child_kind, named_children, pos_of, text,
};

fn is_ref_callee(cx: &Cx<'_, '_>, f: Node<'_>) -> bool {
    if f.kind() != "value_path" {
        return false;
    }
    text(cx, f) == "ref" || text(cx, f) == "Stdlib.ref"
}

fn is_ref_binding(cx: &Cx<'_, '_>, lb: Node<'_>) -> bool {
    if any_named_child(lb, |c| is_parameter(&c)) {
        return false;
    }
    let body = child_by_field(lb, "body");
    if !body.is_some_and(|b| b.kind() == "application_expression") {
        return false;
    }
    body.and_then(|b| child_by_field(b, "function"))
        .is_some_and(|f| is_ref_callee(cx, f))
}

pub(crate) fn decl_names_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "value_definition" {
        return;
    }
    let mutable = has_child_kind(n, "mutable");
    let bindings: Vec<Node<'_>> = named_children(n)
        .into_iter()
        .filter(is_let_binding)
        .collect();
    for lb in bindings {
        if !mutable && !is_ref_binding(cx, lb) {
            continue;
        }
        if let Some(pat) = child_by_field(lb, "pattern") {
            pattern_names(cx, pat, out);
        }
    }
}

fn push_let_pattern<'s>(cx: &Cx<'_, 's>, lb: Node<'_>, out: &mut Vec<&'s str>) {
    if !is_let_binding(&lb) {
        return;
    }
    if let Some(pattern) = child_by_field(lb, "pattern") {
        let mut found = Vec::new();
        pattern_names(cx, pattern, &mut found);
        out.extend(found.into_iter().map(|(name, _)| name));
    }
}
pub(crate) fn shadow_names_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<&'s str>) {
    if n.kind() == "match_case" {
        if let Some(pattern) = child_by_field(n, "pattern") {
            let mut found = Vec::new();
            pattern_names(cx, pattern, &mut found);
            out.extend(found.into_iter().map(|(name, _)| name));
        }
        return;
    }
    if n.kind() != "let_expression" {
        return;
    }
    for value in named_children(n) {
        if value.kind() != "value_definition" {
            continue;
        }
        for lb in named_children(value) {
            push_let_pattern(cx, lb, out);
        }
    }
}

fn push_ref_mutable_bindings<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
    mutable: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    for lb in named_children(n) {
        if !is_let_binding(&lb) {
            continue;
        }
        if !mutable && !is_ref_binding(cx, lb) {
            continue;
        }
        if let Some(pat) = child_by_field(lb, "pattern") {
            let mut found = Vec::new();
            pattern_names(cx, pat, &mut found);
            out.extend(
                found
                    .into_iter()
                    .map(|(name, pos)| (AmbientKind::GlobalMutable, name, pos)),
            );
        }
    }
}

fn is_env_callee(callee: &str) -> bool {
    callee == "Sys.getenv"
        || callee == "Sys.getenv_opt"
        || callee == "Unix.getenv"
        || callee == "Unix.environment"
}

fn file_global_names<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for value in named_children(root) {
        if value.kind() != "value_definition" {
            continue;
        }
        let mutable = has_child_kind(value, "mutable");
        for lb in named_children(value) {
            if !is_let_binding(&lb) {
                continue;
            }
            if !mutable && !is_ref_binding(cx, lb) {
                continue;
            }
            if let Some(pattern) = child_by_field(lb, "pattern") {
                let mut found = Vec::new();
                pattern_names(cx, pattern, &mut found);
                names.extend(found.into_iter().map(|(name, _)| name));
            }
        }
    }
    names
}

pub(crate) fn ambient_head<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() == "value_definition" {
        if in_function {
            return;
        }
        push_ref_mutable_bindings(cx, n, has_child_kind(n, "mutable"), out);
        return;
    }
    if n.kind() == "infix_expression" {
        if !in_function {
            return;
        }
        if let Some((root, pos)) = infix_write_root(cx, n)
            && root != "_"
            && cx
                .file_names(FileSetKind::Scope, || file_global_names(cx))
                .contains(&root)
        {
            out.push((AmbientKind::GlobalWrite, root, pos));
        }
        return;
    }
    if n.kind() != "application_expression" {
        return;
    }
    let Some(func) = child_by_field(n, "function") else {
        return;
    };
    let callee = text(cx, func);
    if callee == "incr" || callee == "decr" {
        if let Some((root, pos)) = incr_write_root(cx, n)
            && root != "_"
            && cx
                .file_names(FileSetKind::Scope, || file_global_names(cx))
                .contains(&root)
        {
            out.push((AmbientKind::GlobalWrite, root, pos));
        }
        return;
    }
    if is_env_callee(callee) {
        let read = (AmbientKind::EnvRead, callee, pos_of(n));
        out.push(read);
    }
}
