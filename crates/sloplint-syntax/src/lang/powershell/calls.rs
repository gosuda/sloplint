use sloplint_core::facts::{Arg, ArgShape, CallKind};
use sloplint_core::text::longest_static_chunk;
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, any_named_child, child_by_field, find_named_child, first_named_child,
    first_named_child_matching, first_of_kind, named_children, narrow, slice, text, trimmed,
};

use super::funcs::{ps_var_name, single_named};

fn ps_string_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let fallback = narrow(text(cx, n).chars().count().saturating_sub(2));
    let Some(inner) = first_of_kind(
        n,
        &[
            "expandable_string_literal",
            "expandable_here_string_literal",
            "verbatim_here_string_characters",
        ],
    ) else {
        return ArgShape::Str {
            longest_static: fallback,
        };
    };
    if inner.kind() == "verbatim_here_string_characters" {
        return ArgShape::Str {
            longest_static: ps_verbatim_here_len(text(cx, inner)),
        };
    }
    if inner.kind() == "expandable_here_string_literal" {
        return ArgShape::Concat {
            longest_static: ps_static_run(cx, inner),
        };
    }
    let interpolated = any_named_child(inner, |c| {
        c.kind() == "variable" || c.kind() == "sub_expression"
    });
    if interpolated {
        return ArgShape::Concat {
            longest_static: ps_static_run(cx, inner),
        };
    }
    let raw = text(cx, inner);
    let body = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);
    ArgShape::Str {
        longest_static: longest_static_chunk(&[body]),
    }
}

fn ps_verbatim_here_len(raw: &str) -> u32 {
    let start = raw.find('\n').map(|i| i + 1).unwrap_or(raw.len());
    let end = raw.rfind("'@").unwrap_or(start).min(raw.len());
    let start = start.min(end);
    narrow(raw.get(start..end).map_or(0, |body| {
        body.trim_end_matches(['\r', '\n']).chars().count()
    }))
}

fn ps_static_run(cx: &Cx<'_, '_>, inner: Node<'_>) -> u32 {
    let dynamics: Vec<Node<'_>> = named_children(inner)
        .into_iter()
        .filter(|c| c.kind() == "variable" || c.kind() == "sub_expression")
        .collect();
    let raw = text(cx, inner);
    let mut start = inner.start_byte().saturating_add(1);
    let mut end = inner.end_byte().saturating_sub(1);
    if raw.starts_with("@\"") {
        if let Some(nl) = raw.find('\n') {
            start = inner.start_byte() + nl + 1;
        }
        if let Some(close) = raw.rfind("\"@") {
            let before = slice(cx.src, inner.start_byte(), inner.start_byte() + close);
            end = inner.start_byte() + before.trim_end_matches(['\r', '\n']).len();
        }
    }
    let mut gaps: Vec<&str> = Vec::new();
    let mut cursor = start.min(end);
    for dynamic in &dynamics {
        gaps.push(slice(
            cx.src,
            cursor.min(dynamic.start_byte()),
            dynamic.start_byte(),
        ));
        cursor = cursor.max(dynamic.end_byte());
    }
    gaps.push(slice(cx.src, cursor.min(end), end));
    longest_static_chunk(&gaps)
}

fn ps_concat_static(cx: &Cx<'_, '_>, n: Node<'_>) -> u32 {
    let mut best = 0;
    let mut stack = vec![n];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "string_literal" => {
                let run = match ps_string_shape(cx, current) {
                    ArgShape::Str { longest_static } | ArgShape::Concat { longest_static } => {
                        longest_static
                    }
                    _ => 0,
                };
                best = best.max(run);
            }
            "parenthesized_expression"
            | "sub_expression"
            | "script_block_expression"
            | "array_expression" => {}
            _ => stack.extend(named_children(current)),
        }
    }
    best
}

fn ps_collection_shape(n: Node<'_>) -> ArgShape<'static> {
    if subtree_has_kind(n, "script_block_expression") {
        ArgShape::CodeBody
    } else {
        ArgShape::Collection
    }
}

fn ps_array_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let inner = unwrap_operand(n);
    if inner.kind() == "string_literal" {
        ps_string_shape(cx, inner)
    } else if inner.kind() == "array_literal_expression"
        && first_of_kind(inner, &["generic_token"]).is_some()
    {
        ArgShape::Other
    } else if inner.named_child_count() > 1 {
        ps_collection_shape(inner)
    } else {
        ArgShape::Other
    }
}

pub(crate) fn ps_arg_value(n: Node<'_>) -> Node<'_> {
    let mut node = n;
    for _ in 0..16 {
        if !matches!(
            node.kind(),
            "argument_expression"
                | "logical_argument_expression"
                | "bitwise_argument_expression"
                | "comparison_argument_expression"
                | "additive_argument_expression"
                | "multiplicative_argument_expression"
                | "format_argument_expression"
                | "range_argument_expression"
                | "logical_expression"
                | "bitwise_expression"
                | "comparison_expression"
                | "additive_expression"
                | "multiplicative_expression"
                | "format_expression"
                | "range_expression"
                | "unary_expression"
                | "array_literal_expression"
                | "parenthesized_expression"
                | "pipeline"
                | "pipeline_chain"
        ) {
            break;
        }
        let Some(one) = single_named(node) else {
            break;
        };
        node = one;
    }
    node
}

fn ps_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let node = ps_arg_value(n);
    match node.kind() {
        "string_literal" => ps_string_shape(cx, node),
        "generic_token" => ArgShape::Str {
            longest_static: narrow(text(cx, node).chars().count()),
        },
        "variable" => ArgShape::Ident(ps_var_name(text(cx, node))),
        "command_parameter" => ArgShape::Other,
        "additive_expression" | "additive_argument_expression" => {
            if ps_spine_is_plus(cx, node) {
                ArgShape::Concat {
                    longest_static: ps_concat_static(cx, node),
                }
            } else {
                ArgShape::Other
            }
        }
        "array_literal_expression" => ps_array_shape(cx, node),
        "array_expression" => ps_collection_shape(node),
        "format_expression" | "format_argument_expression" => ArgShape::Concat {
            longest_static: ps_concat_static(cx, node),
        },
        _ => ArgShape::Other,
    }
}

