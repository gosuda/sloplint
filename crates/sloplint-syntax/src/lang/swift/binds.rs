use tree_sitter::Node;

use sloplint_core::facts::Pos;

use crate::hooks::{Cx, child_by_field, named_children, pos_of, text, trimmed};

struct Binder<'s> {
    name: &'s str,
    pos: Pos,
    is_var: bool,
}

pub(crate) fn shadow_kind_for(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement" | "guard_statement" | "for_statement" | "switch_entry"
    )
}

pub(crate) fn shadow_names_for<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let mut found = Vec::new();
    collect_statement(cx, n, &mut found);
    out.extend(found.into_iter().map(|binder| binder.name));
}

pub(crate) fn decl_names_for<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "value_binding_pattern" || !is_var_binding(cx, n) {
        return;
    }
    let Some(parent) = cx.parent_of(n) else {
        return;
    };
    if parent.kind() == "property_declaration" {
        return;
    }
    let mut found = Vec::new();
    collect_vbp(cx, parent, n, &mut found);
    out.extend(
        found
            .into_iter()
            .filter(|binder| binder.is_var)
            .map(|binder| (binder.name, binder.pos)),
    );
}

pub(crate) fn scope_parent(depth: usize, node: Node<'_>, cx: &Cx<'_, '_>) -> usize {
    match node.kind() {
        "if_statement" | "for_statement" | "switch_entry" => depth,
        "guard_statement" => depth.saturating_sub(1),
        "value_binding_pattern" => binder_scope(depth, cx),
        _ => depth.saturating_sub(1),
    }
}

fn binder_scope(depth: usize, cx: &Cx<'_, '_>) -> usize {
    for (up, ancestor) in cx.ancestors().enumerate() {
        match ancestor.kind() {
            "if_statement" | "for_statement" | "switch_entry" => {
                return depth.saturating_sub(up + 1);
            }
            "guard_statement" => return depth.saturating_sub(up + 2),
            _ => {}
        }
    }
    depth.saturating_sub(1)
}

fn is_var_binding(cx: &Cx<'_, '_>, node: Node<'_>) -> bool {
    trimmed(cx, node) == "var"
}

fn collect_statement<'s>(cx: &Cx<'_, 's>, node: Node<'_>, out: &mut Vec<Binder<'s>>) {
    match node.kind() {
        "if_statement" | "guard_statement" => push_condition_names(cx, node, out),
        "for_statement" => {
            if let Some(item) = child_by_field(node, "item") {
                push_pattern_names(cx, item, None, out);
            }
        }
        "switch_entry" => {
            for child in named_children(node) {
                if child.kind() == "switch_pattern" {
                    push_pattern_names(cx, child, None, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_vbp<'s>(cx: &Cx<'_, 's>, parent: Node<'_>, vbp: Node<'_>, out: &mut Vec<Binder<'s>>) {
    match parent.kind() {
        "pattern" | "switch_pattern" => push_pattern_names(cx, parent, None, out),
        "if_statement" | "guard_statement" => push_sibling_names(cx, parent, vbp, out),
        _ => {}
    }
}

fn push_condition_names<'s>(cx: &Cx<'_, 's>, stmt: Node<'_>, out: &mut Vec<Binder<'s>>) {
    let mut govern: Option<bool> = None;
    for index in 0..stmt.child_count() {
        govern = push_condition_child(cx, stmt, index, govern, out);
    }
}

fn push_condition_child<'s>(
    cx: &Cx<'_, 's>,
    stmt: Node<'_>,
    index: u32,
    govern: Option<bool>,
    out: &mut Vec<Binder<'s>>,
) -> Option<bool> {
    let field = stmt.field_name_for_child(index);
    let Some(child) = stmt.child(index) else {
        return govern;
    };
    if !child.is_named() {
        return govern;
    }
    if child.kind() == "value_binding_pattern" {
        return Some(trimmed(cx, child) == "var");
    }
    push_bound_child(cx, child, field, govern, out);
    govern
}

fn push_sibling_names<'s>(
    cx: &Cx<'_, 's>,
    parent: Node<'_>,
    vbp: Node<'_>,
    out: &mut Vec<Binder<'s>>,
) {
    let mut after = false;
    for index in 0..parent.child_count() {
        let field = parent.field_name_for_child(index);
        let Some(child) = parent.child(index) else {
            continue;
        };
        if !after {
            after = child.id() == vbp.id();
            continue;
        }
        if !child.is_named() || child.kind() == "value_binding_pattern" {
            break;
        }
        push_bound_child(cx, child, field, Some(true), out);
    }
}

fn push_bound_child<'s>(
    cx: &Cx<'_, 's>,
    child: Node<'_>,
    field: Option<&str>,
    govern: Option<bool>,
    out: &mut Vec<Binder<'s>>,
) {
    match child.kind() {
        "pattern" | "switch_pattern" => push_pattern_names(cx, child, govern, out),
        "simple_identifier" if field == Some("bound_identifier") => {
            if let Some(is_var) = govern {
                out.push(Binder {
                    name: text(cx, child),
                    pos: pos_of(child),
                    is_var,
                });
            }
        }
        _ => {}
    }
}

fn push_pattern_names<'s>(
    cx: &Cx<'_, 's>,
    root: Node<'_>,
    govern: Option<bool>,
    out: &mut Vec<Binder<'s>>,
) {
    let mut stack = vec![(root, govern)];
    while let Some((current, govern)) = stack.pop() {
        if current.kind() == "switch_pattern" {
            push_switch_members(current, govern, &mut stack);
            continue;
        }
        if current.kind() != "pattern" {
            continue;
        }
        let govern = direct_mutability(cx, current).or(govern);
        let Some(govern) = govern else {
            continue;
        };
        push_pattern_children(cx, current, govern, &mut stack, out);
    }
}

fn push_switch_members<'t>(
    node: Node<'t>,
    govern: Option<bool>,
    stack: &mut Vec<(Node<'t>, Option<bool>)>,
) {
    for child in named_children(node) {
        if child.kind() == "pattern" {
            stack.push((child, govern));
        }
    }
}

fn push_pattern_children<'s, 't>(
    cx: &Cx<'_, 's>,
    node: Node<'t>,
    govern: bool,
    stack: &mut Vec<(Node<'t>, Option<bool>)>,
    out: &mut Vec<Binder<'s>>,
) {
    for index in 0..node.child_count() {
        let field = node.field_name_for_child(index);
        let Some(child) = node.child(index) else {
            continue;
        };
        if !child.is_named() {
            continue;
        }
        match child.kind() {
            "simple_identifier" if field == Some("bound_identifier") || field.is_none() => {
                out.push(Binder {
                    name: text(cx, child),
                    pos: pos_of(child),
                    is_var: govern,
                });
            }
            "pattern" | "switch_pattern" => stack.push((child, Some(govern))),
            _ => {}
        }
    }
}

fn direct_mutability(cx: &Cx<'_, '_>, node: Node<'_>) -> Option<bool> {
    named_children(node)
        .into_iter()
        .find(|child| child.kind() == "value_binding_pattern")
        .map(|binding| trimmed(cx, binding) == "var")
}
