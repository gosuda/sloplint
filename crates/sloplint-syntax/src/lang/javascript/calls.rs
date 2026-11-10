//! Calls, test markers, and argument shapes for JavaScript.

use sloplint_core::facts::{Arg, ArgShape, CallKind};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use super::first_code;
use crate::hooks::{
    CallHead, Cx, child_by_field, code_children, narrow, only_named_child_matching, sole_call_id,
    spine_has_call, string_arg_shape, text,
};

fn span_arg<'s>(n: Node<'_>, shape: ArgShape<'s>) -> Arg<'s> {
    Arg {
        shape,
        start_line: narrow(n.start_position().row).saturating_add(1),
        end_line: narrow(n.end_position().row).saturating_add(1),
    }
}

fn callee_root(callee: &str) -> &str {
    let end = callee.find(['.', '(', '[', ' ']).unwrap_or(callee.len());
    &callee[..end]
}

fn callee_member(callee: &str) -> Option<&str> {
    let root = callee_root(callee);
    let rest = callee.get(root.len() + 1..)?;
    if !callee[root.len()..].starts_with('.') {
        return None;
    }
    let end = rest.find(['.', '(', '[', ' ']).unwrap_or(rest.len());
    Some(&rest[..end])
}

fn is_suite_callee(callee: &str) -> bool {
    let root = callee_root(callee);
    matches!(root, "describe" | "suite" | "context" | "group")
        || callee_member(callee) == Some("describe")
}

fn is_case_callee(callee: &str) -> bool {
    let root = callee_root(callee);
    if !matches!(root, "it" | "test") {
        return false;
    }
    match callee_member(callee) {
        None => true,
        Some(member) => matches!(
            member,
            "only" | "skip" | "each" | "todo" | "failing" | "concurrent"
        ),
    }
}

fn is_bench_callee(callee: &str) -> bool {
    if callee_root(callee) != "bench" {
        return false;
    }
    match callee_member(callee) {
        None => true,
        Some(member) => matches!(member, "only" | "skip" | "todo"),
    }
}

pub(crate) fn test_call_marker(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<CodeRole> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = child_by_field(n, "function").map_or("", |func| text(cx, func));
    if is_bench_callee(callee) {
        return Some(CodeRole::Bench);
    }
    if is_case_callee(callee) || is_suite_callee(callee) {
        return Some(CodeRole::Test);
    }
    None
}

pub(crate) fn sole_call_of(body: Node<'_>) -> Option<usize> {
    sole_call_id(
        body,
        |block| only_named_child_matching(block, |c| c.kind() != "comment"),
        statement_call,
    )
}

fn statement_call(statement: Node<'_>) -> Option<usize> {
    if !matches!(
        statement.kind(),
        "return_statement" | "expression_statement"
    ) {
        return None;
    }
    let mut expression = first_code(statement)?;
    if expression.kind() == "await_expression" {
        expression = first_code(expression)?;
    }
    (expression.kind() == "call_expression").then(|| expression.id())
}

fn root_of_callee(n: Node<'_>) -> Option<Node<'_>> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" | "super" => return Some(current),
            "member_expression" => child_by_field(current, "object")?,
            "parenthesized_expression" => first_code(current)?,
            _ => return None,
        };
    }
}

pub(crate) fn simple_path(n: Node<'_>) -> bool {
    let mut current = n;
    loop {
        match current.kind() {
            "identifier" | "this" | "super" => return true,
            "member_expression" => {
                let property_simple = child_by_field(current, "property").is_some_and(|property| {
                    matches!(
                        property.kind(),
                        "identifier" | "property_identifier" | "private_property_identifier"
                    )
                });
                if !property_simple {
                    return false;
                }
                let Some(object) = child_by_field(current, "object") else {
                    return false;
                };
                current = object;
            }
            "parenthesized_expression" => return false,
            _ => return false,
        }
    }
}

pub(crate) fn initializes_binding(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let Some(parent) = cx.parent() else {
        return false;
    };
    if parent.kind() != "variable_declarator" {
        return false;
    }
    child_by_field(parent, "value").is_some_and(|value| value.id() == n.id())
        && child_by_field(parent, "name").is_some_and(|name| name.kind() == "identifier")
}

