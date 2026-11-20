//! Lean definition, closure, parameter, and visibility heads.

use sloplint_core::facts::{Param, Pos, PrimitiveKind, Visibility};
use tree_sitter::Node;

use super::{COMMENT_KINDS, declaration_of, field_term};
use crate::hooks::{
    Cx, FunctionHead, child_by_field, named_children, pos_of, preceding_comments, text, trimmed,
};

pub(crate) const BINDER_KINDS: &[&str] = &[
    "explicit_binder",
    "implicit_binder",
    "strict_implicit_binder",
    "instance_binder",
];

pub(crate) fn lean_primitive_kind(ty: Option<&str>) -> PrimitiveKind {
    match ty {
        Some("Bool" | "bool") => PrimitiveKind::Bool,
        Some("String" | "string") => PrimitiveKind::Str,
        Some("Nat" | "Int" | "UInt8" | "UInt16" | "UInt32" | "UInt64" | "USize") => {
            PrimitiveKind::Int
        }
        Some("Float") => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn binder_names<'s>(cx: &Cx<'_, 's>, binder: Node<'_>, out: &mut Vec<&'s str>) {
    let mut cursor = binder.walk();
    for name in binder.children_by_field_name("name", &mut cursor) {
        if name.kind() == "identifier" {
            out.push(text(cx, name));
        } else {
            // `⟨x, y⟩` destructuring or a `_` hole: the identifiers inside
            // bind names, a hole binds nothing.
            let mut stack = vec![name];
            while let Some(node) = stack.pop() {
                if node.kind() == "identifier" {
                    out.push(text(cx, node));
                } else {
                    stack.extend(named_children(node));
                }
            }
        }
    }
}

fn push_binder_params<'s>(cx: &Cx<'_, 's>, binder: Node<'_>, out: &mut Vec<Param<'s>>) {
    let ty = field_term(binder, "type").map(|ty| trimmed(cx, ty));
    let primitive_kind = lean_primitive_kind(ty);
    let default = field_term(binder, "default");
    let constant = default.is_some_and(|d| {
        let body = trimmed(cx, d)
            .trim_start_matches(':')
            .trim_start_matches('=')
            .trim();
        body == "true" || body == "false"
    });
    let mut names = Vec::new();
    binder_names(cx, binder, &mut names);
    for name in names {
        out.push(Param {
            name,
            ty,
            primitive_kind,
            has_default: default.is_some(),
            bool_default: constant,
            keyword_only: false,
            is_variadic: false,
        });
    }
}

/// Parameters from one `binders` node or from a lone binder: bracketed
/// binders carry name and type fields, bare identifiers (the `fun`/`∀`
/// shape) are untyped single names.
fn params_from<'s>(cx: &Cx<'_, 's>, holder: Node<'_>, out: &mut Vec<Param<'s>>) {
    for child in named_children(holder) {
        if BINDER_KINDS.contains(&child.kind()) {
            push_binder_params(cx, child, out);
        } else if child.kind() == "tuple_binder" || child.kind() == "anon_ctor_binder" {
            push_destructured_params(cx, child, out);
        } else if child.kind() == "identifier" {
            out.push(Param {
                name: text(cx, child),
                ty: None,
                primitive_kind: PrimitiveKind::Other,
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            });
        }
    }
}

fn push_destructured_params<'s>(cx: &Cx<'_, 's>, holder: Node<'_>, out: &mut Vec<Param<'s>>) {
    let mut stack = vec![holder];
    while let Some(node) = stack.pop() {
        if node.kind() == "identifier" {
            out.push(Param {
                name: text(cx, node),
                ty: None,
                primitive_kind: PrimitiveKind::Other,
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            });
        } else {
            stack.extend(named_children(node));
        }
    }
}

pub(crate) fn lean_params<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Vec<Param<'s>> {
    let mut out = Vec::new();
    for child in named_children(n) {
        if child.kind() == "binders" {
            params_from(cx, child, &mut out);
        } else if BINDER_KINDS.contains(&child.kind()) {
            push_binder_params(cx, child, &mut out);
        }
    }
    out
}

fn has_private_modifier<'s>(cx: &Cx<'_, 's>, decl: Node<'_>) -> bool {
    named_children(decl)
        .into_iter()
        .any(|child| child.kind() == "decl_modifiers" && text(cx, child).contains("private"))
}

