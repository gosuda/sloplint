//! Go call heads, argument shapes, and assertion facts.

use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion};
use tree_sitter::Node;

use crate::hooks::{
    Cx, Link, binary_concat, child_by_field, code_children, first_named_child, named_children,
    narrow, pos_of, string_arg_shape, text,
};
const GO_CALLEE: u8 = 11;

pub(crate) fn go_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "interpreted_string_literal" => {
            string_arg_shape(cx, n, &["interpreted_string_literal_content"])
        }
        "raw_string_literal" => string_arg_shape(cx, n, &["raw_string_literal_content"]),
        "composite_literal" => go_collection_shape(n),
        "func_literal" => ArgShape::CodeBody,
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ if binary_concat(
            cx,
            n,
            "+",
            &["interpreted_string_literal", "raw_string_literal"],
        ) =>
        {
            ArgShape::Concat {
                longest_static: go_concat_static(cx, n),
            }
        }
        _ => ArgShape::Other,
    }
}

fn go_concat_static(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let mut chunks = Vec::new();
    let mut stack = vec![n];
    while let Some(m) = stack.pop() {
        if m.kind() == "binary_expression"
            && child_by_field(m, "operator").is_some_and(|o| text(cx, o) == "+")
        {
            if let Some(right) = child_by_field(m, "right") {
                stack.push(right);
            }
            if let Some(left) = child_by_field(m, "left") {
                stack.push(left);
            }
            continue;
        }
        if m.kind() == "interpreted_string_literal" || m.kind() == "raw_string_literal" {
            let mut cursor = m.walk();
            for child in m.children(&mut cursor) {
                if child.kind() == "interpreted_string_literal_content"
                    || child.kind() == "raw_string_literal_content"
                {
                    chunks.push(text(cx, child));
                }
            }
        }
    }
    sloplint_core::text::longest_static_chunk(&chunks)
}

fn go_has_func_literal(root: Node<'_>) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "func_literal" {
            return true;
        }
        let mut cursor = n.walk();
        stack.extend(n.children(&mut cursor));
    }
    false
}

fn go_collection_shape<'s>(n: Node<'_>) -> ArgShape<'s> {
    let body = child_by_field(n, "body");
    if body.is_some_and(|b| {
        named_children(b)
            .iter()
            .any(|e| e.kind() == "keyed_element")
    }) {
        return ArgShape::Other;
    }
    if body.is_some_and(go_has_func_literal) {
        return ArgShape::CodeBody;
    }
    ArgShape::Collection
}

pub(crate) fn go_arg<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Arg<'s> {
    Arg {
        shape: go_shape(cx, n),
        start_line: pos_of(n).line,
        end_line: narrow(n.end_position().row) + 1,
    }
}

pub(crate) fn go_is_simple_path(n: Node<'_>) -> bool {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" => return true,
            "selector_expression" => {
                let Some(next) = child_by_field(cur, "operand") else {
                    return false;
                };
                if !matches!(next.kind(), "identifier" | "selector_expression") {
                    return false;
                }
                cur = next;
            }
            _ => return false,
        }
    }
}

pub(crate) fn go_callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    if !go_is_simple_path(n) {
        return None;
    }
    let root = cx.chain_root(GO_CALLEE, n, |cur| match cur.kind() {
        "selector_expression" => child_by_field(cur, "operand").map_or(Link::Root(cur), Link::Next),
        _ => Link::Root(cur),
    });
    (!root.is_empty()).then_some(root)
}

pub(crate) fn go_callee<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    let mut cur = func;
    loop {
        match cur.kind() {
            "identifier" => return text(cx, cur),
            "selector_expression" => {
                if go_receiver_calls(child_by_field(cur, "operand")) {
                    return child_by_field(cur, "field").map_or("", |f| text(cx, f));
                }
                return text(cx, cur);
            }
            "parenthesized_expression" => match first_named_child(cur) {
                Some(next) => cur = next,
                None => return "",
            },
            _ => return "",
        }
    }
}

fn go_receiver_calls(operand: Option<Node<'_>>) -> bool {
    let Some(mut cur) = operand else {
        return false;
    };
    loop {
        match cur.kind() {
            "call_expression" | "index_expression" | "slice_expression" => return true,
            "parenthesized_expression" => match first_named_child(cur) {
                Some(next) => cur = next,
                None => return false,
            },
            "selector_expression" => match child_by_field(cur, "operand") {
                Some(next) => cur = next,
                None => return false,
            },
            _ => return false,
        }
    }
}

