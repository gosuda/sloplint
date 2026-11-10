//! Function and closure heads for JavaScript.

use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use tree_sitter::Node;

use super::imports::is_test_path;
use super::{COMMENTS, first_code, header_of, parent_kind_is};
use crate::hooks::{
    Cx, FunctionHead, child_by_field, code_children, find_named_child, pos_of, preceding_comments,
    text,
};
use crate::lang::javascript::calls::sole_call_of;

pub(crate) struct HeadSite<'a> {
    pub name: Node<'a>,
    pub node: Node<'a>,
    pub header: Node<'a>,
    pub visibility: Visibility,
    pub body: Node<'a>,
}

pub(crate) fn declared_public(cx: &Cx<'_, '_>, decl: Node<'_>, name: Node<'_>) -> bool {
    parent_kind_is(cx, decl, "export_statement")
        || (parent_kind_is(cx, decl, "program") && cx.is_exported(text(cx, name)))
}

fn exported_class_member(cx: &Cx<'_, '_>, member: Node<'_>) -> bool {
    let class = cx
        .parent_of(member)
        .filter(|body| body.kind() == "class_body")
        .and_then(|body| cx.parent_of(body))
        .filter(|class| class.kind() == "class_declaration");
    class.is_some_and(|class| {
        child_by_field(class, "name").is_some_and(|name| declared_public(cx, class, name))
    })
}

pub(crate) fn visibility_of(public: bool) -> Visibility {
    if public {
        Visibility::Exported
    } else {
        Visibility::Private
    }
}

fn plain_param<'s>(cx: &Cx<'_, 's>, name: Node<'_>) -> Param<'s> {
    Param {
        name: text(cx, name),
        ty: None,
        primitive_kind: PrimitiveKind::Other,
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: false,
    }
}

fn formal_param<'s>(cx: &Cx<'_, 's>, param: Node<'_>) -> Option<Param<'s>> {
    match param.kind() {
        "identifier" | "object_pattern" | "array_pattern" => Some(plain_param(cx, param)),
        "assignment_pattern" => {
            let left = child_by_field(param, "left")?;
            let right = child_by_field(param, "right")?;
            Some(Param {
                name: text(cx, left),
                ty: None,
                primitive_kind: PrimitiveKind::Other,
                has_default: true,
                bool_default: matches!(right.kind(), "true" | "false"),
                keyword_only: false,
                is_variadic: false,
            })
        }
        "rest_pattern" => {
            let name = first_code(param).map_or_else(|| text(cx, param), |inner| text(cx, inner));
            Some(Param {
                name,
                ty: None,
                primitive_kind: PrimitiveKind::Other,
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: true,
            })
        }
        _ => None,
    }
}

pub(crate) fn formal_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>) -> Vec<Param<'s>> {
    code_children(list)
        .into_iter()
        .filter_map(|param| formal_param(cx, param))
        .collect()
}

struct ClosureSite<'a> {
    name: Node<'a>,
    visibility: Visibility,
    header: Node<'a>,
}

fn declarator_site<'a>(
    cx: &Cx<'a, '_>,
    n: Node<'a>,
    declarator: Node<'a>,
) -> Option<ClosureSite<'a>> {
    if child_by_field(declarator, "value")?.id() != n.id() {
        return None;
    }
    let name = child_by_field(declarator, "name").filter(|name| name.kind() == "identifier")?;
    let declaration = cx.parent_of(declarator)?;
    if !matches!(
        declaration.kind(),
        "lexical_declaration" | "variable_declaration"
    ) {
        return None;
    }
    let scope = cx.parent_of(declaration)?;
    let wrapped = match scope.kind() {
        "export_statement" => true,
        "program" => false,
        _ => return None,
    };
    let public = wrapped || cx.is_exported(text(cx, name));
    let header = if wrapped { scope } else { declaration };
    Some(ClosureSite {
        name,
        visibility: visibility_of(public),
        header,
    })
}

fn field_site<'a>(cx: &Cx<'_, '_>, n: Node<'a>, field: Node<'a>) -> Option<ClosureSite<'a>> {
    if child_by_field(field, "value")?.id() != n.id() {
        return None;
    }
    let name = child_by_field(field, "property")?;
    let public = exported_class_member(cx, field) && name.kind() != "private_property_identifier";
    Some(ClosureSite {
        name,
        visibility: visibility_of(public),
        header: field,
    })
}

fn closure_site<'a>(
    cx: &Cx<'a, '_>,
    n: Node<'a>,
    parent: Option<Node<'a>>,
) -> Option<ClosureSite<'a>> {
    let parent = parent?;
    match parent.kind() {
        "variable_declarator" => declarator_site(cx, n, parent),
        "field_definition" => field_site(cx, n, parent),
        _ => None,
    }
}

fn has_decorator(node: Node<'_>) -> bool {
    find_named_child(node, |child| child.kind() == "decorator").is_some()
}

fn is_deprecated_named(node: Node<'_>, doc: &[&str], cx: &Cx<'_, '_>) -> bool {
    if doc.iter().any(|line| line.contains("@deprecated")) {
        return true;
    }
    find_named_child(node, |child| child.kind() == "decorator")
        .is_some_and(|decorator| text(cx, decorator).contains("deprecated"))
}

pub(crate) fn head_for<'s>(
    cx: &Cx<'_, 's>,
    site: HeadSite<'_>,
    params: Vec<Param<'s>>,
) -> FunctionHead<'s> {
    let (doc, doc_pos) = preceding_comments(cx, site.header, COMMENTS, &[]);
    let deprecated = is_deprecated_named(site.node, &doc, cx);
    FunctionHead {
        name: text(cx, site.name),
        name_pos: pos_of(site.name),
        visibility: site.visibility,
        signature_forced: false,
        entry_point: false,
        in_test: is_test_path(cx.path),
        is_overload: false,
        is_deprecated: deprecated,
        is_decorated: has_decorator(site.node),
        is_trait_item: false,
        region: None,
        params,
        return_type: None,
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call_of(site.body),
    }
}

pub(crate) fn closure_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    if let Some(single) = child_by_field(n, "parameter") {
        return vec![plain_param(cx, single)];
    }
    child_by_field(n, "parameters").map_or_else(Vec::new, |list| formal_params(cx, list))
}

pub(crate) fn function_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
    let body = child_by_field(n, "body")?;
    let name = child_by_field(n, "name")?;
    let public = if n.kind() == "method_definition" {
        exported_class_member(cx, n) && name.kind() != "private_property_identifier"
    } else {
        declared_public(cx, n, name)
    };
    let params =
        child_by_field(n, "parameters").map_or_else(Vec::new, |list| formal_params(cx, list));
    let site = HeadSite {
        name,
        node: n,
        header: header_of(cx, n),
        visibility: visibility_of(public),
        body,
    };
    Some(head_for(cx, site, params))
}

pub(crate) fn closure_head<'s>(
    n: Node<'_>,
    parent: Option<Node<'_>>,
    cx: &Cx<'_, 's>,
) -> Option<FunctionHead<'s>> {
    let found = closure_site(cx, n, parent)?;
    let body = child_by_field(n, "body")?;
    let site = HeadSite {
        name: found.name,
        node: n,
        header: found.header,
        visibility: found.visibility,
        body,
    };
    Some(head_for(cx, site, closure_params(cx, n)))
}
