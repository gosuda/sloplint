use sloplint_core::facts::{Arg, ArgShape, AssertShape, Assertion, CallKind};
use tree_sitter::Node;

use crate::hooks::{
    CallHead, Cx, child_by_field, find_named_child, first_named_child_matching, first_of_kind,
    last_segment, named_children, narrow, pos_of, rfind_named_child, slice, string_arg_shape, text,
};

fn arg_expr(arg: Node<'_>) -> Node<'_> {
    let named = child_by_field(arg, "name").map(|v| v.id());
    rfind_named_child(arg, |c| {
        Some(c.id()) != named && c.kind() != "reference_modifier"
    })
    .unwrap_or(arg)
}

fn fragment_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>, body: &str, frag: &str) -> ArgShape<'s> {
    match find_named_child(n, |c| c.kind() == body) {
        Some(found) => {
            let parts: Vec<&'s str> = named_children(found)
                .into_iter()
                .filter(|c| c.kind() == frag)
                .map(|c| text(cx, c))
                .collect();
            ArgShape::Str {
                longest_static: sloplint_core::text::longest_static_chunk(&parts),
            }
        }
        None => ArgShape::Other,
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" | "encapsed_string" => string_arg_shape(cx, n, &["string_content"]),
        "heredoc" => fragment_shape(cx, n, "heredoc_body", "string_content"),
        "nowdoc" => fragment_shape(cx, n, "nowdoc_body", "nowdoc_string"),
        "binary_expression" => {
            let dot = child_by_field(n, "operator").is_some_and(|o| text(cx, o) == ".");
            let has_str = crate::hooks::any_named_child(n, |c| {
                matches!(
                    c.kind(),
                    "string" | "encapsed_string" | "heredoc" | "nowdoc"
                )
            });
            if dot && has_str {
                ArgShape::Concat
            } else {
                ArgShape::Other
            }
        }
        "array_creation_expression" => ArgShape::Collection,
        "anonymous_function" | "arrow_function" | "match_expression" => ArgShape::CodeBody,
        "variable_name" => ArgShape::Ident(text(cx, n).strip_prefix('$').unwrap_or(text(cx, n))),
        _ => ArgShape::Other,
    }
}

fn arg_nodes(n: Node<'_>) -> Vec<Node<'_>> {
    let Some(list) = child_by_field(n, "arguments").or_else(|| first_of_kind(n, &["arguments"]))
    else {
        return Vec::new();
    };
    named_children(list)
        .into_iter()
        .filter(|c| c.kind() == "argument")
        .collect()
}

fn call_args<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Arg<'s>> {
    arg_nodes(n)
        .into_iter()
        .map(|a| {
            let expr = arg_expr(a);
            let shape = if expr.kind() == "variadic_unpacking" {
                ArgShape::Other
            } else {
                arg_shape(cx, expr)
            };
            Arg {
                shape,
                start_line: narrow(a.start_position().row) + 1,
                end_line: narrow(a.end_position().row) + 1,
            }
        })
        .collect()
}

fn plain_root(callee: &str) -> Option<&str> {
    callee.split('\\').find(|seg| !seg.is_empty())
}

fn is_simple_static(n: Node<'_>) -> bool {
    child_by_field(n, "name").is_some_and(|v| v.kind() == "name")
}

fn is_simple_path(n: Node<'_>) -> bool {
    match n.kind() {
        "function_call_expression" => child_by_field(n, "function")
            .is_some_and(|f| f.kind() == "name" || f.kind() == "qualified_name"),
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            is_simple_static(n)
        }
        "object_creation_expression" => true,
        _ => false,
    }
}

fn initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "assignment_expression"
        && parent.kind() != "reference_assignment_expression"
    {
        return false;
    }
    if !child_by_field(parent, "right").is_some_and(|r| r.id() == n.id()) {
        return false;
    }
    child_by_field(parent, "left")
        .is_some_and(|l| l.kind() == "variable_name" || l.kind() == "list_literal")
}

