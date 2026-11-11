use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, code_children, named_children, text};

pub(crate) fn closed_name(cx: &Cx<'_, '_>, expr: Node<'_>) -> bool {
    if expr.kind() != "identifier" {
        return false;
    }
    let want = text(cx, expr);
    enclosing_loop_ok(cx, want, expr)
}

fn enclosing_loop_ok(cx: &Cx<'_, '_>, want: &str, expr: Node<'_>) -> bool {
    for anc in cx.ancestors() {
        if is_func_like(anc.kind()) && func_param_binds(cx, anc, want) {
            return false;
        }
        if shadowed_catch(cx, anc, want) {
            return false;
        }
        if let Some(matched) = loop_decides(cx, anc, want, expr) {
            return matched;
        }
    }
    false
}

fn shadowed_catch(cx: &Cx<'_, '_>, anc: Node<'_>, want: &str) -> bool {
    anc.kind() == "catch_clause"
        && child_by_field(anc, "parameter").is_some_and(|param| binds_name(cx, param, want))
}

fn loop_decides(cx: &Cx<'_, '_>, anc: Node<'_>, want: &str, expr: Node<'_>) -> Option<bool> {
    if anc.kind() != "for_in_statement" {
        return None;
    }
    let left = child_by_field(anc, "left")?;
    if !binds_name(cx, left, want) {
        return None;
    }
    Some(loop_ok(cx, anc, want, expr))
}

fn loop_ok(cx: &Cx<'_, '_>, loop_node: Node<'_>, want: &str, expr: Node<'_>) -> bool {
    if !head_const(cx, loop_node) {
        return false;
    }
    if !is_of_loop(cx, loop_node) {
        return false;
    }
    if !head_binds_want(cx, loop_node, want) {
        return false;
    }
    let Some(right) = child_by_field(loop_node, "right") else {
        return false;
    };
    if !inline_string_array(right) {
        return false;
    }
    let Some(body) = child_by_field(loop_node, "body") else {
        return false;
    };
    if !contains(body, expr) {
        return false;
    }
    body_clean(cx, body, want)
}

fn head_const(cx: &Cx<'_, '_>, loop_node: Node<'_>) -> bool {
    child_by_field(loop_node, "kind").is_some_and(|kind| text(cx, kind) == "const")
}

fn is_of_loop(cx: &Cx<'_, '_>, loop_node: Node<'_>) -> bool {
    child_by_field(loop_node, "operator").is_some_and(|op| text(cx, op) == "of")
}

fn head_binds_want(cx: &Cx<'_, '_>, loop_node: Node<'_>, want: &str) -> bool {
    child_by_field(loop_node, "left")
        .is_some_and(|left| left.kind() == "identifier" && text(cx, left) == want)
}

fn inline_string_array(right: Node<'_>) -> bool {
    if right.kind() != "array" {
        return false;
    }
    let kids = code_children(right);
    if kids.is_empty() {
        return false;
    }
    kids.iter().all(|kid| kid.kind() == "string")
}

fn contains(body: Node<'_>, expr: Node<'_>) -> bool {
    body.start_byte() <= expr.start_byte() && expr.end_byte() <= body.end_byte()
}

const BODY_VISIT_CAP: usize = 16384;

fn body_clean(cx: &Cx<'_, '_>, body: Node<'_>, want: &str) -> bool {
    let mut stack = vec![body];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > BODY_VISIT_CAP {
            return false;
        }
        if !visit_body(cx, cur, want, &mut stack) {
            return false;
        }
    }
    true
}

fn visit_body<'t>(cx: &Cx<'_, '_>, cur: Node<'t>, want: &str, stack: &mut Vec<Node<'t>>) -> bool {
    match cur.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            if assigns_target(cx, cur, want) {
                return false;
            }
            push_kids(stack, cur);
        }
        "update_expression" => {
            if updates_name(cx, cur, want) {
                return false;
            }
            push_kids(stack, cur);
        }
        "for_in_statement" => {
            if undeclared_head_binds(cx, cur, want) {
                return false;
            }
            push_kids(stack, cur);
        }
        _ => push_kids(stack, cur),
    }
    true
}

fn assigns_target(cx: &Cx<'_, '_>, cur: Node<'_>, want: &str) -> bool {
    child_by_field(cur, "left").is_some_and(|left| assigns_root(cx, left, want))
}

fn updates_name(cx: &Cx<'_, '_>, cur: Node<'_>, want: &str) -> bool {
    child_by_field(cur, "argument")
        .is_some_and(|arg| arg.kind() == "identifier" && text(cx, arg) == want)
}

fn undeclared_head_binds(cx: &Cx<'_, '_>, loop_node: Node<'_>, want: &str) -> bool {
    if child_by_field(loop_node, "kind").is_some() {
        return false;
    }
    child_by_field(loop_node, "left").is_some_and(|left| binds_name(cx, left, want))
}

fn assigns_root(cx: &Cx<'_, '_>, mut cur: Node<'_>, want: &str) -> bool {
    let mut steps = 0usize;
    loop {
        steps += 1;
        if steps > 32 {
            return true;
        }
        if cur.kind() == "identifier" {
            return text(cx, cur) == want;
        }
        if cur.kind() == "member_expression" || cur.kind() == "subscript_expression" {
            let Some(obj) = child_by_field(cur, "object") else {
                return true;
            };
            cur = obj;
            continue;
        }
        return binds_name(cx, cur, want);
    }
}

fn is_func_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "generator_function_declaration"
            | "function_expression"
            | "generator_function"
            | "arrow_function"
            | "method_definition"
    )
}

fn func_param_binds(cx: &Cx<'_, '_>, func: Node<'_>, want: &str) -> bool {
    if let Some(single) = child_by_field(func, "parameter")
        && binds_name(cx, single, want)
    {
        return true;
    }
    let Some(list) = child_by_field(func, "parameters") else {
        return false;
    };
    for param in code_children(list) {
        if binds_name(cx, param, want) {
            return true;
        }
    }
    false
}

fn binds_name(cx: &Cx<'_, '_>, pat: Node<'_>, want: &str) -> bool {
    let mut stack = vec![pat];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > 64 {
            return true;
        }
        match cur.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                if text(cx, cur) == want {
                    return true;
                }
            }
            "object_pattern" | "array_pattern" => {
                stack.extend(named_children(cur));
            }
            "pair_pattern" => {
                if let Some(value) = child_by_field(cur, "value") {
                    stack.push(value);
                }
            }
            "rest_pattern" => {
                stack.extend(named_children(cur));
            }
            "assignment_pattern" | "object_assignment_pattern" => {
                if let Some(left) = child_by_field(cur, "left") {
                    stack.push(left);
                }
            }
            "required_parameter" | "optional_parameter" => {
                if let Some(inner) = child_by_field(cur, "pattern") {
                    stack.push(inner);
                } else {
                    stack.extend(named_children(cur));
                }
            }
            "parenthesized_expression" => {
                stack.extend(named_children(cur));
            }
            _ => {}
        }
    }
    false
}

fn push_kids<'t>(stack: &mut Vec<Node<'t>>, cur: Node<'t>) {
    stack.extend(named_children(cur));
}
