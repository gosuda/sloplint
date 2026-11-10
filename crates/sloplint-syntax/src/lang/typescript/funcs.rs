//! Function and closure heads for TypeScript.

use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use tree_sitter::Node;

use super::calls::ts_sole_call;
use crate::hooks::{
    Cx, FunctionHead, any_named_child, child_by_field, find_named_child, named_children, pos_of,
    preceding_comments, text,
};
use crate::lang::javascript::is_test_path;
pub(crate) struct TsHead<'a> {
    pub name: Node<'a>,
    pub node: Node<'a>,
    pub header: Node<'a>,
    pub visibility: Visibility,
    pub forced: bool,
    pub overload: bool,
    pub body: Option<Node<'a>>,
}

fn class_member(cx: &Cx<'_, '_>, member: Node<'_>) -> bool {
    let Some(body) = cx.parent_of(member).filter(|b| b.kind() == "class_body") else {
        return false;
    };
    cx.parent_of(body).is_some_and(|class| {
        matches!(
            class.kind(),
            "class_declaration" | "abstract_class_declaration"
        )
    })
}

pub(crate) fn member_public(cx: &Cx<'_, '_>, n: Node<'_>, name_kind: &str) -> bool {
    if name_kind == "private_property_identifier" {
        return false;
    }
    if any_named_child(n, |c| {
        c.kind() == "accessibility_modifier"
            && (text(cx, c) == "private" || text(cx, c) == "protected")
    }) {
        return false;
    }
    class_member(cx, n) && cx.nearest_class_exported().unwrap_or(false)
}

pub(crate) fn exported(cx: &Cx<'_, '_>, decl: Node<'_>, name: &str) -> bool {
    match cx.parent_of(decl).map(|p| p.kind()) {
        Some("export_statement") => true,
        Some("program") => cx.is_exported(name),
        _ => false,
    }
}

pub(crate) fn visibility_of(public: bool) -> Visibility {
    if public {
        Visibility::Exported
    } else {
        Visibility::Private
    }
}

pub(crate) fn has_override(n: Node<'_>) -> bool {
    find_named_child(n, |child| child.kind() == "override_modifier").is_some()
}

fn has_decorator(n: Node<'_>) -> bool {
    find_named_child(n, |child| child.kind() == "decorator").is_some()
}

fn is_deprecated_named(node: Node<'_>, doc: &[&str], cx: &Cx<'_, '_>) -> bool {
    if doc.iter().any(|line| line.contains("@deprecated")) {
        return true;
    }
    find_named_child(node, |child| child.kind() == "decorator")
        .is_some_and(|decorator| text(cx, decorator).contains("deprecated"))
}

fn strip_ty(ty: &str) -> &str {
    ty.trim_start_matches([':', ' ', '\t'])
}

fn primitive_of(ty: Option<&str>) -> PrimitiveKind {
    match ty.map(str::trim) {
        Some("string") => PrimitiveKind::Str,
        Some("boolean") => PrimitiveKind::Bool,
        Some("number") | Some("bigint") => PrimitiveKind::Int,
        _ => PrimitiveKind::Other,
    }
}

fn pattern_name<'s>(cx: &Cx<'_, 's>, pat: Node<'_>) -> (&'s str, bool) {
    if pat.kind() == "rest_pattern" {
        let name = find_named_child(pat, |child| child.kind() == "identifier")
            .map_or_else(|| text(cx, pat), |id| text(cx, id));
        return (name, true);
    }
    if pat.kind() == "identifier" {
        return (text(cx, pat), false);
    }
    (text(cx, pat), false)
}

fn default_of(child: Node<'_>) -> (bool, bool) {
    let Some(value) = child_by_field(child, "value") else {
        return (false, false);
    };
    (true, matches!(value.kind(), "true" | "false"))
}

fn push_required_param<'s>(cx: &Cx<'_, 's>, child: Node<'_>, out: &mut Vec<Param<'s>>) {
    push_param(cx, child, false, out);
}

fn push_optional_param<'s>(cx: &Cx<'_, 's>, child: Node<'_>, out: &mut Vec<Param<'s>>) {
    push_param(cx, child, true, out);
}

