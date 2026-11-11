use sloplint_core::facts::Pos;
use tree_sitter::Node;

use super::has_mod;
use crate::hooks::{Cx, child_by_field, named_children, pos_of, text};

pub(crate) fn decl_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    match n.kind() {
        "declaration_expression" => push_out_name(cx, n, out),
        "declaration_pattern" | "var_pattern" | "recursive_pattern" => {
            collect_designation(cx, n, out);
        }
        "local_declaration_statement" => push_local_names(cx, n, out),
        _ => {}
    }
}
fn push_out_name<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let Some(name) = child_by_field(n, "name") else {
        return;
    };
    if name.kind() != "identifier" {
        return;
    }
    out.push((text(cx, name), pos_of(name)));
}
fn is_tuple_like(kind: &str) -> bool {
    matches!(kind, "tuple_pattern" | "parenthesized_variable_designation")
}
fn is_pattern_node(kind: &str) -> bool {
    matches!(
        kind,
        "declaration_pattern" | "var_pattern" | "recursive_pattern"
    )
}
fn is_pattern_link(kind: &str) -> bool {
    matches!(
        kind,
        "parenthesized_variable_designation"
            | "declaration_pattern"
            | "var_pattern"
            | "recursive_pattern"
            | "tuple_pattern"
    )
}
fn designation_child<'s, 't>(
    cx: &Cx<'_, 's>,
    cur: Node<'t>,
    out: &mut Vec<(&'s str, Pos)>,
    stack: &mut Vec<Node<'t>>,
) {
    if is_tuple_like(cur.kind()) {
        for child in named_children(cur) {
            if child.kind() == "identifier" {
                out.push((text(cx, child), pos_of(child)));
            } else if child.kind() != "discard" {
                stack.push(child);
            }
        }
        return;
    }
    if !is_pattern_node(cur.kind()) {
        return;
    }
    if let Some(name) = child_by_field(cur, "name")
        && name.kind() == "identifier"
    {
        out.push((text(cx, name), pos_of(name)));
    }
    for child in named_children(cur) {
        if is_pattern_link(child.kind()) {
            stack.push(child);
        }
    }
}
fn collect_designation<'s, 't>(cx: &Cx<'_, 's>, n: Node<'t>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        designation_child(cx, cur, out, &mut stack);
    }
}
fn push_local_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if has_mod(cx, n, "const") {
        return;
    }
    for decl in named_children(n) {
        if decl.kind() != "variable_declaration" {
            continue;
        }
        for d in named_children(decl) {
            if d.kind() != "variable_declarator" {
                continue;
            }
            if decl_tuple(cx, d, out) {
                continue;
            }
            if let Some(nm) = child_by_field(d, "name") {
                out.push((text(cx, nm), pos_of(nm)));
            }
        }
    }
}
fn decl_tuple<'s>(cx: &Cx<'_, 's>, d: Node<'_>, out: &mut Vec<(&'s str, Pos)>) -> bool {
    let mut tupled = false;
    for t in named_children(d) {
        if t.kind() == "tuple_pattern" {
            tupled = true;
            tuple_names(cx, t, out);
        }
    }
    tupled
}
fn tuple_names<'s>(cx: &Cx<'_, 's>, t: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut pending: Vec<Node<'_>> = named_children(t).into_iter().rev().collect();
    while let Some(c) = pending.pop() {
        if c.kind() == "tuple_pattern" {
            pending.extend(named_children(c).into_iter().rev());
        } else if c.kind() == "identifier" {
            out.push((text(cx, c), pos_of(c)));
        }
    }
}
