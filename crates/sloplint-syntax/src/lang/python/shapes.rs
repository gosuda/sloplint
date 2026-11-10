//! Python call-argument shape classification.

use sloplint_core::facts::{Arg, ArgShape};
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, code_children, narrow, pos_of, string_arg_shape, text};

fn is_str_operand(n: Node<'_>) -> bool {
    matches!(n.kind(), "string" | "concatenated_string")
}

fn py_is_concat(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let mut cur = n;
    loop {
        if cur.kind() != "binary_operator" {
            return false;
        }
        if !child_by_field(cur, "operator").is_some_and(|op| text(cx, op) == "+") {
            return false;
        }
        let left = child_by_field(cur, "left");
        let right = child_by_field(cur, "right");
        if left.is_some_and(is_str_operand) || right.is_some_and(is_str_operand) {
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

fn py_string_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
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

fn py_concat_longest(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let mut longest = 0;
    let mut cur = n;
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
        let left = child_by_field(cur, "left");
        let right = child_by_field(cur, "right");
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

fn py_has_closure(n: Node<'_>) -> bool {
    let mut cursor = n.walk();
    n.children(&mut cursor)
        .any(|child| child.is_named() && child.kind() == "lambda")
}

fn py_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => py_string_shape(cx, n),
        "concatenated_string" => ArgShape::Concat {
            longest_static: concat_parts_longest(cx, n),
        },
        "identifier" => ArgShape::Ident(text(cx, n)),
        "list" | "tuple" | "set" => {
            if py_has_closure(n) {
                ArgShape::CodeBody
            } else {
                ArgShape::Collection
            }
        }
        "dictionary" => ArgShape::Other,
        "lambda" => ArgShape::CodeBody,
        _ if py_is_concat(cx, n) => ArgShape::Concat {
            longest_static: py_concat_longest(cx, n),
        },
        _ => ArgShape::Other,
    }
}

fn py_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: py_shape(cx, n),
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}

fn py_push_arg<'s>(cx: &Cx<'_, 's>, c: Node<'_>, args: &mut Vec<Arg<'s>>) {
    if c.kind() != "keyword_argument" {
        args.push(py_arg(cx, c));
        return;
    }
    if let Some(v) = child_by_field(c, "value") {
        args.push(py_arg(cx, v));
    }
}

pub(crate) fn py_call_args<'s>(cx: &Cx<'_, 's>, list: Node<'_>, args: &mut Vec<Arg<'s>>) {
    if list.kind() == "generator_expression" {
        args.push(py_arg(cx, list));
        return;
    }
    for c in code_children(list) {
        py_push_arg(cx, c, args);
    }
}
