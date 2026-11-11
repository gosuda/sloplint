use sloplint_core::facts::{Param, PrimitiveKind};
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, first_of_kind, named_children, text};

pub(crate) fn is_parameter(c: &Node<'_>) -> bool {
    c.kind() == "parameter"
}

pub(crate) fn is_let_binding(c: &Node<'_>) -> bool {
    c.kind() == "let_binding"
}

fn is_true_or_false(cx: &Cx<'_, '_>, d: Node<'_>) -> bool {
    text(cx, d) == "true" || text(cx, d) == "false"
}

pub(crate) fn ocaml_param<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<Param<'s>> {
    let pat = child_by_field(p, "pattern")?;
    if pat.kind() == "typed_pattern" {
        let name = child_by_field(pat, "pattern")
            .map(|q| text(cx, q))
            .unwrap_or("");
        let ty = child_by_field(pat, "type").map(|t| text(cx, t));
        return Some(param_at(cx, p, name, ty));
    }
    let ty = child_by_field(p, "type").map(|t| text(cx, t));
    Some(param_at(cx, p, text(cx, pat), ty))
}

fn primitive_of(ty: Option<&str>) -> PrimitiveKind {
    match ty {
        Some("bool") => PrimitiveKind::Bool,
        Some("string") => PrimitiveKind::Str,
        Some("int") => PrimitiveKind::Int,
        Some("float") => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn param_at<'s>(cx: &Cx<'_, 's>, p: Node<'_>, name: &'s str, ty: Option<&'s str>) -> Param<'s> {
    let default = child_by_field(p, "default");
    let constant = default.is_some_and(|d| is_true_or_false(cx, d));
    let primitive_kind = if constant {
        PrimitiveKind::Bool
    } else {
        primitive_of(ty)
    };
    Param {
        name,
        ty,
        primitive_kind,
        has_default: default.is_some(),
        bool_default: constant,
        keyword_only: false,
        is_variadic: false,
    }
}

pub(crate) fn ocaml_params<'s>(cx: &Cx<'_, 's>, holder: Node<'_>) -> Vec<Param<'s>> {
    named_children(holder)
        .into_iter()
        .filter(is_parameter)
        .filter_map(|p| ocaml_param(cx, p))
        .collect()
}

pub(crate) fn missing_of<'s>(params: &[Param<'s>], return_type: Option<&str>) -> Vec<&'s str> {
    let mut missing: Vec<&'s str> = params
        .iter()
        .filter(|p| p.ty.is_none() && p.name != "()")
        .map(|p| p.name)
        .collect();
    if return_type.is_none() {
        missing.push("return");
    }
    missing
}

pub(crate) fn spec_signature<'s>(
    cx: &Cx<'_, 's>,
    n: Node<'_>,
) -> (Vec<Param<'s>>, Option<&'s str>) {
    let Some(mut current) = child_by_field(n, "type") else {
        return (Vec::new(), None);
    };
    if current.kind() == "polymorphic_type" {
        return (Vec::new(), Some(text(cx, current)));
    }
    let mut params = Vec::new();
    loop {
        if current.kind() != "function_type" {
            return (params, Some(text(cx, current)));
        }
        let domain = child_by_field(current, "domain");
        let labeled = domain.is_some_and(|d| d.kind() == "labeled_argument_type");
        let (name, ty) = match domain {
            Some(found) if labeled => (
                label_of(cx, found),
                child_by_field(found, "type").map(|t| text(cx, t)),
            ),
            found => ("", found.map(|d| text(cx, d))),
        };
        params.push(Param {
            name,
            ty,
            primitive_kind: primitive_of(ty),
            has_default: false,
            bool_default: false,
            keyword_only: false,
            is_variadic: false,
        });
        match child_by_field(current, "codomain") {
            Some(next) => current = next,
            None => return (params, None),
        }
    }
}

fn label_of<'s>(cx: &Cx<'_, 's>, domain: Node<'_>) -> &'s str {
    first_of_kind(domain, &["label_name"])
        .map(|l| text(cx, l))
        .unwrap_or("")
}
