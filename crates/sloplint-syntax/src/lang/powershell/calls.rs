use sloplint_core::facts::{Arg, ArgShape, CallKind};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, any_named_child, binary_concat, find_named_child, first_named_child,
    first_named_child_matching, first_of_kind, named_children, narrow, slice, text,
};

use super::funcs::ps_var_name;

fn ps_string_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    if any_named_child(n, |c| {
        c.kind() == "variable" || c.kind() == "sub_expression"
    }) {
        let parts: Vec<&'s str> = named_children(n)
            .into_iter()
            .filter(|c| c.kind() != "variable" && c.kind() != "sub_expression")
            .map(|c| text(cx, c))
            .collect();
        ArgShape::Str {
            longest_static: longest_static_chunk(&parts),
        }
    } else {
        ArgShape::Str {
            longest_static: narrow(text(cx, n).chars().count().saturating_sub(2)),
        }
    }
}

fn ps_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string_literal" => ps_string_shape(cx, n),
        "generic_token" => ArgShape::Str {
            longest_static: narrow(text(cx, n).chars().count()),
        },
        "variable" => ArgShape::Ident(ps_var_name(text(cx, n))),
        "command_parameter" => ArgShape::Other,
        "additive_expression" => {
            if binary_concat(cx, n, "+", &["string_literal"]) {
                ArgShape::Concat
            } else {
                ArgShape::Other
            }
        }
        _ => ArgShape::Other,
    }
}

fn ps_span_arg<'s>(cx: &Cx<'_, 's>, shape_node: Node<'_>, span_node: Node<'_>) -> Arg<'s> {
    Arg {
        shape: ps_shape(cx, shape_node),
        start_line: narrow(span_node.start_position().row) + 1,
        end_line: narrow(span_node.end_position().row) + 1,
    }
}

fn ps_arg<'s>(cx: &Cx<'_, 's>, arg: Node<'_>) -> Arg<'s> {
    let value = unwrap_operand(arg);
    let shaped = if value.kind() == "variable" {
        value
    } else {
        arg
    };
    ps_span_arg(cx, shaped, arg)
}

fn pipeline_operand<'a>(cx: &Cx<'_, '_>, n: Node<'a>) -> Option<Node<'a>> {
    let parent = cx.parent()?;
    if parent.kind() != "pipeline_chain" {
        return None;
    }
    let mut prev = n.prev_named_sibling()?;
    while matches!(prev.kind(), "comment" | "redirections") {
        prev = prev.prev_named_sibling()?;
    }
    Some(prev)
}

fn unwrap_operand(mut n: Node<'_>) -> Node<'_> {
    loop {
        let transparent = matches!(
            n.kind(),
            "logical_expression"
                | "bitwise_expression"
                | "comparison_expression"
                | "additive_expression"
                | "multiplicative_expression"
                | "format_expression"
                | "range_expression"
                | "array_literal_expression"
                | "unary_expression"
                | "parenthesized_expression"
                | "pipeline"
                | "pipeline_chain"
        );
        if !transparent || n.named_child_count() != 1 {
            break;
        }
        let Some(one) = n.named_child(0) else {
            break;
        };
        if one.kind() == "comment" {
            break;
        }
        n = one;
    }
    n
}

pub(crate) fn ps_command_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    let name = first_named_child_matching(n, &["command_name", "command_name_expr"])?;
    let callee = text(cx, name);
    let mut args = Vec::new();
    match first_of_kind(n, &["command_elements"]) {
        Some(list) => {
            for arg in named_children(list) {
                if arg.kind() != "comment" && arg.kind() != "command_argument_sep" {
                    args.push(ps_arg(cx, arg));
                }
            }
        }
        None => {
            for child in named_children(n) {
                if child.id() != name.id() && child.kind() != "comment" {
                    args.push(ps_arg(cx, child));
                }
            }
        }
    }
    if args.is_empty()
        && let Some(operand) = pipeline_operand(cx, n)
    {
        args.push(ps_span_arg(cx, unwrap_operand(operand), operand));
    }
    Some(CallHead {
        callee,
        kind: CallKind::Plain,
        callee_root: Some(callee),
        is_simple_path: true,
        initializes_binding: false,
        args,
    })
}

fn invokation_receiver(n: Node<'_>) -> Option<Node<'_>> {
    find_named_child(n, |c| {
        c.kind() != "member_name" && c.kind() != "argument_list"
    })
}

fn ps_receiver_root<'s>(cx: &Cx<'_, 's>, recv: Node<'_>) -> Option<&'s str> {
    let mut current = recv;
    loop {
        match current.kind() {
            "variable" => return Some(ps_var_name(text(cx, current))),
            "member_access" | "element_access" => {
                let next = first_named_child(current)?;
                current = next
            }
            _ => return None,
        }
    }
}

pub(crate) fn ps_invokation_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    let member = first_of_kind(n, &["member_name"])?;
    let member_text = text(cx, member);
    let recv = invokation_receiver(n);
    let mut kind = CallKind::Plain;
    let callee: &'s str = match recv {
        Some(r) => {
            if member_text == "new" && r.kind() == "type_literal" {
                kind = CallKind::New;
                let t = text(cx, r);
                t.strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .unwrap_or(t)
            } else {
                slice(cx.src, r.start_byte(), member.end_byte())
            }
        }
        None => member_text,
    };
    let mut args = Vec::new();
    if let Some(list) = first_of_kind(n, &["argument_list"]) {
        let exprs = first_of_kind(list, &["argument_expression_list"]).unwrap_or(list);
        for arg in named_children(exprs) {
            if arg.kind() != "comment" {
                args.push(ps_arg(cx, arg));
            }
        }
    }
    Some(CallHead {
        callee,
        kind,
        callee_root: recv.and_then(|r| ps_receiver_root(cx, r)),
        is_simple_path: false,
        initializes_binding: false,
        args,
    })
}

pub(crate) fn ps_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    if n.kind() == "command" {
        return ps_command_call(cx, n);
    }
    if n.kind() == "invokation_expression" {
        return ps_invokation_call(cx, n);
    }
    None
}

fn subtree_has_kind(n: Node<'_>, kind: &str) -> bool {
    let mut stack = vec![n];
    while let Some(current) = stack.pop() {
        if current.kind() == kind {
            return true;
        }
        stack.extend(named_children(current));
    }
    false
}

pub(crate) fn ps_dynamic(n: Node<'_>, cx: &Cx<'_, '_>) -> Option<&'static str> {
    if n.kind() == "command" {
        let name = first_named_child_matching(n, &["command_name", "command_name_expr"])?;
        let t = text(cx, name);
        if t.eq_ignore_ascii_case("invoke-expression") || t.eq_ignore_ascii_case("iex") {
            return Some("Invoke-Expression");
        }
        if name.kind() == "command_name_expr" && subtree_has_kind(name, "variable") {
            return Some("computed command name");
        }
        return None;
    }
    if n.kind() == "invokation_expression" {
        let member = first_of_kind(n, &["member_name"])?;
        if text(cx, member) != "Create" {
            return None;
        }
        let recv = invokation_receiver(n)?;
        if text(cx, recv).eq_ignore_ascii_case("[scriptblock]") {
            return Some("scriptblock creation");
        }
    }
    None
}