fn push_param<'s>(cx: &Cx<'_, 's>, child: Node<'_>, optional: bool, out: &mut Vec<Param<'s>>) {
    let Some(pat) = child_by_field(child, "pattern") else {
        return;
    };
    if pat.kind() == "this" {
        return;
    }
    let (name, variadic) = pattern_name(cx, pat);
    let ty = child_by_field(child, "type").map(|t| strip_ty(text(cx, t)));
    let (valued, bool_default) = default_of(child);
    out.push(Param {
        name,
        ty,
        primitive_kind: primitive_of(ty),
        has_default: valued || optional,
        bool_default,
        keyword_only: false,
        is_variadic: variadic,
    });
}

pub(crate) fn ts_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    for child in named_children(list) {
        match child.kind() {
            "required_parameter" => push_required_param(cx, child, out),
            "optional_parameter" => push_optional_param(cx, child, out),
            _ => {}
        }
    }
}

pub(crate) fn closure_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>, out: &mut Vec<Param<'s>>) {
    if let Some(list) = child_by_field(n, "parameters") {
        ts_params(cx, list, out);
    } else if let Some(single) = child_by_field(n, "parameter") {
        let (name, variadic) = pattern_name(cx, single);
        out.push(Param {
            name,
            ty: None,
            primitive_kind: PrimitiveKind::Other,
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: variadic,
        });
    }
}

pub(crate) fn missing_of<'s>(
    params: &[Param<'s>],
    ret: Option<&'s str>,
    name: &str,
) -> Vec<&'s str> {
    let mut missing: Vec<&'s str> = params
        .iter()
        .filter(|p| p.ty.is_none())
        .map(|p| p.name)
        .collect();
    if ret.is_none() && name != "constructor" {
        missing.push("return");
    }
    missing
}

pub(crate) fn top_level(cx: &Cx<'_, '_>) -> bool {
    matches!(
        cx.parent().map(|parent| parent.kind()),
        Some("program" | "export_statement")
    )
}

pub(crate) fn head_for<'s>(
    cx: &Cx<'_, 's>,
    site: TsHead<'_>,
    params: Vec<Param<'s>>,
) -> FunctionHead<'s> {
    let ret = child_by_field(site.node, "return_type").map(|t| text(cx, t));
    let name = text(cx, site.name);
    let missing = missing_of(&params, ret, name);
    let (doc, doc_pos) = preceding_comments(cx, site.header, &["comment"], &["decorator"]);
    let deprecated = is_deprecated_named(site.node, &doc, cx);
    FunctionHead {
        name,
        name_pos: pos_of(site.name),
        visibility: site.visibility,
        signature_forced: site.forced,
        entry_point: false,
        in_test: is_test_path(cx.path),
        is_overload: site.overload,
        is_deprecated: deprecated,
        is_decorated: has_decorator(site.node),
        is_trait_item: false,
        region: None,
        params,
        return_type: ret,
        missing_annotations: missing,
        doc,
        doc_pos,
        sole_call: site.body.and_then(ts_sole_call),
    }
}

pub(crate) fn doc_anchor<'t>(cx: &Cx<'t, '_>, n: Node<'t>) -> Node<'t> {
    let mut anchor = n;
    let mut current = cx.parent_of(anchor);
    while let Some(found) = current {
        if found.kind() == "export_statement" || found.kind() == "lexical_declaration" {
            anchor = found;
            current = cx.parent_of(found);
        } else {
            break;
        }
    }
    anchor
}

pub(crate) fn function_head<'s>(n: Node<'_>, cx: &Cx<'_, 's>) -> Option<FunctionHead<'s>> {
    let name_node = child_by_field(n, "name")?;
    let public = if n.kind() == "method_definition" {
        member_public(cx, n, name_node.kind())
    } else {
        exported(cx, n, text(cx, name_node))
    };
    let overload = n.kind() == "function_signature";
    if !overload && child_by_field(n, "body").is_none() {
        return None;
    }
    let mut params = Vec::new();
    if let Some(list) = child_by_field(n, "parameters") {
        ts_params(cx, list, &mut params);
    }
    let site = TsHead {
        name: name_node,
        node: n,
        header: doc_anchor(cx, n),
        visibility: visibility_of(public),
        forced: has_override(n),
        overload,
        body: child_by_field(n, "body"),
    };
    Some(head_for(cx, site, params))
}
