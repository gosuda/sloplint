use sloplint_core::facts::{Pos, Visibility};
use sloplint_core::lang::Lang;
use tree_sitter::Node;

use super::OcamlHooks;
use super::params::{is_let_binding, is_parameter, missing_of, ocaml_params, spec_signature};
use crate::hooks::{
    Cx, FunctionHead, any_named_child, child_by_field, find_named_child, first_named_child,
    first_of_kind, pos_of, preceding_comments, text,
};

fn impl_bound(cx: &Cx<'_, '_>) -> bool {
    cx.nearest_impl_interface_bound().unwrap_or(false)
}

pub(crate) fn pattern_ident<'s>(cx: &Cx<'_, 's>, pat: Node<'_>) -> Option<(&'s str, Pos)> {
    if matches!(pat.kind(), "identifier" | "value_name") {
        return Some((text(cx, pat), pos_of(pat)));
    }
    let target = if pat.kind() == "value_pattern" {
        pat
    } else {
        first_of_kind(pat, &["value_pattern"])?
    };
    let id = find_named_child(target, |c| matches!(c.kind(), "identifier" | "value_name"));
    match id {
        Some(id) => Some((text(cx, id), pos_of(id))),
        None if target.named_child_count() == 0 => Some((text(cx, target), pos_of(target))),
        None => None,
    }
}

fn ocaml_header<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (Vec<&'s str>, Option<Pos>) {
    if let Some(parent) = cx.parent_of(n)
        && parent.kind() == "value_definition"
    {
        let first = find_named_child(parent, |c| is_let_binding(&c));
        if first.is_some_and(|f| f.id() != n.id()) {
            return (Vec::new(), None);
        }
        return preceding_comments(cx, parent, &["comment"], &[]);
    }
    preceding_comments(cx, n, &["comment"], &[])
}

fn ocaml_sole(_cx: &Cx<'_, '_>, body: Node<'_>) -> Option<usize> {
    let mut inner = body;
    if inner.kind() == "parenthesized_expression" {
        inner = first_named_child(inner)?;
    }
    (inner.kind() == "application_expression").then_some(inner.id())
}

fn let_head<'s>(
    cx: &Cx<'_, 's>,
    lb: Node<'_>,
    params_src: Node<'_>,
    body: Node<'_>,
    visibility: Visibility,
) -> FunctionHead<'s> {
    let name_node = child_by_field(lb, "pattern");
    let params = ocaml_params(cx, params_src);
    let return_type = child_by_field(lb, "type").map(|t| text(cx, t));
    let missing = missing_of(&params, return_type);
    let (doc, doc_pos) = ocaml_header(cx, lb);
    FunctionHead {
        name: name_node.map(|p| text(cx, p)).unwrap_or(""),
        name_pos: name_node.map(pos_of).unwrap_or_else(|| pos_of(lb)),
        visibility,
        signature_forced: impl_bound(cx),
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params,
        return_type,
        missing_annotations: missing,
        doc,
        doc_pos,
        sole_call: ocaml_sole(cx, body),
    }
}

pub(crate) fn function_head<'s>(
    hooks: &OcamlHooks,
    n: Node<'_>,
    cx: &Cx<'_, 's>,
) -> Option<FunctionHead<'s>> {
    match n.kind() {
        "let_binding" => {
            if cx.lang != Lang::Ocaml {
                return None;
            }
            let body = child_by_field(n, "body")?;
            let closure = body.kind() == "fun_expression" || body.kind() == "function_expression";
            let typed = any_named_child(n, |c| is_parameter(&c));
            if closure || !typed {
                return None;
            }
            Some(let_head(cx, n, n, body, hooks.visibility_at(cx, n)))
        }
        "method_definition" => {
            let name_node = child_by_field(n, "method_name")?;
            let body = child_by_field(n, "body")?;
            let mut head = let_head(cx, n, n, body, Visibility::Exported);
            head.name = text(cx, name_node);
            head.name_pos = pos_of(name_node);
            Some(head)
        }
        "value_specification" => {
            let name_node = child_by_field(n, "value_name")?;
            let (params, return_type) = spec_signature(cx, n);
            let (doc, doc_pos) = preceding_comments(cx, n, &["comment"], &[]);
            Some(FunctionHead {
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
                params,
                return_type,
                missing_annotations: Vec::new(),
                doc,
                doc_pos,
                sole_call: None,
            })
        }
        _ => None,
    }
}

pub(crate) fn closure_head<'s>(
    hooks: &OcamlHooks,
    n: Node<'_>,
    cx: &Cx<'_, 's>,
) -> Option<FunctionHead<'s>> {
    if n.kind() != "fun_expression" && n.kind() != "function_expression" {
        return None;
    }
    let parent = cx.parent()?;
    if parent.kind() != "let_binding" {
        return None;
    }
    let body = child_by_field(parent, "body");
    if body.is_some_and(|b| b.id() != n.id()) {
        return None;
    }
    Some(let_head(
        cx,
        parent,
        n,
        child_by_field(n, "body")?,
        hooks.visibility_at(cx, parent),
    ))
}
