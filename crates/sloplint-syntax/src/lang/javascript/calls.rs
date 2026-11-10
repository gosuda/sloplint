use sloplint_core::facts::{Arg, ArgShape, CallKind, Param};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use super::{COMMENTS, HeadSite, first_code, formal_params};
use crate::hooks::{
    CallHead, Cx, FunctionHead, binary_concat, child_by_field, code_children, narrow,
    only_named_child_matching, pos_of, preceding_comments, sole_call_id, string_arg_shape, text,
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
fn in_test_scope(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    let mut cur = n.parent();
    while let Some(parent) = cur {
        match parent.kind() {
            "program" => return true,
            "expression_statement"
            | "export_statement"
            | "statement_block"
            | "block"
            | "arguments"
            | "arrow_function"
            | "function_expression"
            | "function" => cur = parent.parent(),
            "call_expression" => {
                let callee = child_by_field(parent, "function").map_or("", |func| text(cx, func));
                if is_suite_callee(callee) {
                    cur = parent.parent();
                } else {
                    return false;
                }
            }
            _ => return false,
        }
    }
    false
}
pub(crate) fn test_call_marker(cx: &Cx<'_, '_>, n: Node<'_>) -> Option<CodeRole> {
    if n.kind() != "call_expression" {
        return None;
    }
    let callee = child_by_field(n, "function").map_or("", |func| text(cx, func));
    if is_bench_callee(callee) {
        return in_test_scope(cx, n).then_some(CodeRole::Bench);
    }
    if is_case_callee(callee) {
        return in_test_scope(cx, n).then_some(CodeRole::Test);
    }
    None
}
pub(super) fn closure_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    if let Some(single) = child_by_field(n, "parameter") {
        return vec![Param {
            name: text(cx, single),
            ty: None,
            bool_default: false,
        }];
    }
    child_by_field(n, "parameters").map_or_else(Vec::new, |list| formal_params(cx, list))
}
pub(super) fn sole_call_of(body: Node<'_>) -> Option<usize> {
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
pub(super) fn head_for<'s>(
    cx: &Cx<'_, 's>,
    site: HeadSite<'_>,
    params: Vec<Param<'s>>,
) -> FunctionHead<'s> {
    let (doc, doc_pos) = preceding_comments(cx, site.header, COMMENTS, &[]);
    FunctionHead {
        name: text(cx, site.name),
        name_pos: pos_of(site.name),
        public: site.public,
        interface_bound: false,
        region: None,
        params,
        return_type: None,
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call_of(site.body),
    }
}
fn root_of_callee(n: Node<'_>) -> Option<Node<'_>> {
    let mut current = n;
    loop {
        current = match current.kind() {
            "identifier" | "this" | "super" => return Some(current),
            "member_expression" | "subscript_expression" => child_by_field(current, "object")?,
            "parenthesized_expression" => first_code(current)?,
            _ => return None,
        };
    }
}
fn arg_shape<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> ArgShape<'s> {
    match n.kind() {
        "string" | "template_string" => string_arg_shape(cx, n, &["string_fragment"]),
        "binary_expression" if binary_concat(cx, n, "+", &["string", "template_string"]) => {
            ArgShape::Concat
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
pub(super) fn call_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<CallHead<'s>> {
    match n.kind() {
        "call_expression" => {
            let function = child_by_field(n, "function")?;
            let args = child_by_field(n, "arguments").map_or_else(Vec::new, |a| call_args(cx, a));
            Some(CallHead {
                callee: text(cx, function),
                kind: CallKind::Plain,
                callee_root: root_of_callee(function).map(|root| text(cx, root)),
                args,
            })
        }
        "new_expression" => {
            let constructor = child_by_field(n, "constructor")?;
            let args = child_by_field(n, "arguments").map_or_else(Vec::new, |a| call_args(cx, a));
            Some(CallHead {
                callee: text(cx, constructor),
                kind: CallKind::New,
                callee_root: None,
                args,
            })
        }
        _ => None,
    }
}