fn cx_grandparent<'t>(cx: &Cx<'t, '_>) -> Option<Node<'t>> {
    cx.ancestors().nth(1)
}

fn go_init_host<'t>(cx: &Cx<'t, '_>, n: Node<'t>) -> Option<(Node<'t>, Node<'t>)> {
    let parent = cx.parent()?;
    if parent.kind() == "expression_list" && parent.named_child_count() == 1 {
        Some((cx_grandparent(cx)?, parent))
    } else {
        Some((parent, n))
    }
}

fn go_has_named(cx: &Cx<'_, '_>, host: Node<'_>) -> bool {
    if host.kind() == "short_var_declaration" {
        return child_by_field(host, "left").is_some_and(|left| {
            named_children(left)
                .into_iter()
                .any(|c| c.kind() == "identifier" && text(cx, c) != "_")
        });
    }
    named_children(host)
        .into_iter()
        .any(|c| c.kind() == "identifier" && text(cx, c) != "_")
}

pub(crate) fn go_initializes_binding<'t>(cx: &Cx<'t, '_>, n: Node<'t>) -> bool {
    let Some((host, init)) = go_init_host(cx, n) else {
        return false;
    };
    match host.kind() {
        "short_var_declaration" => {
            host.child_by_field_name("right")
                .is_some_and(|r| r.id() == init.id())
                && go_has_named(cx, host)
        }
        "var_spec" | "const_spec" => {
            host.child_by_field_name("value")
                .is_some_and(|v| v.id() == init.id())
                && go_has_named(cx, host)
        }
        _ => false,
    }
}

fn go_unparen(n: Node<'_>) -> Node<'_> {
    let mut cur = n;
    while cur.kind() == "parenthesized_expression" {
        match first_named_child(cur) {
            Some(next) => cur = next,
            None => break,
        }
    }
    cur
}

fn go_assert_shape(cx: &Cx<'_, '_>, value: Node<'_>) -> (AssertShape, bool) {
    let value = go_unparen(value);
    match value.kind() {
        "true" | "false" => (AssertShape::Other, true),
        "identifier" => (AssertShape::BareName, false),
        "binary_expression" => {
            let same = child_by_field(value, "operator")
                .is_some_and(|o| text(cx, o) == "==" || text(cx, o) == "!=");
            (
                if same {
                    AssertShape::TopEqNe
                } else {
                    AssertShape::Other
                },
                false,
            )
        }
        _ => (AssertShape::Other, false),
    }
}

fn go_last_segment(callee: &str) -> &str {
    let bytes = callee.as_bytes();
    let mut start = bytes.len();
    for &b in bytes.iter().rev() {
        start -= 1;
        if b == b'.' || b == b'/' {
            return &callee[start + 1..];
        }
    }
    callee
}

fn tail_two(callee: &str) -> &str {
    let mut seen = 0;
    for (i, &b) in callee.as_bytes().iter().enumerate().rev() {
        if b == b'.' || b == b'/' {
            seen += 1;
            if seen == 2 {
                return &callee[i + 1..];
            }
        }
    }
    callee
}

pub(crate) fn go_call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    child_by_field(n, "arguments").map_or(Vec::new(), |a| {
        code_children(a)
            .into_iter()
            .map(|c| go_arg(cx, c))
            .collect()
    })
}

pub(crate) fn go_assertion(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
    let tail = tail_two(callee);
    if tail != "assert.True"
        && tail != "assert.False"
        && tail != "require.True"
        && tail != "require.False"
    {
        return None;
    }
    let args = child_by_field(n, "arguments")
        .map(code_children)
        .unwrap_or_default();
    let count = args.len();
    let (shape, literal) = args
        .get(1)
        .map_or((AssertShape::Other, false), |v| go_assert_shape(cx, *v));
    Some(Assertion {
        pos: pos_of(n),
        has_message: count >= 3,
        comparison: false,
        shape,
        in_typecheck: false,
        value_is_bool_literal: literal,
    })
}

pub(crate) fn go_dynamic(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = child_by_field(n, "function").map_or("", |f| text(cx, f));
    let last = go_last_segment(callee);
    if last != "MethodByName" && last != "FieldByName" {
        return None;
    }
    let first = child_by_field(n, "arguments").and_then(|a| code_children(a).into_iter().next());
    if first.is_some_and(|f| {
        f.kind() == "interpreted_string_literal" || f.kind() == "raw_string_literal"
    }) {
        return None;
    }
    Some("reflection by name")
}
