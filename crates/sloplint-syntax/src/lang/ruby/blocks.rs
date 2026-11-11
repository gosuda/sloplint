use sloplint_core::facts::Pos;
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, named_children, pos_of, text};

use super::field_text;

pub(crate) fn block_shadow_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    if n.kind() != "block" && n.kind() != "do_block" {
        return;
    }
    let Some(list) = child_by_field(n, "parameters") else {
        return;
    };
    let mut stack = named_children(list);
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "identifier" => out.push(text(cx, cur)),
            "destructured_parameter" => stack.extend(named_children(cur)),
            "forward_parameter" | "hash_splat_nil" => {}
            _ => {
                if let Some(name) = field_text(cx, cur, "name") {
                    out.push(name);
                }
            }
        }
    }
}

pub(crate) fn is_block_shadowed(cx: &Cx<'_, '_>, name: &str) -> bool {
    for anc in cx.ancestors() {
        match anc.kind() {
            "block" | "do_block" => {
                let mut shadowed = Vec::new();
                block_shadow_names(cx, anc, &mut shadowed);
                if shadowed.contains(&name) {
                    return true;
                }
            }
            "method" | "singleton_method" | "class" | "module" => return false,
            _ => {}
        }
    }
    false
}

pub(crate) fn flow_target_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "for" {
        if let Some(pattern) = child_by_field(n, "pattern") {
            push_pattern_target(cx, pattern, out);
        }
        return;
    }
    if n.kind() == "rescue"
        && let Some(variable) = child_by_field(n, "variable")
    {
        push_pattern_target(cx, variable, out);
    }
}

fn push_pattern_target<'s>(cx: &Cx<'_, 's>, root: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "left_assignment_list"
            | "destructured_left_assignment"
            | "rest_assignment"
            | "exception_variable" => stack.extend(named_children(cur)),
            _ => push_target_name(cx, cur, out),
        }
    }
}

fn push_target_name<'s>(cx: &Cx<'_, 's>, node: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if !matches!(
        node.kind(),
        "identifier" | "instance_variable" | "global_variable" | "class_variable"
    ) {
        return;
    }
    let name = text(cx, node);
    if !is_block_shadowed(cx, name) {
        out.push((name, pos_of(node)));
    }
}

pub(crate) fn op_write_target<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() != "operator_assignment" {
        return;
    }
    if let Some(left) = child_by_field(n, "left") {
        push_target_name(cx, left, out);
    }
}
