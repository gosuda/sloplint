use tree_sitter::Node;

use sloplint_core::facts::Pos;

use crate::hooks::{Cx, child_by_field, named_children, pos_of, text};

fn push_var<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let raw = text(cx, n);
    let name = raw.strip_prefix('$').unwrap_or(raw);
    if !name.is_empty() {
        out.push((name, pos_of(n)));
    }
}

pub(crate) fn flow_targets<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "catch_clause" {
        if let Some(name) = child_by_field(n, "name") {
            push_var(cx, name, out);
        }
        return;
    }
    if n.kind() != "foreach_statement" {
        return;
    }
    let mut stack: Vec<Node<'_>> = named_children(n).into_iter().skip(1).collect();
    while let Some(current) = stack.pop() {
        if current.kind() == "variable_name" {
            push_var(cx, current, out);
        } else if matches!(current.kind(), "pair" | "list_literal" | "by_ref") {
            stack.extend(named_children(current));
        }
    }
}

pub(crate) fn captured_by_outer_ref(cx: &Cx<'_, '_>, want: &str) -> bool {
    for anc in cx.ancestors() {
        if anc.kind() == "anonymous_function" {
            return super::funcs::captures_by_ref(cx, anc, want);
        }
        if matches!(
            anc.kind(),
            "arrow_function" | "function_definition" | "method_declaration"
        ) {
            return false;
        }
    }
    false
}
pub(crate) fn inside_arrow(cx: &Cx<'_, '_>) -> bool {
    for anc in cx.ancestors() {
        if anc.kind() == "arrow_function" {
            return true;
        }
        if matches!(
            anc.kind(),
            "anonymous_function" | "function_definition" | "method_declaration"
        ) {
            return false;
        }
    }
    false
}