fn object_root<'s>(cx: &Cx<'_, 's>, obj: Node<'_>) -> Option<&'s str> {
    if obj.kind() != "variable_name" && child_by_field(obj, "object").is_none() {
        return None;
    }
    let root = cx.chain_root(3, obj, |current| {
        if current.kind() == "variable_name" {
            crate::hooks::Link::Root(current)
        } else {
            match child_by_field(current, "object") {
                Some(next) => crate::hooks::Link::Next(next),
                None => crate::hooks::Link::Root(current),
            }
        }
    });
    let root = root.strip_prefix('$').unwrap_or(root);
    (!root.as_bytes().first().is_some_and(|b| b.is_ascii_digit())
        && root
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || !b.is_ascii()))
    .then_some(root)
}

fn scope_root<'s>(cx: &Cx<'_, 's>, scope: Node<'_>) -> Option<&'s str> {
    let t = text(cx, scope);
    Some(t.split('\\').next().unwrap_or(t))
}

fn callee_span<'s>(cx: &Cx<'_, 's>, n: Node<'_>, field: &str) -> (&'s str, CallKind) {
    match (child_by_field(n, field), child_by_field(n, "name")) {
        (Some(o), Some(m)) => (slice(cx.src, o.start_byte(), m.end_byte()), CallKind::Plain),
        _ => ("", CallKind::Plain),
    }
}

fn stmt_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (&'s str, CallKind) {
    match n.kind() {
        "member_call_expression" | "nullsafe_member_call_expression" => {
            callee_span(cx, n, "object")
        }
        "scoped_call_expression" => callee_span(cx, n, "scope"),
        "object_creation_expression" => (
            first_named_child_matching(n, &["name", "qualified_name", "relative_name"])
                .map(|c| text(cx, c))
                .unwrap_or(""),
            CallKind::New,
        ),
        _ => (
            child_by_field(n, "function")
                .map(|f| text(cx, f))
                .unwrap_or(""),
            CallKind::Plain,
        ),
    }
}

fn callee_last<'s>(cx: &Cx<'_, 's>, n: Node<'_>, callee: &'s str) -> &'s str {
    const SEPS: &[&str] = &["::", "->", ".", "\\"];
    let field = if n.kind() == "scoped_call_expression" {
        "scope"
    } else {
        "object"
    };
    let (Some(receiver), Some(name)) = (child_by_field(n, field), child_by_field(n, "name")) else {
        return last_segment(callee, SEPS);
    };
    let callee_tail = slice(cx.src, receiver.end_byte(), name.end_byte());
    last_segment(callee_tail, SEPS)
}

pub(crate) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    match n.kind() {
        "function_call_expression" => {
            let fun = child_by_field(n, "function")?;
            let callee = text(cx, fun);
            Some(CallHead {
                callee,
                kind: CallKind::Plain,
                callee_root: plain_root(callee),
                is_simple_path: is_simple_path(n),
                initializes_binding: initializes_binding(cx, n),
                args: call_args(cx, n),
            })
        }
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            let scoped = n.kind() == "scoped_call_expression";
            let field = if scoped { "scope" } else { "object" };
            let base = child_by_field(n, field)?;
            let (callee, kind) = stmt_callee(cx, n);
            if callee.is_empty() {
                return None;
            }
            let root = if scoped {
                scope_root(cx, base)
            } else {
                object_root(cx, base)
            };
            Some(CallHead {
                callee,
                kind,
                callee_root: root,
                is_simple_path: is_simple_path(n),
                initializes_binding: initializes_binding(cx, n),
                args: call_args(cx, n),
            })
        }
        "object_creation_expression" => {
            let (callee, kind) = stmt_callee(cx, n);
            if callee.is_empty() {
                return None;
            }
            Some(CallHead {
                callee,
                kind,
                callee_root: None,
                is_simple_path: is_simple_path(n),
                initializes_binding: initializes_binding(cx, n),
                args: call_args(cx, n),
            })
        }
        _ => None,
    }
}

