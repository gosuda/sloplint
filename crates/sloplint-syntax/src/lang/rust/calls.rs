use sloplint_core::facts::{Arg, ArgShape};
use tree_sitter::Node;

use crate::hooks::{
    Cx, Link, binary_concat, child_by_field, first_named_child, first_named_child_matching, narrow,
    only_named_child_matching, pos_of, string_arg_shape, text,
};
const RUST_CALLEE: u8 = 12;

pub(super) fn is_simple_path(n: Node<'_>) -> bool {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" => return true,
            "field_expression" => {
                let Some(value) = child_by_field(cur, "value") else {
                    return false;
                };
                if !matches!(
                    value.kind(),
                    "identifier" | "field_expression" | "scoped_identifier"
                ) {
                    return false;
                }
                cur = value;
            }
            "scoped_identifier" => {
                let Some(path) = child_by_field(cur, "path") else {
                    return false;
                };
                if !matches!(path.kind(), "identifier" | "scoped_identifier") {
                    return false;
                }
                cur = path;
            }
            _ => return false,
        }
    }
}

pub(super) fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    if !is_simple_path(n) {
        return None;
    }
    let root = cx.chain_root(RUST_CALLEE, n, |cur| match cur.kind() {
        "field_expression" => child_by_field(cur, "value").map_or(Link::Root(cur), Link::Next),
        "scoped_identifier" => child_by_field(cur, "path").map_or(Link::Root(cur), Link::Next),
        _ => Link::Root(cur),
    });
    (!root.is_empty()).then_some(root)
}
pub(super) fn initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    match parent.kind() {
        "let_declaration" | "const_item" | "static_item" => {
            child_by_field(parent, "value").is_some_and(|value| value.id() == n.id())
        }
        _ => false,
    }
}

fn unwrap_call(n: Node<'_>) -> Option<Node<'_>> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "call_expression" | "macro_invocation" => return Some(cur),
            "expression_statement"
            | "return_expression"
            | "try_expression"
            | "await_expression" => {
                cur = first_named_child(cur)?;
            }
            _ => return None,
        }
    }
}

pub(super) fn sole_call_id(body: Node<'_>) -> Option<usize> {
    let only = only_named_child_matching(body, |c| {
        c.kind() != "line_comment" && c.kind() != "block_comment"
    })?;
    unwrap_call(only).map(|c| c.id())
}

fn macro_body_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let Some(tt) = first_named_child_matching(n, &["token_tree"]) else {
        return ArgShape::Other;
    };
    let mut cursor = tt.walk();
    let opener = tt
        .children(&mut cursor)
        .filter(|c| !c.is_named())
        .map(|c| text(cx, c))
        .find(|t| matches!(*t, "(" | "[" | "{"));
    match opener {
        Some("[") => ArgShape::Collection,
        Some("{") => ArgShape::CodeBody,
        _ => ArgShape::Other,
    }
}

fn shaped_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    const STR_KINDS: [&str; 2] = ["string_literal", "raw_string_literal"];
    let shape = match n.kind() {
        "string_literal" | "raw_string_literal" => string_arg_shape(cx, n, &["string_content"]),
        "identifier" => ArgShape::Ident(text(cx, n)),
        "array_expression" | "tuple_expression" | "struct_expression" => ArgShape::Collection,
        "closure_expression" | "block" | "match_expression" | "if_expression"
        | "for_expression" | "while_expression" | "loop_expression" => ArgShape::CodeBody,
        "macro_invocation" => macro_body_shape(cx, n),
        _ if binary_concat(cx, n, "+", &STR_KINDS) => ArgShape::Concat,
        _ => ArgShape::Other,
    };
    Arg {
        shape,
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}

pub(super) fn macro_args<'s>(cx: &Cx<'_, 's>, tt: Node<'_>) -> Vec<Arg<'s>> {
    let mut groups: Vec<Vec<Node<'_>>> = vec![Vec::new()];
    let mut cursor = tt.walk();
    for child in tt.children(&mut cursor) {
        if !child.is_named() {
            let t = text(cx, child);
            if t == "," {
                groups.push(Vec::new());
                continue;
            }
            if matches!(t, "(" | ")" | "[" | "]" | "{" | "}") {
                continue;
            }
        }
        if let Some(last) = groups.last_mut() {
            last.push(child);
        }
    }
    if groups
        .first()
        .is_some_and(|g| g.len() >= 2 && text(cx, g[0]) == "target" && text(cx, g[1]) == ":")
    {
        groups.remove(0);
    }
    groups
        .into_iter()
        .filter(|g| !g.is_empty())
        .enumerate()
        .map(|(i, g)| {
            let shape = if i == 0 {
                g.iter()
                    .find(|n| n.is_named())
                    .map_or(ArgShape::Other, |n| {
                        if n.kind() == "string_literal" {
                            string_arg_shape(cx, *n, &["string_content"])
                        } else {
                            ArgShape::Other
                        }
                    })
            } else {
                ArgShape::Other
            };
            let start = g.first().map_or(0, |n| pos_of(*n).line);
            let end = g.last().map_or(0, |n| narrow(n.end_position().row) + 1);
            Arg {
                shape,
                start_line: start,
                end_line: end,
            }
        })
        .collect()
}

pub(super) fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    child_by_field(n, "arguments").map_or(Vec::new(), |a| {
        let mut cursor = a.walk();
        a.children(&mut cursor)
            .filter(|c| c.is_named())
            .map(|c| shaped_arg(cx, c))
            .collect()
    })
}
