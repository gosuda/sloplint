//! Function and closure heads for the Dafny adapter.

use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use tree_sitter::Node;

use super::shapes::{COMMENT_KINDS, is_type_kind, pattern_names, sole_call_in};
use crate::hooks::{
    Cx, FunctionHead, child_by_field, first_named_child_matching, named_children, pos_of,
    preceding_comments, text, trimmed,
};

fn primitive_of(type_node: Node<'_>) -> PrimitiveKind {
    match type_node.kind() {
        "string_type" | "char_type" => PrimitiveKind::Str,
        "bool_type" => PrimitiveKind::Bool,
        "int_type" | "nat_type" => PrimitiveKind::Int,
        "real_type" | "float_type" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

pub(crate) fn is_bool_literal(node: Node<'_>) -> bool {
    node.kind() == "boolean_literal"
        || (node.kind() == "literal_expression" && node.named_child(0).is_some_and(is_bool_literal))
}

fn param_of<'s>(cx: &Cx<'_, 's>, formal: Node<'_>) -> Option<Param<'s>> {
    let name_node = child_by_field(formal, "name")?;
    let name = trimmed(cx, name_node);
    let mut type_node = None;
    let mut default_node = None;
    for child in named_children(formal) {
        if child.id() == name_node.id() {
            continue;
        }
        if type_node.is_none() && is_type_kind(child.kind()) {
            type_node = Some(child);
        } else {
            default_node = Some(child);
        }
    }
    Some(Param {
        name,
        ty: type_node.map(|ty| trimmed(cx, ty)),
        primitive_kind: type_node.map(primitive_of).unwrap_or(PrimitiveKind::Other),
        has_default: default_node.is_some(),
        bool_default: default_node.is_some_and(is_bool_literal),
        keyword_only: false,
        is_variadic: false,
    })
}

fn lambda_params<'s>(cx: &Cx<'_, 's>, lambda: Node<'_>) -> Vec<Param<'s>> {
    let mut params = Vec::new();
    for child in named_children(lambda) {
        match child.kind() {
            "formal" => {
                if let Some(param) = param_of(cx, child) {
                    params.push(param);
                }
            }
            "case_pattern" => {
                let mut names = Vec::new();
                pattern_names(cx, child, &mut names);
                params.extend(names.into_iter().map(|(name, _)| Param {
                    name,
                    ty: None,
                    primitive_kind: PrimitiveKind::Other,
                    has_default: false,
                    bool_default: false,
                    keyword_only: false,
                    is_variadic: false,
                }));
            }
            "identifier" => params.push(Param {
                name: text(cx, child),
                ty: None,
                primitive_kind: PrimitiveKind::Other,
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            }),
            _ => {}
        }
    }
    params
}

fn formals_params<'s>(cx: &Cx<'_, 's>, formals: Node<'_>) -> Vec<Param<'s>> {
    named_children(formals)
        .into_iter()
        .filter(|child| child.kind() == "formal")
        .filter_map(|formal| param_of(cx, formal))
        .collect()
}

fn is_dafny_deprecated(doc: &[&str]) -> bool {
    doc.iter()
        .any(|line| line.to_lowercase().contains("deprecat"))
}

pub(crate) fn dafny_visibility(name: &str) -> Visibility {
    if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Exported
    }
}

fn return_type_of<'s>(
    cx: &Cx<'_, 's>,
    node: Node<'_>,
    formals: Option<Node<'_>>,
) -> Option<&'s str> {
    let mut seen_formals = 0;
    for child in named_children(node) {
        if Some(child.id()) == formals.map(|form| form.id()) {
            seen_formals += 1;
            if seen_formals == 1 && node.kind() == "method_decl" {
                continue;
            }
        }
        if is_type_kind(child.kind()) {
            return Some(trimmed(cx, child));
        }
        if child.kind() == "formals" && seen_formals >= 2 {
            return Some(trimmed(cx, child));
        }
    }
    None
}

pub(crate) fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let name_node = child_by_field(n, "name")?;
    let name = trimmed(cx, name_node);
    if name.is_empty() {
        return None;
    }
    let (doc, doc_pos) = preceding_comments(cx, n, COMMENT_KINDS, &["attribute"]);
    let formals = first_named_child_matching(n, &["formals"]);
    let params = formals
        .map(|form| formals_params(cx, form))
        .unwrap_or_default();
    let is_trait_item = cx.ancestors().any(|a| a.kind() == "trait_decl");
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name,
        name_pos: pos_of(name_node),
        visibility: dafny_visibility(name),
        signature_forced: false,
        entry_point: name == "Main",
        in_test: false,
        is_overload: false,
        is_deprecated: is_dafny_deprecated(&doc),
        is_decorated: named_children(n)
            .iter()
            .any(|child| child.kind() == "attribute"),
        is_trait_item,
        region: None,
        params,
        receiver: None,
        return_type: return_type_of(cx, n, formals),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: sole_call_in(n),
    })
}

pub(crate) fn closure_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> FunctionHead<'s> {
    FunctionHead {
        shadow_params: Vec::new(),
        name: "",
        name_pos: pos_of(n),
        visibility: Visibility::Private,
        signature_forced: false,
        entry_point: false,
        in_test: false,
        is_overload: false,
        is_deprecated: false,
        is_decorated: false,
        is_trait_item: false,
        region: None,
        params: lambda_params(cx, n),
        receiver: None,
        return_type: None,
        missing_annotations: Vec::new(),
        doc: Vec::new(),
        doc_pos: None,
        sole_call: sole_call_in(n),
    }
}