fn ps_spine_is_plus(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let mut cursor = n.walk();
    n.children(&mut cursor)
        .any(|child| trimmed(cx, child) == "+")
}

fn ps_span_arg<'s>(cx: &Cx<'_, 's>, shape_node: Node<'_>, span_node: Node<'_>) -> Arg<'s> {
    Arg {
        shape: ps_shape(cx, shape_node),
        start_line: narrow(span_node.start_position().row) + 1,
        end_line: narrow(span_node.end_position().row) + 1,
    }
}

fn ps_arg<'s>(cx: &Cx<'_, 's>, arg: Node<'_>) -> Arg<'s> {
    ps_span_arg(cx, arg, arg)
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
                | "array_expression"
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

fn has_dynamic_name(n: Node<'_>) -> bool {
    any_named_child(n, |c| {
        c.kind() == "variable" || c.kind() == "sub_expression"
    })
}

fn ps_command_callee<'s>(name: Node<'_>, callee: &'s str) -> (Option<&'s str>, bool) {
    let kind = name.kind();
    if kind != "command_name" && kind != "command_name_expr" {
        return (None, false);
    }
    if has_dynamic_name(name) {
        return (None, false);
    }
    if kind == "command_name_expr" {
        let inner = first_named_child(name);
        if inner.is_none_or(|c| c.kind() != "command_name") {
            return (None, false);
        }
        if inner.is_some_and(has_dynamic_name) {
            return (None, false);
        }
    }
    (Some(callee), true)
}

fn ps_plain_variable_target(parent: Node<'_>) -> bool {
    let Some(left) = first_of_kind(parent, &["left_assignment_expression"]) else {
        return false;
    };
    let mut node = left;
    for _ in 0..16 {
        let Some(one) = single_named(node) else {
            break;
        };
        node = one;
    }
    node.kind() == "variable"
}

fn ps_reaches_target(mut node: Node<'_>, target: Node<'_>) -> bool {
    for _ in 0..16 {
        if node.id() == target.id() {
            return true;
        }
        let Some(one) = single_named(node) else {
            return false;
        };
        node = one;
    }
    false
}

fn ps_initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let mut current = n;
    for _ in 0..16 {
        let Some(parent) = cx.parent_of(current) else {
            return false;
        };
        if parent.kind() == "assignment_expression" {
            return child_by_field(parent, "value").is_some_and(|v| ps_reaches_target(v, current))
                && ps_plain_variable_target(parent);
        }
        if single_named(parent).is_none_or(|one| one.id() != current.id()) {
            return false;
        }
        current = parent;
    }
    false
}

fn ps_other_arg<'s>(arg: Node<'_>) -> Arg<'s> {
    Arg {
        shape: ArgShape::Other,
        start_line: narrow(arg.start_position().row) + 1,
        end_line: narrow(arg.end_position().row) + 1,
    }
}

pub(crate) fn ps_command_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    let name = first_named_child_matching(n, &["command_name", "command_name_expr"])?;
    let callee = text(cx, name);
    let mut args = Vec::new();
    match first_of_kind(n, &["command_elements"]) {
        Some(list) => {
            let mut prev_param = false;
            for arg in named_children(list) {
                if arg.kind() == "comment" || arg.kind() == "command_argument_sep" {
                    continue;
                }
                let bare_value = prev_param
                    && arg.kind() == "generic_token"
                    && !text(cx, arg)
                        .bytes()
                        .any(|b| matches!(b, b'$' | b'"' | b'\'' | b'`' | b'=' | b'/' | b'\\'));
                prev_param = arg.kind() == "command_parameter";
                if bare_value {
                    args.push(ps_other_arg(arg));
                } else {
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
    let (callee_root, is_simple_path) = ps_command_callee(name, callee);
    Some(CallHead {
        callee,
        kind: CallKind::Plain,
        callee_root,
        is_simple_path,
        initializes_binding: ps_initializes_binding(cx, n),
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
            "member_access" => {
                let next = first_named_child(current)?;
                current = next
            }
            _ => return None,
        }
    }
}

fn ps_invokation_simple(recv: Option<Node<'_>>) -> bool {
    let Some(mut current) = recv else {
        return false;
    };
    while current.kind() == "member_access" {
        let Some(next) = first_named_child(current) else {
            return false;
        };
        current = next;
    }
    matches!(current.kind(), "variable" | "type_literal")
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
            } else if r.kind() == "invokation_expression" {
                member_text
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
        is_simple_path: ps_invokation_simple(recv),
        initializes_binding: ps_initializes_binding(cx, n),
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
