use sloplint_core::facts::{Arg, ArgShape};
use tree_sitter::Node;

use crate::hooks::{
    Cx, child_by_field, named_children, narrow, pos_of, string_arg_shape, text, trimmed,
};

use super::is_comment;

fn unwrap_paren(mut node: Node<'_>) -> Node<'_> {
    while node.kind() == "parenthesized_expression" {
        let Some(inner) = node.named_child(0) else {
            break;
        };
        node = inner;
    }
    node
}

fn starlark_is_concat(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let mut cur = unwrap_paren(n);
    loop {
        if cur.kind() != "binary_operator" {
            return false;
        }
        if !child_by_field(cur, "operator").is_some_and(|op| text(cx, op) == "+") {
            return false;
        }
        let left = child_by_field(cur, "left").map(unwrap_paren);
        let right = child_by_field(cur, "right").map(unwrap_paren);
        if left.is_some_and(|v| matches!(v.kind(), "string" | "concatenated_string"))
            || right.is_some_and(|v| matches!(v.kind(), "string" | "concatenated_string"))
        {
            return true;
        }
        if let Some(v) = left
            && v.kind() == "binary_operator"
        {
            cur = v;
            continue;
        }
        if let Some(v) = right
            && v.kind() == "binary_operator"
        {
            cur = v;
            continue;
        }
        return false;
    }
}

fn longest_str_run(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let ArgShape::Str { longest_static } = string_arg_shape(cx, n, &["string_content"]) else {
        return 0;
    };
    longest_static
}

fn has_interpolation(n: Node<'_>) -> bool {
    let mut cursor = n.walk();
    n.children(&mut cursor)
        .any(|child| child.kind() == "interpolation")
}

fn starlark_string_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let longest = longest_str_run(cx, n);
    if has_interpolation(n) {
        return ArgShape::Concat {
            longest_static: longest,
        };
    }
    ArgShape::Str {
        longest_static: longest,
    }
}

fn concat_parts_longest(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let mut longest = 0;
    let mut cursor = n.walk();
    for child in n.children(&mut cursor).filter(|c| c.kind() == "string") {
        longest = longest.max(longest_str_run(cx, child));
    }
    longest
}

fn side_longest(cx: &Cx<'_, '_>, side: Option<Node<'_>>) -> u32 {
    let Some(v) = side else {
        return 0;
    };
    if v.kind() == "string" {
        return longest_str_run(cx, v);
    }
    if v.kind() == "concatenated_string" {
        return concat_parts_longest(cx, v);
    }
    0
}

fn starlark_concat_longest(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let mut longest = 0;
    let mut cur = unwrap_paren(n);
    loop {
        if cur.kind() == "string" {
            return longest.max(longest_str_run(cx, cur));
        }
        if cur.kind() == "concatenated_string" {
            return longest.max(concat_parts_longest(cx, cur));
        }
        if cur.kind() != "binary_operator" {
            return longest;
        }
        let left = child_by_field(cur, "left").map(unwrap_paren);
        let right = child_by_field(cur, "right").map(unwrap_paren);
        longest = longest.max(side_longest(cx, left));
        longest = longest.max(side_longest(cx, right));
        if let Some(v) = left.filter(|v| v.kind() == "binary_operator") {
            cur = v;
        } else if let Some(v) = right.filter(|v| v.kind() == "binary_operator") {
            cur = v;
        } else {
            return longest;
        }
    }
}

fn starlark_has_closure(n: Node<'_>) -> bool {
    let mut cursor = n.walk();
    n.children(&mut cursor)
        .any(|child| child.is_named() && child.kind() == "lambda")
}

fn shape_of<'s>(cx: &Cx<'_, 's>, value: Node<'_>) -> ArgShape<'s> {
    match value.kind() {
        "string" => starlark_string_shape(cx, value),
        "concatenated_string" => ArgShape::Concat {
            longest_static: concat_parts_longest(cx, value),
        },
        "identifier" => ArgShape::Ident(trimmed(cx, value)),
        "list" | "tuple" | "set" => {
            if starlark_has_closure(value) {
                ArgShape::CodeBody
            } else {
                ArgShape::Collection
            }
        }
        "dictionary" => ArgShape::Other,
        "lambda" => ArgShape::CodeBody,
        _ if starlark_is_concat(cx, value) => ArgShape::Concat {
            longest_static: starlark_concat_longest(cx, value),
        },
        _ => ArgShape::Other,
    }
}

fn arg<'s>(shape: ArgShape<'s>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape,
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}

pub(crate) fn argument_nodes(call: Node<'_>) -> Vec<Node<'_>> {
    child_by_field(call, "arguments").map_or_else(Vec::new, |list| {
        named_children(list)
            .into_iter()
            .filter(|c| !is_comment(c.kind()))
            .collect()
    })
}

pub(crate) fn call_args<'s>(cx: &Cx<'_, 's>, call: Node<'_>) -> Vec<Arg<'s>> {
    argument_nodes(call)
        .into_iter()
        .map(|argument| {
            let value = if argument.kind() == "keyword_argument" {
                child_by_field(argument, "value")
            } else {
                Some(argument)
            };
            match value {
                Some(value) => arg(shape_of(cx, value), value),
                None => arg(ArgShape::Other, argument),
            }
        })
        .collect()
}

pub(crate) fn is_static_name(n: Node<'_>) -> bool {
    match n.kind() {
        "string" => !has_interpolation(n),
        "concatenated_string" => {
            let mut cursor = n.walk();
            n.children(&mut cursor)
                .filter(|c| c.kind() == "string")
                .all(|s| !has_interpolation(s))
        }
        _ => false,
    }
}

pub(crate) fn name_from_literal_loop(cx: &Cx<'_, '_>, name: Node<'_>) -> bool {
    let wanted = text(cx, name);
    let Some(func) = cx.ancestors().find(|a| a.kind() == "function_definition") else {
        return false;
    };
    cx.literal_targets(
        func.id(),
        || literal_loop_targets(func, cx),
        |targets| targets.iter().any(|t| t.as_str() == wanted),
    )
}

fn literal_loop_targets(func: Node<'_>, cx: &Cx<'_, '_>) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = vec![func];
    while let Some(next) = pending.pop() {
        if next.id() != func.id() && matches!(next.kind(), "function_definition" | "lambda") {
            continue;
        }
        if next.kind() == "for_statement" {
            push_loop_target(next, cx, &mut out);
        }
        let mut cursor = next.walk();
        pending.extend(next.children(&mut cursor).filter(|c| c.is_named()));
    }
    out
}

fn push_loop_target(for_node: Node<'_>, cx: &Cx<'_, '_>, out: &mut Vec<String>) {
    let Some(right) = child_by_field(for_node, "right") else {
        return;
    };
    if !matches!(right.kind(), "list" | "tuple") {
        return;
    }
    let mut cursor = right.walk();
    let mut count = 0;
    for child in right.children(&mut cursor).filter(|c| c.is_named()) {
        count += 1;
        if !is_static_name(child) {
            return;
        }
    }
    if count == 0 {
        return;
    }
    let Some(left) = child_by_field(for_node, "left") else {
        return;
    };
    if left.kind() == "identifier" {
        out.push(text(cx, left).to_owned());
    }
}