pub(crate) fn collapsed_callee<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> Option<&'s str> {
    if func.kind() != "member_expression" {
        return None;
    }
    let property = child_by_field(func, "property")?;
    if !matches!(
        property.kind(),
        "identifier" | "property_identifier" | "private_property_identifier"
    ) {
        return None;
    }
    spine_has_call(func, "object", &["parenthesized_expression"]).then_some(text(cx, property))
}

fn callee_text<'s>(cx: &Cx<'_, 's>, func: Node<'_>) -> &'s str {
    collapsed_callee(cx, func).unwrap_or_else(|| text(cx, func))
}

fn plus_operands<'n>(cx: &Cx<'_, '_>, n: Node<'n>) -> Option<(Node<'n>, Node<'n>)> {
    if n.kind() != "binary_expression" || n.child_count() != 3 {
        return None;
    }
    let (Some(left), Some(op), Some(right)) = (n.child(0), n.child(1), n.child(2)) else {
        return None;
    };
    (text(cx, op) == "+").then_some((left, right))
}

fn concat_static(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<u32> {
    plus_operands(cx, n)?;
    let mut longest = 0;
    let mut found = false;
    let mut chain = vec![n];
    while let Some(node) = chain.pop() {
        let mut current = node;
        while current.kind() == "parenthesized_expression" {
            let Some(inner) = first_code(current) else {
                break;
            };
            current = inner;
        }
        if let Some((left, right)) = plus_operands(cx, current) {
            chain.push(left);
            chain.push(right);
        } else if matches!(current.kind(), "string" | "template_string") {
            found = true;
            if let ArgShape::Str { longest_static } =
                string_arg_shape(cx, current, &["string_fragment"])
            {
                longest = longest.max(longest_static);
            }
        }
    }
    found.then_some(longest)
}

fn template_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    let longest_static = match string_arg_shape(cx, n, &["string_fragment"]) {
        ArgShape::Str { longest_static } => longest_static,
        _ => 0,
    };
    if code_children(n)
        .iter()
        .any(|child| child.kind() == "template_substitution")
    {
        ArgShape::Concat { longest_static }
    } else {
        ArgShape::Str { longest_static }
    }
}

fn array_shape<'s>(n: Node<'_>) -> ArgShape<'s> {
    if code_children(n).iter().any(|child| {
        matches!(
            child.kind(),
            "arrow_function"
                | "function_expression"
                | "generator_function"
                | "function"
                | "statement_block"
        )
    }) {
        ArgShape::CodeBody
    } else {
        ArgShape::Collection
    }
}

fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" => string_arg_shape(cx, n, &["string_fragment"]),
        "template_string" => template_shape(cx, n),
        "binary_expression" => match concat_static(cx, n) {
            Some(longest_static) => ArgShape::Concat { longest_static },
            None => ArgShape::Other,
        },
        "array" => array_shape(n),
        "object" => ArgShape::Other,
        "arrow_function" | "function_expression" | "generator_function" | "function" => {
            ArgShape::CodeBody
        }
        "identifier" => ArgShape::Ident(text(cx, n)),
        _ => ArgShape::Other,
    }
}

fn call_args<'s>(cx: &Cx<'_, 's>, arguments: Node<'_>) -> Vec<Arg<'s>> {
    if arguments.kind() == "template_string" {
        return vec![span_arg(arguments, arg_shape(cx, arguments))];
    }
    code_children(arguments)
        .into_iter()
        .map(|arg| span_arg(arg, arg_shape(cx, arg)))
        .collect()
}

fn plain_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    let function = child_by_field(n, "function")?;
    let args = child_by_field(n, "arguments").map_or_else(Vec::new, |a| call_args(cx, a));
    Some(CallHead {
        callee: callee_text(cx, function),
        kind: CallKind::Plain,
        callee_root: root_of_callee(function).map(|root| text(cx, root)),
        is_simple_path: simple_path(function),
        initializes_binding: initializes_binding(cx, n),
        args,
    })
}

fn new_call<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    let constructor = child_by_field(n, "constructor")?;
    let args = child_by_field(n, "arguments").map_or_else(Vec::new, |a| call_args(cx, a));
    Some(CallHead {
        callee: callee_text(cx, constructor),
        kind: CallKind::New,
        callee_root: root_of_callee(constructor).map(|root| text(cx, root)),
        is_simple_path: simple_path(constructor),
        initializes_binding: initializes_binding(cx, n),
        args,
    })
}

pub(super) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    match n.kind() {
        "call_expression" => plain_call(cx, n),
        "new_expression" => new_call(cx, n),
        _ => None,
    }
}
