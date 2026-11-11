//! ObjC function and method heads.

use sloplint_core::facts::{Param, Visibility};
use tree_sitter::Node;

use crate::hooks::{
    Cx, FunctionHead, child_by_field, find_named_child, first_named_child,
    first_named_child_matching, named_children, only_named_child_matching, pos_of,
    preceding_comments, rfind_named_child, text,
};
use crate::lang::c_family::{self, C_DECLARATORS, defaulted_param, span, word};

pub(crate) fn c_function<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let body = child_by_field(n, "body")?;
    let (fd, name_node) = c_family::func_parts(n, &C_DECLARATORS, &["identifier"])?;
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
    let public = !word(cx, n, &["storage_class_specifier"], &["static"]);
    let entry_point = text(cx, name_node) == "main" && !nested_function_scope(cx);
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name: text(cx, name_node),
        name_pos: pos_of(name_node),
        visibility: if public {
            Visibility::Exported
        } else {
            Visibility::Private
        },
        signature_forced: false,
        entry_point,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        receiver: None,
        params: c_family::params(cx, fd, &C_DECLARATORS, &["parameter_declaration"]),
        return_type: c_return(cx, n, fd),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call(body, &["call_expression"]),
    })
}
fn nested_function_scope(cx: &Cx<'_, '_>) -> bool {
    cx.ancestors().take(8).any(|a| {
        matches!(
            a.kind(),
            "function_definition" | "method_definition" | "block_literal"
        )
    })
}
fn c_return<'s>(cx: &Cx<'_, 's>, n: Node<'_>, fd: Node<'_>) -> Option<&'s str> {
    let ty = child_by_field(n, "type")?;
    Some(span(cx, ty, fd))
}
fn parens(raw: &str) -> &str {
    if raw.starts_with('(') && raw.ends_with(')') && raw.len() >= 2 {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}
pub(crate) fn method<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let body = first_named_child_matching(n, &["compound_statement"])?;
    let name_node = selector_name(n)?;
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name: text(cx, name_node),
        name_pos: pos_of(name_node),
        visibility: Visibility::Exported,
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        receiver: None,
        params: method_params(cx, n),
        return_type: method_return(cx, n),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call(body, &["call_expression", "message_expression"]),
    })
}
fn selector_name(n: Node<'_>) -> Option<Node<'_>> {
    match find_named_child(n, |c| c.kind() == "keyword_declarator") {
        Some(kd) => first_named_child_matching(kd, &["identifier"]),
        None => first_named_child_matching(n, &["identifier"]),
    }
}
fn method_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    for c in named_children(n) {
        let found = match c.kind() {
            "keyword_declarator" => keyword_param(cx, c),
            "method_parameter" => method_param(cx, c),
            _ => None,
        };
        if let Some(found) = found {
            out.push(found);
        }
    }
    out
}
fn keyword_param<'s>(cx: &Cx<'_, 's>, kd: Node<'_>) -> Option<Param<'s>> {
    let name_node = rfind_named_child(kd, |c| c.kind() == "identifier")?;
    let ty = first_named_child_matching(kd, &["method_type"]).map(|t| parens(text(cx, t)));
    Some(defaulted_param(cx, kd, text(cx, name_node), ty))
}
fn method_param<'s>(cx: &Cx<'_, 's>, mp: Node<'_>) -> Option<Param<'s>> {
    let name_node = first_named_child_matching(mp, &["identifier"])?;
    let ty = child_by_field(mp, "type").map(|t| text(cx, t));
    Some(defaulted_param(cx, mp, text(cx, name_node), ty))
}
fn method_return<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<&'s str> {
    first_named_child_matching(n, &["method_type"]).map(|t| parens(text(cx, t)))
}
fn sole_call(body: Node<'_>, kinds: &[&str]) -> Option<usize> {
    if body.kind() != "compound_statement" {
        return None;
    }
    let stmt = only_named_child_matching(body, |c| c.kind() != "comment")?;
    if stmt.kind() != "expression_statement" && stmt.kind() != "return_statement" {
        return None;
    }
    let expr = child_by_field(stmt, "expression").or_else(|| first_named_child(stmt))?;
    if expr.kind() != "call_expression" && expr.kind() != "message_expression" {
        return None;
    }
    kinds.contains(&expr.kind()).then_some(expr.id())
}