fn two_operand(rest: &str) -> bool {
    matches!(
        rest,
        "Same"
            | "NotSame"
            | "Equals"
            | "NotEquals"
            | "Count"
            | "InstanceOf"
            | "MatchesRegularExpression"
            | "StringContainsString"
    ) || rest.starts_with("GreaterThan")
        || rest.starts_with("LessThan")
        || rest.starts_with("Contains")
}

pub(crate) fn assertion_head(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<Assertion> {
    let kind = n.kind();
    if kind != "function_call_expression"
        && kind != "member_call_expression"
        && kind != "scoped_call_expression"
        && kind != "nullsafe_member_call_expression"
    {
        return None;
    }
    let (callee, _) = stmt_callee(cx, n);
    let name = callee_last(cx, n, callee);
    if !name.starts_with("assert") {
        return None;
    }
    let two = two_operand(name.strip_prefix("assert").unwrap_or(name));
    let args = if child_by_field(n, "arguments").is_none() {
        return Some(Assertion {
            pos: pos_of(n),
            has_message: false,
            comparison: two,
            shape: if two {
                AssertShape::TopEqNe
            } else {
                AssertShape::Other
            },
            in_typecheck: false,
            value_is_bool_literal: false,
        });
    } else {
        arg_nodes(n)
    };
    let last_str = args.last().is_some_and(|a| {
        let expr = arg_expr(*a);
        expr.kind() == "string" || expr.kind() == "encapsed_string"
    });
    let first_bare = args.first().is_some_and(|a| {
        let expr = arg_expr(*a);
        expr.kind() == "variable_name"
    });
    Some(Assertion {
        pos: pos_of(n),
        has_message: args.len() > (if two { 2 } else { 1 }) && last_str,
        comparison: two,
        shape: if two {
            AssertShape::TopEqNe
        } else if first_bare {
            AssertShape::BareName
        } else {
            AssertShape::Other
        },
        in_typecheck: false,
        value_is_bool_literal: false,
    })
}

pub(crate) fn dynamic_head(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<&'static str> {
    if n.kind() == "dynamic_variable_name" {
        return Some("variable variable");
    }
    if n.kind() == "function_call_expression" {
        let fun = child_by_field(n, "function")?;
        if matches!(
            fun.kind(),
            "variable_name"
                | "dynamic_variable_name"
                | "string"
                | "encapsed_string"
                | "parenthesized_expression"
        ) {
            return Some("variable function");
        }
        let callee = text(cx, fun);
        if callee == "eval" {
            return Some("eval");
        }
        if callee.starts_with("call_user_func") || callee.starts_with("forward_static_call") {
            return Some("call_user_func");
        }
        return None;
    }
    if matches!(
        n.kind(),
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression"
    ) {
        let name = child_by_field(n, "name")?;
        if name.kind() != "name" {
            return Some("variable method name");
        }
    }
    None
}

pub(crate) fn first_arg_text<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    let list = child_by_field(n, "arguments")?;
    first_named_child_matching(list, &["argument"]).map(|a| text(cx, arg_expr(a)))
}

pub(crate) fn is_call_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function_call_expression"
            | "member_call_expression"
            | "scoped_call_expression"
            | "nullsafe_member_call_expression"
    )
}

pub(crate) fn creation_args(n: Node<'_>) -> Vec<Node<'_>> {
    arg_nodes(n)
}

pub(crate) fn call_stmt_callee<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (&'s str, CallKind) {
    stmt_callee(cx, n)
}

pub(crate) fn call_callee_last<'s>(cx: &Cx<'_, 's>, n: Node<'_>, callee: &'s str) -> &'s str {
    callee_last(cx, n, callee)
}
