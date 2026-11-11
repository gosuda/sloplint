use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use super::funcs::{cut_at, seg_last};
use crate::hooks::{
    CallHead, Cx, child_by_field, first_named_child, first_named_child_matching, named_children,
    narrow, pos_of, text,
};
const ASSERT_CMP_OPS: &[&str] = &["==", "===", "!=", "!=="];

pub(crate) fn func_node(n: Node<'_>) -> Option<Node<'_>> {
    let mut func = child_by_field(n, "function")?;
    if func.kind() == "generic_function" {
        func = child_by_field(func, "function")?;
    }
    Some(func)
}

fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" => return Some(text(cx, current)),
            "generic_function" => child_by_field(current, "function")?,
            "field_expression" => child_by_field(current, "value")?,
            _ => return None,
        };
    }
}

fn func_root<'s>(cx: &Cx<'_, 's>, f: Node<'_>) -> Option<&'s str> {
    callee_root(cx, func_node(f).unwrap_or(f))
}

pub(crate) fn callee_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> &'s str {
    if n.kind() == "field_expression" {
        let mut current = n;
        loop {
            match child_by_field(current, "value") {
                Some(v) if v.kind() == "field_expression" => current = v,
                Some(v) if matches!(v.kind(), "identifier" | "operator_identifier" | "this") => {
                    return cut_at(text(cx, n), '[');
                }
                _ => {
                    return child_by_field(n, "field")
                        .map(|f| text(cx, f))
                        .unwrap_or_else(|| cut_at(text(cx, n), '['));
                }
            }
        }
    }
    cut_at(text(cx, n), '[')
}

fn gaps<'s>(cx: &Cx<'_, 's>, n: Node<'_>, gap_kind: &str) -> Vec<&'s str> {
    let t = text(cx, n);
    let open = if t.starts_with("\"\"\"") { 3 } else { 1 };
    let mut cursor = n.start_byte() + open;
    let close_at = n.end_byte().saturating_sub(open);
    let mut segs = Vec::new();
    let mut walker = n.walk();
    for c in n.children(&mut walker) {
        if c.kind() == gap_kind {
            segs.push(crate::hooks::slice(cx.src, cursor, c.start_byte()));
            cursor = c.end_byte();
        }
    }
    segs.push(crate::hooks::slice(cx.src, cursor, close_at));
    segs
}

fn collapse_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn plain_static_len<'s>(cx: &Cx<'_, 's>, a: Node<'_>) -> u32 {
    let t = text(cx, a);
    if t.starts_with("\"\"\"") {
        return longest_static_chunk(&gaps(cx, a, "escape_sequence"));
    }
    let body = t.get(1..t.len().saturating_sub(1)).unwrap_or("");
    let collapsed = collapse_escapes(body);
    longest_static_chunk(&[collapsed.as_str()])
}

fn interpolated_static_len<'s>(cx: &Cx<'_, 's>, a: Node<'_>) -> u32 {
    let inner = first_named_child_matching(a, &["interpolated_string"]).unwrap_or(a);
    let segs = gaps(cx, inner, "interpolation");
    let collapsed: Vec<String> = segs.iter().map(|s| collapse_escapes(s)).collect();
    let refs: Vec<&str> = collapsed.iter().map(|s| s.as_str()).collect();
    longest_static_chunk(&refs)
}

