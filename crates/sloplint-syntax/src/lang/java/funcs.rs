//! Java function and parameter heads.

use sloplint_core::facts::{Param, PrimitiveKind, Visibility};
use sloplint_core::role::CodeRole;
use tree_sitter::Node;

use super::{
    COMMENTS, TEST_ANNOTATIONS, annotation_names, code_children, container_exports, first_code,
    has_keyword, modifiers_of,
};
use crate::hooks::{
    Container, Cx, FunctionHead, attributed_region, child_by_field, find_named_child,
    first_of_kind, only_named_child_matching, pos_of, preceding_comments, slice, text,
};

fn primitive_of(ty: &str) -> PrimitiveKind {
    match ty.trim() {
        "String" => PrimitiveKind::Str,
        "boolean" | "Boolean" => PrimitiveKind::Bool,
        "byte" | "short" | "int" | "long" | "char" | "Byte" | "Short" | "Integer" | "Long"
        | "Character" => PrimitiveKind::Int,
        "float" | "double" | "Float" | "Double" => PrimitiveKind::Float,
        _ => PrimitiveKind::Other,
    }
}

fn spread_param<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<Param<'s>> {
    let ty_node = find_named_child(p, |child| {
        !matches!(
            child.kind(),
            "modifiers" | "annotation" | "marker_annotation" | "variable_declarator"
        )
    })?;
    let declarator = first_of_kind(p, &["variable_declarator"])?;
    let name = text(cx, child_by_field(declarator, "name")?);
    let ty = match first_of_kind(p, &["..."]) {
        Some(dots) => slice(cx.src, ty_node.start_byte(), dots.end_byte()),
        None => text(cx, ty_node),
    };
    Some(Param {
        name,
        ty: Some(ty),
        primitive_kind: primitive_of(ty),
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: true,
    })
}

fn formal_param<'s>(cx: &Cx<'_, 's>, p: Node<'_>) -> Option<Param<'s>> {
    match p.kind() {
        "formal_parameter" => {
            let name = text(cx, child_by_field(p, "name")?);
            let type_node = child_by_field(p, "type")?;
            let ty = match child_by_field(p, "dimensions") {
                Some(dims) => slice(cx.src, type_node.start_byte(), dims.end_byte()),
                None => text(cx, type_node),
            };
            Some(Param {
                name,
                ty: Some(ty),
                primitive_kind: primitive_of(ty),
                has_default: false,
                bool_default: false,
                keyword_only: false,
                is_variadic: false,
            })
        }
        "spread_parameter" => spread_param(cx, p),
        _ => None,
    }
}
fn formal_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>) -> Vec<Param<'s>> {
    code_children(list)
        .into_iter()
        .filter_map(|p| formal_param(cx, p))
        .collect()
}
fn anon_param<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Param<'s> {
    Param {
        name: text(cx, n),
        ty: None,
        primitive_kind: PrimitiveKind::Other,
        has_default: false,
        bool_default: false,
        keyword_only: false,
        is_variadic: false,
    }
}

fn inferred_params<'s>(cx: &Cx<'_, 's>, list: Node<'_>, out: &mut Vec<Param<'s>>) {
    for child in code_children(list) {
        if child.kind() == "identifier" {
            out.push(anon_param(cx, child));
        }
    }
}

pub(crate) fn lambda_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    if n.kind() != "lambda_expression" {
        return None;
    }
    let mut params = Vec::new();
    if let Some(list) = child_by_field(n, "parameters") {
        match list.kind() {
            "formal_parameters" => params.extend(formal_params(cx, list)),
            "inferred_parameters" => inferred_params(cx, list, &mut params),
            "identifier" => params.push(anon_param(cx, list)),
            _ => {}
        }
    }
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
        return_type: None,
        missing_annotations: Vec::new(),
        doc: Vec::new(),
        doc_pos: None,
        sole_call: None,
    })
}

fn statement_call(statement: Node<'_>) -> Option<usize> {
    if !matches!(
        statement.kind(),
        "expression_statement" | "return_statement"
    ) {
        return None;
    }
    let expression = first_code(statement)?;
    (expression.kind() == "method_invocation").then(|| expression.id())
}

