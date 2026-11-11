use sloplint_core::facts::{AmbientKind, Pos};
use tree_sitter::Node;

use crate::hooks::{Cx, first_named_child, first_named_child_matching, pos_of, text, trimmed};

use super::funcs::ps_var_name;

fn is_plain_ps_var(t: &str) -> bool {
    if !t.starts_with('$') {
        return false;
    }
    let s = ps_var_name(t);
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
    {
        return false;
    }
    !s.starts_with("env:")
        && !s.starts_with("global:")
        && !s.starts_with("script:")
        && !s.starts_with("using:")
        && !s.starts_with("local:")
}

pub(crate) fn ps_decl_names<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    if n.kind() == "foreach_statement" {
        if let Some(var) = first_named_child_matching(n, &["variable"]) {
            let t = text(cx, var);
            if is_plain_ps_var(t) {
                out.push((ps_var_name(t), pos_of(var)));
            }
        }
        return;
    }
    if n.kind() != "assignment_expression" {
        return;
    }
    let Some(left) = first_named_child_matching(n, &["left_assignment_expression"]) else {
        return;
    };
    let t = trimmed(cx, left);
    if is_plain_ps_var(t) {
        out.push((ps_var_name(t), pos_of(left)));
    }
}

fn write_operand_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut current = match first_named_child(n) {
        Some(first) => first,
        None => return,
    };
    loop {
        match current.kind() {
            "variable" => {
                out.push((ps_var_name(text(cx, current)), pos_of(current)));
                return;
            }
            "unary_expression"
            | "member_access"
            | "element_access"
            | "parenthesized_expression" => match first_named_child(current) {
                Some(next) => current = next,
                None => return,
            },
            _ => return,
        }
    }
}

pub(crate) fn ps_write_roots<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<(&'s str, Pos)>) {
    if matches!(
        n.kind(),
        "pre_increment_expression"
            | "post_increment_expression"
            | "pre_decrement_expression"
            | "post_decrement_expression"
    ) {
        write_operand_root(cx, n, out);
    }
}

fn ps_write_target(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let mut current = n;
    for _ in 0..12 {
        let Some(parent) = cx.parent_of(current) else {
            return false;
        };
        match parent.kind() {
            "left_assignment_expression" => return true,
            "pre_increment_expression"
            | "post_increment_expression"
            | "pre_decrement_expression"
            | "post_decrement_expression" => return true,
            "assignment_expression" => return false,
            _ => current = parent,
        }
    }
    false
}

pub(crate) fn ps_ambient<'s>(
    n: Node<'_>,
    cx: &Cx<'_, 's>,
    in_function: bool,
    out: &mut Vec<(AmbientKind, &'s str, Pos)>,
) {
    if n.kind() != "variable" {
        return;
    }
    let t = text(cx, n);
    if t.starts_with("$env:") || t.starts_with("${env:") {
        out.push((AmbientKind::EnvRead, t, pos_of(n)));
        return;
    }
    let scoped = t.starts_with("$global:")
        || t.starts_with("${global:")
        || t.starts_with("$script:")
        || t.starts_with("${script:")
        || t.starts_with("$using:")
        || t.starts_with("${using:");
    if !scoped {
        return;
    }
    let kind = if !in_function {
        AmbientKind::GlobalMutable
    } else if ps_write_target(cx, n) {
        AmbientKind::GlobalWrite
    } else {
        return;
    };
    out.push((kind, t, pos_of(n)));
}
