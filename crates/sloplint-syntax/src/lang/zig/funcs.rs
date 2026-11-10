use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use tree_sitter::Node;

use crate::hooks::{
    Container, Cx, FunctionHead, child_by_field, find_named_child, first_named_child,
    first_of_kind, named_children, only_named_child_matching, pos_of, preceding_comments, text,
    trimmed,
};

fn is_pub_token(c: Node<'_>) -> bool {
    !c.is_named() && c.kind() == "pub"
}

pub(crate) fn has_pub(n: Node<'_>) -> bool {
    (0..n.child_count()).any(|i| n.child(i).is_some_and(is_pub_token))
}

pub(crate) fn zig_visibility(n: Node<'_>, cx: &Cx<'_, '_>) -> Visibility {
    let exported = cx.nearest_class_exported().unwrap_or(true);
    if has_pub(n) && exported {
        Visibility::Exported
    } else {
        Visibility::Private
    }
}

pub(crate) fn sole_call_id(_cx: &Cx<'_, '_>, body: Node<'_>) -> Option<usize> {
    if body.kind() != "block" {
        return None;
    }
    let mut node = only_named_child_matching(body, |c| c.kind() != "comment")?;
    if node.kind() == "expression_statement" {
        node = find_named_child(node, |c| c.kind() != "comment")?;
    }
    if node.kind() == "return_expression" || node.kind() == "try_expression" {
        node = first_named_child(node)?;
    }
    (node.kind() == "call_expression").then_some(node.id())
}

fn zig_param<'s>(cx: &Cx<'_, 's>, child: Node<'_>) -> Option<Param<'s>> {
    match child_by_field(child, "name") {
        Some(decl) => {
            let ty = child_by_field(child, "type").map(|t| text(cx, t));
            let primitive_kind = if ty == Some("bool") {
                PrimitiveKind::Bool
            } else {
                PrimitiveKind::Other
            };
            Some(Param {
                name: text(cx, decl),
                ty,
                primitive_kind,
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            })
        }
        None if trimmed(cx, child) == "..." => Some(Param {
            name: "...",
            ty: None,
            primitive_kind: PrimitiveKind::Other,
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: true,
        }),
        None => None,
    }
}

fn is_method_receiver(cx: &Cx<'_, '_>, params: &[Param<'_>]) -> bool {
    let last = cx.innermost_container();
    let method = matches!(last, Some(Container::Class { .. }));
    method && params.first().is_some_and(|p| p.name == "self")
}

pub(crate) fn zig_function<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
    if n.kind() != "function_declaration" {
        return None;
    }
    let name_node = crate::hooks::child_by_field(n, "name")?;
    let body = crate::hooks::child_by_field(n, "body")?;
    let mut params = Vec::new();
    if let Some(holder) = first_of_kind(n, &["parameters"]) {
        for child in named_children(holder) {
            if child.kind() != "parameter" {
                continue;
            }
            if let Some(param) = zig_param(cx, child) {
                params.push(param);
            }
        }
    }
    if is_method_receiver(cx, &params) {
        params.remove(0);
    }
    let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
    let name = text(cx, name_node);
    Some(FunctionHead {
        name,
        name_pos: pos_of(name_node),
        visibility: zig_visibility(n, cx),
        signature_forced: false,
        entry_point: name == "main",
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params,
        return_type: crate::hooks::child_by_field(n, "type").map(|t| text(cx, t)),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call_id(cx, body),
    })
}
