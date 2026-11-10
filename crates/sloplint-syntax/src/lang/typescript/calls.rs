//! Call and pattern helpers for TypeScript.

use sloplint_core::facts::{Arg, ArgShape, Pos};
use tree_sitter::Node;

use crate::hooks::{
    Cx, binary_concat, child_by_field, code_children, find_named_child, first_named_child,
    named_children, narrow, only_named_child_matching, pos_of, sole_call_id, string_arg_shape,
    text,
};

pub(crate) fn ts_call_stmt(n: Node<'_>) -> Option<usize> {
    let inner = match n.kind() {
        "return_statement" | "expression_statement" => {
            find_named_child(n, |child| child.kind() != "comment")?
        }
        _ => return None,
    };
    let mut cur = inner;
    loop {
        if cur.kind() == "call_expression" {
            return Some(cur.id());
        }
        if cur.kind() == "await_expression" {
            cur = find_named_child(cur, |child| child.kind() != "comment")?;
        } else {
            return None;
        }
    }
}

pub(crate) fn ts_sole_call(body: Node<'_>) -> Option<usize> {
    sole_call_id(
        body,
        |block| only_named_child_matching(block, |c| c.kind() != "comment"),
        ts_call_stmt,
    )
}

pub(crate) fn ts_pat<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<(&'s str, Pos)>) {
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                out.push((text(cx, cur), pos_of(cur)));
            }
            "object_pattern" | "array_pattern" | "rest_pattern" => {
                stack.extend(named_children(cur));
            }
            "pair_pattern" => {
                if let Some(v) = child_by_field(cur, "value") {
                    stack.push(v);
                }
            }
            "assignment_pattern" | "object_assignment_pattern" => {
                if let Some(l) = child_by_field(cur, "left") {
                    stack.push(l);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn ts_declared<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<(&'s str, Pos)> {
    let mut out = Vec::new();
    for d in named_children(n) {
        if d.kind() != "variable_declarator" {
            continue;
        }
        if let Some(name) = child_by_field(d, "name") {
            ts_pat(cx, name, &mut out);
        }
    }
    out
}

pub(crate) fn ts_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<(&'s str, Pos)> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" | "this" => return Some((text(cx, cur), pos_of(cur))),
            "member_expression" => cur = child_by_field(cur, "object")?,
            "subscript_expression" => cur = child_by_field(cur, "object")?,
            "parenthesized_expression" => cur = first_named_child(cur)?,
            _ => return None,
        }
    }
}

pub(crate) fn callee_root<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let mut cur = n;
    loop {
        match cur.kind() {
            "identifier" | "this" | "super" => return Some(text(cx, cur)),
            "member_expression" | "subscript_expression" => {
                cur = child_by_field(cur, "object")?;
            }
            "parenthesized_expression" => cur = cur.named_child(0)?,
            _ => return None,
        }
    }
}

pub(crate) fn ts_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_fragment"]),
        "template_string" => string_arg_shape(cx, n, &["string_fragment"]),
        "array" | "object" => ArgShape::Collection,
        "arrow_function" | "function_expression" | "generator_function" => ArgShape::CodeBody,
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ if binary_concat(cx, n, "+", &["string", "template_string"]) => ArgShape::Concat,
        _ => ArgShape::Other,
    }
}

pub(crate) fn ts_args<'s>(cx: &Cx<'_, 's>, args: Node<'_>) -> Vec<Arg<'s>> {
    let items = match args.kind() {
        "template_string" => vec![args],
        _ => code_children(args),
    };
    items
        .into_iter()
        .map(|c| Arg {
            shape: ts_shape(cx, c),
            start_line: pos_of(c).line,
            end_line: narrow(c.end_position().row) + 1,
        })
        .collect()
}