fn declaration_text<'s>(cx: &Cx<'_, 's>, decl: Node<'_>, kind: &str) -> Option<&'s str> {
    named_children(decl)
        .into_iter()
        .find(|child| child.kind() == kind)
        .map(|child| text(cx, child))
}

pub(crate) fn lean_visibility<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Visibility {
    if n.kind() == "example" || n.kind() == "where_aux_def" {
        return Visibility::Private;
    }
    if let Some(decl) = declaration_of(n, cx)
        && has_private_modifier(cx, decl)
    {
        return Visibility::Private;
    }
    let mut current = cx.parent();
    while let Some(node) = current {
        if super::FUNCTION_KINDS.contains(&node.kind()) && node.id() != n.id() {
            return Visibility::Private;
        }
        current = cx.parent_of(node);
    }
    Visibility::Exported
}

fn lean_doc<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> (Vec<&'s str>, Option<Pos>) {
    // Doc comments attach as siblings of the `declaration` wrapper, not of
    // the definition itself; anchor on the wrapper when there is one.
    if let Some(decl) = declaration_of(n, cx) {
        preceding_comments(cx, decl, COMMENT_KINDS, &[])
    } else {
        preceding_comments(cx, n, COMMENT_KINDS, &[])
    }
}

fn is_deprecated_attrs<'s>(cx: &Cx<'_, 's>, decl: Option<Node<'_>>) -> bool {
    decl.is_some_and(|decl| {
        declaration_text(cx, decl, "attributes").is_some_and(|attrs| attrs.contains("deprecated"))
    })
}

fn is_decorated_attrs(decl: Option<Node<'_>>) -> bool {
    decl.is_some_and(|decl| {
        named_children(decl)
            .into_iter()
            .any(|child| child.kind() == "attributes")
    })
}

fn sole_app(body: Node<'_>) -> Option<Node<'_>> {
    let mut inner = body;
    loop {
        if inner.kind() == "paren" {
            let mut cursor = inner.walk();
            let mut named = inner.named_children(&mut cursor);
            let only = named.next()?;
            if named.next().is_some() {
                return None;
            }
            inner = only;
            continue;
        }
        return (inner.kind() == "app").then_some(inner);
    }
}

pub(crate) fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> FunctionHead<'s> {
    let name_node = child_by_field(n, "name");
    let name = name_node.map_or("", |name| trimmed(cx, name));
    let params = lean_params(cx, n);
    let return_type = field_term(n, "type").map(|ty| trimmed(cx, ty));
    let missing_annotations: Vec<&'s str> = params
        .iter()
        .filter(|param| param.ty.is_none())
        .map(|param| param.name)
        .chain(return_type.is_none().then_some("return"))
        .collect();
    let body = field_term(n, "body");
    let decl = declaration_of(n, cx);
    let (doc, doc_pos) = lean_doc(cx, n);
    FunctionHead {
        shadow_params: Vec::new(),
        name,
        name_pos: name_node.map_or_else(|| pos_of(n), pos_of),
        visibility: lean_visibility(cx, n),
        signature_forced: false,
        entry_point: name == "main",
        in_test: false,
        is_overload: false,
        is_deprecated: is_deprecated_attrs(cx, decl),
        is_decorated: is_decorated_attrs(decl),
        is_trait_item: false,
        region: None,
        params,
        receiver: None,
        return_type,
        missing_annotations,
        doc,
        doc_pos,
        sole_call: body.and_then(sole_app).map(|app| app.id()),
    }
}

pub(crate) fn closure_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    if n.kind() != "fun" {
        return None;
    }
    let mut params = Vec::new();
    for child in named_children(n) {
        if child.kind() == "binders" {
            params_from(cx, child, &mut params);
        } else if BINDER_KINDS.contains(&child.kind()) {
            push_binder_params(cx, child, &mut params);
        } else if child.kind() == "identifier" {
            params.push(Param {
                name: text(cx, child),
                ty: None,
                primitive_kind: PrimitiveKind::Other,
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            });
        }
    }
    let missing: Vec<&'s str> = params
        .iter()
        .filter(|param| param.ty.is_none())
        .map(|param| param.name)
        .collect();
    let body = field_term(n, "body");
    Some(FunctionHead {
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
        params,
        receiver: None,
        return_type: field_term(n, "type").map(|ty| trimmed(cx, ty)),
        missing_annotations: missing,
        doc: Vec::new(),
        doc_pos: None,
        sole_call: body.and_then(sole_app).map(|app| app.id()),
    })
}