fn concat_tree_len(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<u32> {
    let mut best = 0;
    let mut found = false;
    let mut stack = vec![n];
    while let Some(c) = stack.pop() {
        match c.kind() {
            "infix_expression" => {
                if child_by_field(c, "operator")
                    .map(|o| text(cx, o))
                    .unwrap_or("")
                    != "+"
                {
                    continue;
                }
                stack.extend(
                    [child_by_field(c, "left"), child_by_field(c, "right")]
                        .into_iter()
                        .flatten(),
                );
            }
            "parenthesized_expression" => {
                stack.extend(first_named_child(c));
            }
            "string" => {
                found = true;
                best = best.max(plain_static_len(cx, c));
            }
            "interpolated_string_expression" => {
                found = true;
                best = best.max(interpolated_static_len(cx, c));
            }
            _ => {}
        }
    }
    found.then_some(best)
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, a: Node<'_>) -> ArgShape<'s> {
    if a.kind() == "string" {
        return ArgShape::Str {
            longest_static: plain_static_len(cx, a),
        };
    }
    if a.kind() == "interpolated_string_expression" {
        return ArgShape::Concat {
            longest_static: interpolated_static_len(cx, a),
        };
    }
    if a.kind() == "infix_expression"
        && let Some(best) = concat_tree_len(cx, a)
    {
        return ArgShape::Concat {
            longest_static: best,
        };
    }
    if a.kind() == "identifier" {
        return ArgShape::Ident(text(cx, a));
    }
    if a.kind() == "lambda_expression" || a.kind() == "block" || a.kind() == "case_block" {
        return ArgShape::CodeBody;
    }
    if a.kind() == "tuple_expression" {
        if named_children(a).iter().any(|c| {
            c.kind() == "lambda_expression" || c.kind() == "block" || c.kind() == "case_block"
        }) {
            return ArgShape::CodeBody;
        }
        return ArgShape::Collection;
    }
    ArgShape::Other
}

fn arg_nodes(n: Node<'_>) -> Vec<Node<'_>> {
    let Some(list) = child_by_field(n, "arguments") else {
        return Vec::new();
    };
    if list.kind() == "arguments" {
        named_children(list)
            .into_iter()
            .filter(|c| c.kind() != "comment" && c.kind() != "block_comment")
            .collect()
    } else {
        vec![list]
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    arg_nodes(n)
        .into_iter()
        .map(|a| Arg {
            shape: arg_shape(cx, a),
            start_line: narrow(a.start_position().row) + 1,
            end_line: narrow(a.end_position().row) + 1,
        })
        .collect()
}

fn first_arg(n: Node<'_>) -> Option<Node<'_>> {
    arg_nodes(n).into_iter().next()
}
pub(crate) fn call_arg_nodes(n: Node<'_>) -> Vec<Node<'_>> {
    arg_nodes(n)
}

pub(crate) fn call_first_arg(n: Node<'_>) -> Option<Node<'_>> {
    first_arg(n)
}

fn is_simple(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" | "operator_identifier" | "this" => return true,
            "generic_function" => match child_by_field(current, "function") {
                Some(next) => current = next,
                None => return false,
            },
            "field_expression" => match child_by_field(current, "value") {
                Some(next) => current = next,
                None => return false,
            },
            _ => return false,
        }
    }
}

fn initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "var_definition" && parent.kind() != "val_definition" {
        return false;
    };
    child_by_field(parent, "value").is_some_and(|v| v.id() == n.id())
}

pub(crate) fn call_identity<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
) -> Option<(&'s str, Option<&'s str>, bool)> {
    let func = child_by_field(n, "function")?;
    if func.kind() == "call_expression" {
        return Some(("", None, false));
    }
    let node = func_node(func).unwrap_or(func);
    Some((callee_text(cx, node), func_root(cx, func), is_simple(node)))
}

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    if n.kind() == "call_expression" || n.kind() == "split_call_condition" {
        let (callee, callee_root, is_simple_path) = call_identity(cx, n)?;
        return Some(CallHead {
            callee,
            kind: CallKind::Plain,
            callee_root,
            is_simple_path,
            initializes_binding: initializes_binding(cx, n),
            args: call_args(cx, n),
        });
    }
    if n.kind() != "instance_expression" {
        return None;
    }
    let ty = first_named_child_matching(
        n,
        &["type_identifier", "stable_type_identifier", "generic_type"],
    )?;
    Some(CallHead {
        callee: cut_at(text(cx, ty), '['),
        kind: CallKind::New,
        callee_root: None,
        is_simple_path: true,
        initializes_binding: initializes_binding(cx, n),
        args: call_args(cx, n),
    })
}

fn first_arg_count(n: Node<'_>) -> usize {
    arg_nodes(n).len()
}

pub(crate) fn assertion_head(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() != "call_expression" {
        return None;
    }
    if let Some(p) = cx.parent()
        && p.kind() == "call_expression"
        && child_by_field(p, "function").is_some_and(|f| f.id() == n.id())
    {
        return None;
    }
    let func = func_node(n)?;
    let name = if func.kind() == "call_expression" {
        seg_last(text(cx, func_node(func).unwrap_or(func)))
    } else {
        seg_last(text(cx, func))
    };
    match name {
        "assert" => {
            let cond = first_arg(n);
            let cmp = cond.is_some_and(|f| {
                f.kind() == "infix_expression"
                    && child_by_field(f, "operator")
                        .is_some_and(|o| ASSERT_CMP_OPS.contains(&text(cx, o)))
            });
            let bare = cond.is_some_and(|f| f.kind() == "identifier");
            Some(Assertion {
                pos: pos_of(n),
                has_message: narrow(first_arg_count(n)) >= 2,
                comparison: cmp,
                shape: if cmp {
                    AssertShape::TopEqNe
                } else if bare {
                    AssertShape::BareName
                } else {
                    AssertShape::Other
                },
                in_typecheck: false,
                value_is_bool_literal: false,
            })
        }
        "assertResult" => Some(Assertion {
            pos: pos_of(n),
            has_message: narrow(first_arg_count(n)) >= 2,
            comparison: true,
            shape: AssertShape::TopEqNe,
            in_typecheck: false,
            value_is_bool_literal: false,
        }),
        "assertEquals" | "assertNotEquals" => Some(Assertion {
            pos: pos_of(n),
            has_message: narrow(first_arg_count(n)) >= 3,
            comparison: true,
            shape: AssertShape::TopEqNe,
            in_typecheck: false,
            value_is_bool_literal: false,
        }),
        _ => None,
    }
}

const REFLECT_NAMES: &[&str] = &[
    "getMethod",
    "getDeclaredMethod",
    "getField",
    "getDeclaredField",
    "forName",
];

pub(crate) fn dynamic_head(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() != "call_expression" {
        return None;
    }
    let func = func_node(n)?;
    if !REFLECT_NAMES.contains(&seg_last(text(cx, func))) {
        return None;
    }
    if first_arg(n).is_none_or(|a| a.kind() == "string") {
        return None;
    }
    Some("reflection lookup by computed name")
}
