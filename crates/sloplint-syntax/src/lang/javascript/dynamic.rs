//! JavaScript dynamic-symbol closed-set suppression.

use tree_sitter::Node;

use crate::hooks::{
    Cx, FileSetKind, child_by_field, code_children, find_named_child, named_children, text,
};

pub(crate) fn closed_name(cx: &Cx<'_, '_>, expr: Node<'_>) -> bool {
    if literal_expr(expr) {
        return true;
    }
    if expr.kind() != "identifier" {
        return false;
    }
    let want = text(cx, expr);
    const_member(cx, want) || loop_member(cx, want)
}

fn literal_expr(n: Node<'_>) -> bool {
    let mut stack = vec![n];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > 64 {
            return false;
        }
        match cur.kind() {
            "string" | "number" | "true" | "false" | "null" | "undefined" => {}
            "template_string" => {
                if find_named_child(cur, |child| child.kind() == "template_substitution").is_some()
                {
                    return false;
                }
            }
            "member_expression" => {
                if !child_by_field(cur, "property")
                    .is_some_and(|property| property.kind() == "property_identifier")
                {
                    return false;
                }
                if let Some(object) = child_by_field(cur, "object") {
                    stack.push(object);
                }
            }
            "ternary_expression" => {
                for field in ["consequence", "alternative"] {
                    match child_by_field(cur, field) {
                        Some(branch) => stack.push(branch),
                        None => return false,
                    }
                }
            }
            "binary_expression" | "parenthesized_expression" => stack.extend(named_children(cur)),
            _ => return false,
        }
    }
    true
}

fn const_member(cx: &Cx<'_, '_>, want: &str) -> bool {
    cx.file_names(FileSetKind::Literal, || const_bound_names(cx))
        .binary_search(&want)
        .is_ok()
}

fn const_bound_names<'s>(cx: &Cx<'_, 's>) -> Vec<&'s str> {
    let Some(root) = cx.root() else {
        return Vec::new();
    };
    let mut bound: Vec<&'s str> = Vec::new();
    let mut seen: Vec<&'s str> = Vec::new();
    let mut queue = vec![root];
    let mut head = 0usize;
    while head < queue.len() {
        let cur = queue[head];
        head += 1;
        record_const_declaration(cur, cx, &mut bound, &mut seen);
        push_children(&mut queue, cur);
    }
    bound.retain(|name| seen.iter().filter(|other| *other == name).count() == 1);
    bound.sort_unstable();
    bound.dedup();
    bound
}

fn record_const_declaration<'s>(
    declaration: Node<'_>,
    cx: &Cx<'_, 's>,
    bound: &mut Vec<&'s str>,
    seen: &mut Vec<&'s str>,
) {
    if declaration.kind() != "lexical_declaration"
        || !child_by_field(declaration, "kind").is_some_and(|kind| text(cx, kind) == "const")
    {
        return;
    }
    let declarators = named_children(declaration);
    let single = declarators.len() == 1;
    for declarator in declarators {
        let Some(name) =
            child_by_field(declarator, "name").filter(|name| name.kind() == "identifier")
        else {
            continue;
        };
        let label = text(cx, name);
        seen.push(label);
        let closed = single
            && child_by_field(declarator, "value").is_some_and(|value| const_value(cx, value));
        if closed {
            bound.push(label);
        }
    }
}

fn const_value(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    match n.kind() {
        "array" => array_const(cx, n),
        "object" => object_const(cx, n),
        _ => literal_expr(n),
    }
}

fn array_const(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let kids = code_children(n);
    !kids.is_empty()
        && kids
            .iter()
            .all(|child| child.kind() != "spread_element" && const_value(cx, *child))
}

fn object_const(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let kids = code_children(n);
    !kids.is_empty()
        && kids.iter().all(|child| match child.kind() {
            "pair" => {
                object_key(*child)
                    && child_by_field(*child, "value").is_some_and(|value| const_value(cx, value))
            }
            "shorthand_property_identifier" | "method_definition" => true,
            _ => false,
        })
}

fn object_key(pair: Node<'_>) -> bool {
    child_by_field(pair, "key")
        .is_some_and(|key| matches!(key.kind(), "property_identifier" | "string" | "number"))
}

fn loop_member(cx: &Cx<'_, '_>, want: &str) -> bool {
    let Some(root) = cx.root() else {
        return false;
    };
    cx.literal_targets(
        root.id(),
        || loop_targets(root, cx),
        |names| names.iter().any(|name| name == want),
    )
}

fn loop_targets(root: Node<'_>, cx: &Cx<'_, '_>) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue = vec![root];
    let mut head = 0usize;
    while head < queue.len() {
        let cur = queue[head];
        head += 1;
        if cur.kind() == "for_in_statement" {
            push_for_names(cur, cx, &mut out);
        }
        push_children(&mut queue, cur);
    }
    out
}

fn push_for_names(n: Node<'_>, cx: &Cx<'_, '_>, out: &mut Vec<String>) {
    let Some(right) = child_by_field(n, "right") else {
        return;
    };
    if !closed_iterable(cx, right) {
        return;
    }
    let Some(left) = child_by_field(n, "left") else {
        return;
    };
    js_pattern_names(left, cx, out);
}

fn closed_iterable(cx: &Cx<'_, '_>, right: Node<'_>) -> bool {
    match right.kind() {
        "array" | "object" => const_value(cx, right),
        "call_expression" => keys_call(cx, right),
        "identifier" => const_member(cx, text(cx, right)),
        _ => false,
    }
}

fn keys_call(cx: &Cx<'_, '_>, call: Node<'_>) -> bool {
    let Some(function) = child_by_field(call, "function") else {
        return false;
    };
    if !matches!(
        text(cx, function),
        "Object.keys" | "Object.values" | "Object.entries"
    ) {
        return false;
    }
    let args = child_by_field(call, "arguments").map_or(Vec::new(), named_children);
    args.len() == 1 && keys_source(cx, args[0])
}

fn keys_source(cx: &Cx<'_, '_>, source: Node<'_>) -> bool {
    match source.kind() {
        "array" | "object" => const_value(cx, source),
        "identifier" => const_member(cx, text(cx, source)),
        _ => false,
    }
}

fn js_pattern_names(left: Node<'_>, cx: &Cx<'_, '_>, out: &mut Vec<String>) {
    let mut stack = vec![left];
    let mut visits = 0usize;
    while let Some(cur) = stack.pop() {
        visits += 1;
        if visits > 64 {
            return;
        }
        match cur.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                out.push(text(cx, cur).to_owned());
            }
            "object_pattern" => {
                for child in named_children(cur) {
                    if child.kind() == "pair_pattern" {
                        if let Some(value) = child_by_field(child, "value") {
                            stack.push(value);
                        }
                    } else {
                        stack.push(child);
                    }
                }
            }
            "array_pattern" => stack.extend(named_children(cur)),
            _ => {}
        }
    }
}

fn push_children<'t>(queue: &mut Vec<Node<'t>>, n: Node<'t>) {
    let mut kids = named_children(n);
    kids.reverse();
    queue.extend(kids);
}