fn sole_call_of(n: Node<'_>, body: Node<'_>) -> Option<usize> {
    if n.kind() != "method_declaration" {
        return None;
    }
    let only = only_named_child_matching(body, |c| c.kind() != "comment")?;
    statement_call(only)
}

fn is_nested_method(cx: &Cx<'_, '_>) -> bool {
    cx.ancestors().take(8).any(|parent| {
        matches!(
            parent.kind(),
            "method_declaration"
                | "constructor_declaration"
                | "compact_constructor_declaration"
                | "block"
                | "constructor_body"
                | "lambda_expression"
        )
    })
}

fn is_string_element(cx: &Cx<'_, '_>, ty: Node<'_>) -> bool {
    matches!(text(cx, ty), "String" | "java.lang.String")
}

fn spread_element<'n>(p: Node<'n>) -> Option<Node<'n>> {
    find_named_child(p, |child| {
        !matches!(
            child.kind(),
            "modifiers" | "annotation" | "marker_annotation" | "variable_declarator" | "..."
        )
    })
}

fn formal_is_string_array(cx: &Cx<'_, '_>, first: Node<'_>) -> bool {
    let Some(ty) = child_by_field(first, "type") else {
        return false;
    };
    if child_by_field(first, "dimensions").is_some() {
        return is_string_element(cx, ty);
    }
    if ty.kind() == "array_type" {
        return child_by_field(ty, "element").is_some_and(|element| is_string_element(cx, element));
    }
    false
}

fn has_string_array_param<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> bool {
    let Some(list) = child_by_field(n, "parameters") else {
        return false;
    };
    let mut cursor = list.walk();
    let mut params = list
        .named_children(&mut cursor)
        .filter(|child| matches!(child.kind(), "formal_parameter" | "spread_parameter"));
    let Some(first) = params.next() else {
        return false;
    };
    if params.next().is_some() {
        return false;
    }
    if first.kind() == "spread_parameter" {
        return spread_element(first).is_some_and(|ty| is_string_element(cx, ty));
    }
    formal_is_string_array(cx, first)
}

pub(crate) fn function_head<'s>(cx: &Cx<'_, 's>, n: Node<'_>) -> Option<FunctionHead<'s>> {
    let body = child_by_field(n, "body");
    let name = child_by_field(n, "name")?;
    let annotations = annotation_names(cx, n);
    let in_trait = matches!(cx.innermost_container(), Some(Container::Trait { .. }));
    let private = has_keyword(n, "private") || is_nested_method(cx);
    let visibility = if private {
        Visibility::Private
    } else if has_keyword(n, "public") || has_keyword(n, "protected") || in_trait {
        Visibility::Exported
    } else {
        Visibility::Restricted
    };
    let visibility = if visibility == Visibility::Exported && !in_trait && !container_exports(cx) {
        Visibility::Restricted
    } else {
        visibility
    };
    let tested = annotations.iter().any(|a| TEST_ANNOTATIONS.contains(a));
    let benched = !tested && annotations.contains(&"Benchmark");
    let name_text = text(cx, name);
    let entry_point =
        name_text == "main" && has_keyword(n, "static") && has_string_array_param(cx, n);
    let (doc, doc_pos) = preceding_comments(cx, n, COMMENTS, &[]);
    Some(FunctionHead {
        shadow_params: Vec::new(),
        name: name_text,
        name_pos: pos_of(name),
        visibility,
        signature_forced: annotations.contains(&"Override"),
        entry_point,
        in_test: tested || benched,
        is_overload: false,
        is_deprecated: annotations.contains(&"Deprecated"),
        is_decorated: !annotations.is_empty(),
        is_trait_item: in_trait,
        region: tested
            .then(|| attributed_region(n, modifiers_of(n), CodeRole::Test))
            .or_else(|| benched.then(|| attributed_region(n, modifiers_of(n), CodeRole::Bench))),
        receiver: None,
        params: child_by_field(n, "parameters")
            .map_or_else(Vec::new, |list| formal_params(cx, list)),
        return_type: child_by_field(n, "type").map(|ty| text(cx, ty)),
        missing_annotations: Vec::new(),
        doc,
        doc_pos,
        sole_call: body.and_then(|found| sole_call_of(n, found)),
    })
}
