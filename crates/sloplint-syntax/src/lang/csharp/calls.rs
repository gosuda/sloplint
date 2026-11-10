use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use super::{EQUALITY_ASSERTS, STRING_KINDS, cut_generics, seg_last};
use crate::hooks::{
    CallHead, Cx, binary_concat, child_by_field, find_named_child, first_named_child_matching,
    named_children, narrow, pos_of, string_arg_shape, text, trimmed,
};

pub(crate) fn callee_text<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    if func.kind() == "generic_name"
        && let Some(i) = first_named_child_matching(func, &["identifier"])
    {
        return text(cx, i);
    }
    cut_generics(text(cx, func))
}

fn last_name<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    if func.kind() == "member_access_expression"
        && let Some(m) = child_by_field(func, "name")
    {
        return text(cx, m);
    }
    if func.kind() == "generic_name"
        && let Some(i) = first_named_child_matching(func, &["identifier"])
    {
        return text(cx, i);
    }
    seg_last(trimmed(cx, func))
}

pub(crate) fn callee_root<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    let mut current = func;
    loop {
        current = match current.kind() {
            "identifier" | "this" | "base" => return Some(text(cx, current)),
            "generic_name" => {
                return first_named_child_matching(current, &["identifier"]).map(|i| text(cx, i));
            }
            "member_access_expression" => child_by_field(current, "expression")?,
            _ => return None,
        };
    }
}

fn is_simple_path(func: Node<'_>) -> bool {
    let mut current = func;
    loop {
        current = match current.kind() {
            "identifier" | "generic_name" | "this" | "base" => return true,
            "member_access_expression" => match child_by_field(current, "expression") {
                Some(next) => next,
                None => return false,
            },
            _ => return false,
        };
    }
}

fn declares_value(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent_of(n) else {
        return false;
    };
    if parent.kind() != "variable_declarator" {
        return false;
    }
    child_by_field(parent, "name").is_some()
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, e: Node<'_>) -> ArgShape<'s> {
    match e.kind() {
        "string_literal" => string_arg_shape(cx, e, &["string_literal_content"]),
        "verbatim_string_literal" => {
            let t = text(cx, e);
            let seg = t.get(2..t.len().saturating_sub(1)).unwrap_or(t);
            ArgShape::Str {
                longest_static: longest_static_chunk(&[seg]),
            }
        }
        "raw_string_literal" => string_arg_shape(cx, e, &["raw_string_content"]),
        "interpolated_string_expression" => string_arg_shape(cx, e, &["string_content"]),
        "binary_expression" => {
            if binary_concat(cx, e, "+", STRING_KINDS) {
                ArgShape::Concat
            } else {
                ArgShape::Other
            }
        }
        "identifier" => ArgShape::Ident(text(cx, e)),
        "lambda_expression" | "anonymous_method_expression" => ArgShape::CodeBody,
        "object_creation_expression"
        | "array_creation_expression"
        | "implicit_array_creation_expression"
        | "collection_expression"
        | "initializer_expression" => ArgShape::Collection,
        _ => ArgShape::Other,
    }
}

pub(crate) fn arg_expr(n: Node<'_>) -> Option<Node<'_>> {
    let named = child_by_field(n, "name").map(|m| m.id());
    find_named_child(n, |c| Some(c.id()) != named)
}

fn args_from_list<'s>(cx: &Cx<'_, 's>, list: Node<'_>) -> Vec<Arg<'s>> {
    named_children(list)
        .into_iter()
        .filter(|a| a.kind() == "argument")
        .map(|a| Arg {
            shape: arg_expr(a)
                .map(|e| arg_shape(cx, e))
                .unwrap_or(ArgShape::Other),
            start_line: narrow(a.start_position().row) + 1,
            end_line: narrow(a.end_position().row) + 1,
        })
        .collect()
}

fn arg_len(n: Node<'_>) -> u32 {
    let mut count = 0u32;
    if let Some(list) = child_by_field(n, "arguments") {
        for a in named_children(list) {
            if a.kind() == "argument" {
                count += 1;
            }
        }
    }
    count
}

pub(crate) fn first_arg(n: Node<'_>) -> Option<Node<'_>> {
    child_by_field(n, "arguments").and_then(|l| first_named_child_matching(l, &["argument"]))
}

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    if n.kind() == "invocation_expression" {
        let func = child_by_field(n, "function")?;
        return Some(CallHead {
            callee: callee_text(cx, func),
            kind: CallKind::Plain,
            callee_root: callee_root(cx, func),
            is_simple_path: is_simple_path(func),
            initializes_binding: declares_value(cx, n),
            args: child_by_field(n, "arguments")
                .map(|l| args_from_list(cx, l))
                .unwrap_or_default(),
        });
    }
    if n.kind() != "object_creation_expression" {
        return None;
    }
    let ty = child_by_field(n, "type")?;
    Some(CallHead {
        callee: cut_generics(text(cx, ty)),
        kind: CallKind::New,
        callee_root: None,
        is_simple_path: true,
        initializes_binding: declares_value(cx, n),
        args: child_by_field(n, "arguments")
            .map(|l| args_from_list(cx, l))
            .unwrap_or_default(),
    })
}

pub(crate) fn dynamic_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() != "invocation_expression" {
        return None;
    }
    let last = last_name(cx, child_by_field(n, "function")?);
    if last != "GetMethod"
        && last != "GetProperty"
        && last != "GetField"
        && last != "GetType"
        && last != "InvokeMember"
    {
        return None;
    }
    if first_arg(n)
        .and_then(arg_expr)
        .is_none_or(|e| STRING_KINDS.contains(&e.kind()))
    {
        return None;
    }
    if last == "InvokeMember" {
        Some("reflective member invocation")
    } else {
        Some("reflection lookup by computed name")
    }
}

pub(crate) fn assertion_site(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    if n.kind() != "invocation_expression" {
        return None;
    }
    let func = child_by_field(n, "function")?;
    if func.kind() != "member_access_expression" {
        return None;
    }
    let expr = child_by_field(func, "expression")?;
    let name = child_by_field(func, "name")
        .map(|m| text(cx, m))
        .unwrap_or("");
    let head = last_name(cx, expr);
    let debug = head == "Debug" && name == "Assert";
    if head != "Assert" && !debug {
        return None;
    }
    let index = match name {
        "True" | "False" | "IsTrue" | "IsFalse" | "Null" | "NotNull" | "IsNull" | "IsNotNull" => 1,
        "Equal" | "NotEqual" | "AreEqual" | "AreNotEqual" | "Same" | "NotSame" | "Contains"
        | "That" => 2,
        "Assert" if debug => 1,
        _ => return None,
    };
    let shape = match name {
        "True" | "False" | "IsTrue" | "IsFalse" | "Null" | "NotNull" | "IsNull" | "IsNotNull" => {
            if first_arg(n)
                .and_then(arg_expr)
                .is_some_and(|e| e.kind() == "identifier")
            {
                AssertShape::BareName
            } else {
                AssertShape::Other
            }
        }
        "Equal" | "NotEqual" | "AreEqual" | "AreNotEqual" => AssertShape::TopEqNe,
        _ => AssertShape::Other,
    };
    Some(Assertion {
        pos: pos_of(n),
        has_message: arg_len(n) > index,
        comparison: EQUALITY_ASSERTS.contains(&name),
        shape,
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}
