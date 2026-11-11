use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, named_children, text};

fn tuple_names<'s, 'a>(
    cx: &Cx<'_, 's>,
    cur: Node<'a>,
    out: &mut Vec<&'s str>,
    stack: &mut Vec<Node<'a>>,
) {
    for c in named_children(cur) {
        if c.kind() == "identifier" {
            out.push(text(cx, c));
        } else if c.kind() != "discard" {
            stack.push(c);
        }
    }
}

fn pattern_names<'s, 'a>(
    cx: &Cx<'_, 's>,
    cur: Node<'a>,
    out: &mut Vec<&'s str>,
    stack: &mut Vec<Node<'a>>,
) {
    if let Some(nm) = child_by_field(cur, "name")
        && nm.kind() == "identifier"
    {
        out.push(text(cx, nm));
    }
    for c in named_children(cur) {
        if matches!(
            c.kind(),
            "parenthesized_variable_designation"
                | "declaration_pattern"
                | "var_pattern"
                | "recursive_pattern"
                | "tuple_pattern"
        ) {
            stack.push(c);
        }
    }
}

fn designation_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "tuple_pattern" || cur.kind() == "parenthesized_variable_designation" {
            tuple_names(cx, cur, out, &mut stack);
        } else if matches!(
            cur.kind(),
            "declaration_pattern" | "var_pattern" | "recursive_pattern"
        ) {
            pattern_names(cx, cur, out, &mut stack);
        }
    }
}

pub(crate) fn shadow_decl_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    match n.kind() {
        "foreach_statement" => {
            if let Some(l) = child_by_field(n, "left") {
                if l.kind() == "identifier" {
                    out.push(text(cx, l));
                } else {
                    designation_names(cx, l, out);
                }
            }
        }
        "declaration_expression" | "catch_declaration" => {
            if let Some(nm) = child_by_field(n, "name")
                && nm.kind() == "identifier"
            {
                out.push(text(cx, nm));
            }
        }
        "declaration_pattern" | "var_pattern" | "recursive_pattern" => {
            designation_names(cx, n, out);
        }
        _ => {}
    }
}
pub(crate) fn if_condition_shadow_names<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<&'s str>) {
    let mut stack = Vec::new();
    if let Some(cond) = child_by_field(n, "condition") {
        stack.push(cond);
    }
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "declaration_expression" => {
                if let Some(nm) = child_by_field(cur, "name")
                    && nm.kind() == "identifier"
                {
                    out.push(text(cx, nm));
                }
            }
            "declaration_pattern" | "var_pattern" | "recursive_pattern" => {
                designation_names(cx, cur, out);
            }
            "lambda_expression" | "anonymous_method_expression" => {}
            _ => {
                stack.extend(named_children(cur));
            }
        }
    }
}
