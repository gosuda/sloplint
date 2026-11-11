use tree_sitter::Node;

use sloplint_core::facts::Pos;

use crate::hooks::{Cx, FileSetKind, child_by_field, named_children, pos_of, text};

use super::{enclosing_function, py_pat_names, py_root, py_with_targets};

fn stmt_declares(cx: &Cx<'_, '_>, stmt: Node<'_>, kind: &str, want: &str) -> bool {
    if stmt.kind() != kind {
        return false;
    }
    named_children(stmt)
        .iter()
        .any(|cand| cand.kind() == "identifier" && text(cx, *cand) == want)
}

fn collect_outer_names<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, out: &mut Vec<&'s str>) {
    if !matches!(stmt.kind(), "nonlocal_statement" | "global_statement") {
        return;
    }
    for cand in named_children(stmt) {
        if cand.kind() == "identifier" {
            out.push(text(cx, cand));
        }
    }
}

fn file_outer_names<'a, 's>(cx: &'a Cx<'_, 's>) -> &'a [&'s str] {
    cx.file_names(FileSetKind::Scope, || {
        let mut out = Vec::new();
        let Some(root) = cx.root() else {
            return out;
        };
        let mut stack = vec![root];
        while let Some(cur) = stack.pop() {
            collect_outer_names(cx, cur, &mut out);
            stack.extend(named_children(cur));
        }
        out.sort_unstable();
        out.dedup();
        out
    })
}

fn body_declares(cx: &Cx<'_, '_>, func: Node<'_>, kind: &str, want: &str) -> bool {
    if !file_outer_names(cx).contains(&want) {
        return false;
    }
    let Some(body) = child_by_field(func, "body") else {
        return false;
    };
    let mut stack = vec![body];
    while let Some(cur) = stack.pop() {
        if matches!(
            cur.kind(),
            "function_definition" | "lambda" | "class_definition"
        ) {
            continue;
        }
        if stmt_declares(cx, cur, kind, want) {
            return true;
        }
        stack.extend(named_children(cur));
    }
    false
}

fn enclosing_declares(cx: &Cx<'_, '_>, kind: &str, want: &str) -> bool {
    enclosing_function(cx).is_some_and(|func| body_declares(cx, func, kind, want))
}

pub(crate) fn nonlocal_target(cx: &Cx<'_, '_>, want: &str) -> bool {
    enclosing_declares(cx, "nonlocal_statement", want)
}

pub(crate) fn global_target(cx: &Cx<'_, '_>, want: &str) -> bool {
    enclosing_declares(cx, "global_statement", want)
}

fn keep_local(cx: &Cx<'_, '_>, want: &str) -> bool {
    !nonlocal_target(cx, want) && !global_target(cx, want)
}

fn push_locals<'s>(cx: &Cx<'_, 's>, names: Vec<(&'s str, Pos)>, out: &mut Vec<(&'s str, Pos)>) {
    for (name, pos) in names {
        if keep_local(cx, name) {
            out.push((name, pos));
        }
    }
}

fn push_outers<'s>(cx: &Cx<'_, 's>, names: Vec<(&'s str, Pos)>, out: &mut Vec<(&'s str, Pos)>) {
    for (name, pos) in names {
        if nonlocal_target(cx, name) {
            out.push((name, pos));
        }
    }
}

fn push_single_outer<'s>(cx: &Cx<'_, 's>, node: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let name = text(cx, node);
    if nonlocal_target(cx, name) {
        out.push((name, pos_of(node)));
    }
}

pub(crate) fn decl_targets<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    let mut names = Vec::new();
    py_pat_names(cx, left, &mut names);
    push_locals(cx, names, out);
}

pub(crate) fn decl_with<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut names = Vec::new();
    py_with_targets(cx, n, &mut names);
    push_locals(cx, names, out);
}

pub(crate) fn decl_walrus<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let Some(name) = child_by_field(n, "name") else {
        return;
    };
    if name.kind() != "identifier" {
        return;
    }
    let found = text(cx, name);
    if keep_local(cx, found) {
        out.push((found, pos_of(name)));
    }
}

pub(crate) fn write_assignment<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    if left.kind() == "identifier" {
        push_single_outer(cx, left, out);
        return;
    }
    if matches!(left.kind(), "attribute" | "subscript")
        && let Some(root) = py_root(cx, left)
        && !global_target(cx, root.0)
    {
        out.push(root);
    }
    let mut names = Vec::new();
    py_pat_names(cx, left, &mut names);
    push_outers(cx, names, out);
}

pub(crate) fn write_augmented<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    if let Some(root) = py_root(cx, left)
        && !global_target(cx, root.0)
    {
        out.push(root);
    }
}

pub(crate) fn write_walrus<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let Some(name) = child_by_field(n, "name") else {
        return;
    };
    if name.kind() != "identifier" {
        return;
    }
    push_single_outer(cx, name, out);
}

pub(crate) fn write_loop<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    let mut names = Vec::new();
    py_pat_names(cx, left, &mut names);
    push_outers(cx, names, out);
}

pub(crate) fn write_with<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut names = Vec::new();
    py_with_targets(cx, n, &mut names);
    push_outers(cx, names, out);
}

pub(crate) fn decl_except<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let Some(alias) = child_by_field(n, "alias") else {
        return;
    };
    let mut names = Vec::new();
    py_pat_names(cx, alias, &mut names);
    push_locals(cx, names, out);
}
